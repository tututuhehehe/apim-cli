//! 回读 **pi 现场**：它现在真正在用哪个 provider / 哪把密钥。
//!
//! ★ 只认这里读出来的东西：`settings.json` 的 `defaultProvider` 指向哪个 provider，
//! `models.json` 里那个 provider 的 `apiKey` 是不是和 apim 的密钥一模一样。
//! 用户手改了 pi 的配置（换 token、把 `defaultProvider` 切回内置 provider、删掉那个条目），
//! ★ 会跟着变 —— apim 自己不记「上次导入了谁」。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{agent_dir, config};
use crate::clients::{ActiveProvider, normalize_base_url};
use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// 读 pi 的配置（agent 目录 + 项目级覆盖），回答「它现在默认用哪个 provider」。
///
/// 读不到 / 没写 `defaultProvider` / 那个 provider 不在 `models.json` 里 / 那个 provider
/// 根本没凭据 → `None`（等价于「pi 现在没在用 apim 写进去的东西」）。
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
    let entry = models.get("providers")?.get(&provider_key)?;
    let base_url = entry.get("baseUrl").and_then(Value::as_str)?.to_string();
    // 根本没有 `apiKey`（或不是字符串）= pi 眼里这个 provider 没有凭据可用
    // （`provider-composer` 会直接报 no authentication method configured）→ 用不起来，不算在用。
    // 有值但是在运行时求值的（`$ENV` / `!cmd`）→ 拿不到明文，退化成比 base_url。
    let token = plaintext_api_key(entry.get("apiKey").and_then(Value::as_str)?).map(str::to_string);
    Some(ActiveProvider {
        provider_key,
        base_url,
        token,
    })
}

/// 当前工作目录下的项目级设置（pi 会把它深合并到 agent 目录设置之上）。
fn project_settings_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(".pi").join(config::SETTINGS_FILE))
}

/// pi 的 `apiKey` 可以是明文，也可以是 `$NAME` / `${NAME}` / `!command`（请求时才求值，
/// 值不在文件里）。只有**明文**才能拿来跟 apim 的密钥对账；取不到明文就返回 `None`，
/// 调用方退化成比 base_url。
///
/// `$$` / `$!` 是 pi 的转义写法（分别表示字面上的 `$` 和开头的 `!`），按字面值比：
/// 一个 `$$sk-1` 的真实值就是 `$sk-1`（转义只吃掉一个字符）。
fn plaintext_api_key(raw: &str) -> Option<&str> {
    if raw.starts_with("$$") || raw.starts_with("$!") {
        return Some(&raw[1..]);
    }
    if raw.starts_with('$') || raw.starts_with('!') {
        return None;
    }
    Some(raw)
}

/// 现场 → apim 密钥 id。
///
/// apim 写的 provider 键一律是 `apim-<厂商id>`（见模块文档），所以先剥前缀拿到厂商 id，
/// 再要求 **apiKey 一模一样**（或者只有 `$ENV` / `!command` 这类取不到明文时，退化成比 base_url）。
pub fn active_key_ids(
    dir: Option<&Path>,
    keys: &[KeyEntry],
    recipes: &HashMap<String, Recipe>,
) -> Vec<String> {
    let Some(active) = active_provider(dir) else {
        return Vec::new();
    };
    let Some(provider_id) = active.provider_key.strip_prefix("apim-") else {
        // 内置 provider（deepseek / openai …）或用户手写的：不是 apim 写的
        return Vec::new();
    };
    keys.iter()
        .filter(|key| key.provider == provider_id)
        .filter(|key| match &active.token {
            Some(token) => key.token == *token,
            None => recipes.get(&key.provider).is_some_and(|recipe| {
                normalize_base_url(&recipe.base_url) == normalize_base_url(&active.base_url)
            }),
        })
        .map(KeyEntry::id)
        .collect()
}
