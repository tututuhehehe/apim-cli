//! 回读 **pi 现场**：它现在真正用着哪把密钥。
//!
//! ★ 只认这里读出来的东西：`settings.json` 的 `defaultProvider`（项目级设置优先）指向哪个
//! provider，那个 provider 的凭据是不是和 apim 的密钥一模一样。用户手改了 pi 的配置
//! （换 token、把 `defaultProvider` 切走、删掉那个条目），★ 会跟着变 —— apim 自己不记台账。
//!
//! 凭据按 **pi 自己的优先级**找（`--api-key` 是运行时的，文件里看不到）：
//!
//! 1. `<agent-dir>/auth.json` —— `/login` 存下来的凭据（`{ "<providerId>": { "type": "api_key",
//!    "key": "…" } }`，也可能是 `type: "oauth"` 的订阅凭据）。**只读**：这个文件里还有你的
//!    订阅 token，apim 一个字都不写（理由见 AGENTS.md 约定 14）。
//! 2. `models.json` 里那个 provider 的 `apiKey`（apim 自己写的就是这里）。
//!
//! 读不到明文（`$ENV` / `!command`，请求时才求值）时退化成比 `base_url`；
//! 拿到的凭据是订阅（OAuth）或两处都没有凭据时，直接算「不是在用 apim 的 key」。
//!
//! **provider 键的 `apim-` 前缀只约束「写」，不约束「认」**：用户手写的 provider
//! （叫什么都行）只要用着 apim 里这把 key，就是「在用」。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{agent_dir, config};
use crate::clients::{ActiveProvider, normalize_base_url};
use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// 这个 provider 现在的凭据，按我们能不能拿来对账分三类。
enum Credential {
    /// 明文密钥（`$$` / `$!` 转义写法已还原成字面值）。
    Plain(String),
    /// 有凭据，但值要运行时才求值（`$ENV` / `!command`）：看不见，只能比 base_url。
    Runtime,
    /// 不是 apim 的 key：订阅（OAuth）凭据，或者压根没有凭据。
    NotOurs,
}

/// 读 pi 的配置（agent 目录 + 项目级覆盖），回答「它现在默认用哪个 provider、哪把密钥」。
///
/// 读不到 / 没写 `defaultProvider` / 那个 provider 不在 `models.json` 里 / 凭据不是 apim 的
/// → `None`（等价于「pi 现在没在用 apim 里的 key」）。
pub fn active_provider(dir: Option<&Path>) -> Option<ActiveProvider> {
    active_provider_with(dir, project_settings_path())
}

/// `active_provider` 的可测版：项目级设置路径由调用方给（`None` = 没有项目设置）。
pub(super) fn active_provider_with(
    dir: Option<&Path>,
    project_settings: Option<PathBuf>,
) -> Option<ActiveProvider> {
    let dir: PathBuf = match dir {
        Some(dir) => dir.to_path_buf(),
        None => agent_dir(),
    };
    let settings = config::read(&config::settings_path(&dir)).ok()?;
    // 项目级 `.pi/settings.json` 优先：pi 把项目设置深合并到 agent 目录设置之上
    // （只有项目被信任时才生效 —— 未信任时我们会保守地少打一个 ★，不会多打）。
    let project = project_settings.and_then(|path| config::read(&path).ok());
    let provider_key = project
        .as_ref()
        .and_then(|map| map.get("defaultProvider"))
        .or_else(|| settings.get("defaultProvider"))
        .and_then(Value::as_str)?
        .to_string();
    let models = config::read(&config::models_path(&dir)).ok()?;
    // models.json 里可能没这个 provider（比如 pi 内置的 deepseek + 你在 auth.json 里存了 key）：
    // 那种情况下只剩一个地址信息，依然根据凭据对账。
    let entry = models
        .get("providers")
        .and_then(|map| map.get(&provider_key));
    let base_url = entry
        .and_then(|entry| entry.get("baseUrl"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let token = match credential_for(&dir, &provider_key, entry) {
        Credential::Plain(token) => Some(token),
        // 看不见的凭据（`$ENV` / `!command`）：退化成比 base_url；连地址都不知道就只能放弃
        Credential::Runtime => {
            base_url.as_ref()?;
            None
        }
        // 订阅凭据 / 没有凭据：pi 用的根本不是 apim 里的 key
        Credential::NotOurs => return None,
    };
    Some(ActiveProvider {
        provider_key,
        base_url: base_url.unwrap_or_default(),
        token,
    })
}

/// 按 pi 的优先级取这个 provider 的凭据：`auth.json` 先，`models.json` 的 `apiKey` 后。
fn credential_for(dir: &Path, provider_key: &str, models_entry: Option<&Value>) -> Credential {
    if let Some(credential) = auth_credential(dir, provider_key) {
        return credential;
    }
    match models_entry
        .and_then(|entry| entry.get("apiKey"))
        .and_then(Value::as_str)
    {
        Some(raw) => plaintext(raw),
        None => Credential::NotOurs,
    }
}

/// `auth.json` 里这个 provider 的凭据（文件不存在 / 读坏了都当没有，绝不影响检测）。
fn auth_credential(dir: &Path, provider_key: &str) -> Option<Credential> {
    let auth = config::read(&dir.join(config::AUTH_FILE)).ok()?;
    let value = auth.get(provider_key)?;
    // 订阅凭据（OAuth）：pi 用它登的订阅，不是 apim 的 key
    if value.get("type").and_then(Value::as_str) != Some("api_key") {
        return Some(Credential::NotOurs);
    }
    // `key` 缺失是 pi 允许的（等于没有可用的 key），与 `$ENV` 一样按「看不见」处理
    Some(match value.get("key").and_then(Value::as_str) {
        Some(raw) => plaintext(raw),
        None => Credential::Runtime,
    })
}

/// 明文才算「能对账」：`$NAME` / `${NAME}` / `!command` 要运行时才求值，值不在文件里。
///
/// `$$` / `$!` 是 pi 的转义写法（分别表示字面上的 `$` 和开头的 `!`），按字面值比：
/// 一个 `$$sk-1` 的真实值就是 `$sk-1`（转义只吃掉一个字符）。
fn plaintext(raw: &str) -> Credential {
    if raw.starts_with("$$") || raw.starts_with("$!") {
        return Credential::Plain(raw[1..].to_string());
    }
    if raw.starts_with('$') || raw.starts_with('!') {
        return Credential::Runtime;
    }
    Credential::Plain(raw.to_string())
}

/// 当前工作目录下的项目级设置（pi 会把它深合并到 agent 目录设置之上）。
fn project_settings_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(".pi").join(config::SETTINGS_FILE))
}

/// 现场 → apim 密钥 id。
///
/// 能看见明文时**只比密钥**：谁的 token 和 pi 正在用的这把一样，谁就是在用的那把
/// （同一个 token 挂在多处就都打 ★，这是实话）。看不见明文时退化成比 `base_url`。
pub fn active_key_ids(
    dir: Option<&Path>,
    keys: &[KeyEntry],
    recipes: &HashMap<String, Recipe>,
) -> Vec<String> {
    let Some(active) = active_provider(dir) else {
        return Vec::new();
    };
    keys.iter()
        .filter(|key| match &active.token {
            Some(token) => key.token == *token,
            None => recipes.get(&key.provider).is_some_and(|recipe| {
                normalize_base_url(&recipe.base_url) == normalize_base_url(&active.base_url)
            }),
        })
        .map(KeyEntry::id)
        .collect()
}
