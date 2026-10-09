//! 回读 **pi 现场**：它配置里在用的凭据都有哪些（★ 角标）。
//!
//! pi 为什么没有「唯一激活的 provider」、两张表各看什么、`$NAME` / `!command` 怎么退化、
//! `type: "oauth"` 为什么不算、前缀为什么只约束写 —— 都在 `docs/clients/pi.md` 的
//! 「怎么认出正在用的密钥」，**改这个文件之前先读它**。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{agent_dir, config};
use crate::clients::normalize_base_url;
use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// pi 配置里的一份凭据（能拿来和 apim 对账的那种）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    /// 明文密钥（`$$` / `$!` 转义写法已还原成字面值）；`None` = 要运行时才求值，看不见。
    pub token: Option<String>,
    /// 该 provider 的地址（只有 `models.json` 里有），供看不见明文时按地址比。
    pub base_url: Option<String>,
}

/// 读 pi 的两张配置，列出**所有**能被 pi 用起来的凭据。
pub fn credentials_in_use(dir: Option<&Path>) -> Vec<Credential> {
    let dir: PathBuf = match dir {
        Some(dir) => dir.to_path_buf(),
        None => agent_dir(),
    };
    let mut found = Vec::new();

    // 1) auth.json：/login 存的 API key（订阅凭据不是我们的，跳过）
    if let Ok(auth) = config::read(&dir.join(config::AUTH_FILE)) {
        for (_, value) in auth.iter() {
            if value.get("type").and_then(Value::as_str) != Some("api_key") {
                continue;
            }
            // 没有 `key` = pi 眼里也没有可用凭据；`$ENV` / `!cmd` → 看不见明文
            if let Some(raw) = value.get("key").and_then(Value::as_str) {
                let credential = plaintext(raw, None);
                if credential.token.is_some() {
                    found.push(credential);
                }
            }
        }
    }

    // 2) models.json：每个 **带 apiKey** 的 provider（带上它的 base_url 供退化比较）
    //
    //    没有 apiKey 的 provider 不算（它可能靠自己的环境变量/登录用，那个值我们看不见；
    //    只看地址会把「地址相同的其它密钥」误标成在用）。
    if let Ok(models) = config::read(&config::models_path(&dir))
        && let Some(providers) = models.get("providers").and_then(Value::as_object)
    {
        for (_, entry) in providers.iter() {
            let Some(raw) = entry.get("apiKey").and_then(Value::as_str) else {
                continue;
            };
            let base_url = entry
                .get("baseUrl")
                .and_then(Value::as_str)
                .map(str::to_string);
            let credential = plaintext(raw, base_url);
            if credential.token.is_some() || credential.base_url.is_some() {
                found.push(credential);
            }
        }
    }

    found
}

/// 现场 → apim 密钥 id：谁的 token 和 pi 现在配着的某把一样，谁就是在用的那把
/// （同一个 token 挂在多处就都打 ★，这是实话）；看不见明文时按地址比。
pub fn active_key_ids(
    dir: Option<&Path>,
    keys: &[KeyEntry],
    recipes: &HashMap<String, Recipe>,
) -> Vec<String> {
    let credentials = credentials_in_use(dir);
    keys.iter()
        .filter(|key| {
            credentials
                .iter()
                .any(|credential| match &credential.token {
                    Some(token) => key.token == *token,
                    None => credential.base_url.as_deref().is_some_and(|base_url| {
                        recipes.get(&key.provider).is_some_and(|recipe| {
                            normalize_base_url(&recipe.base_url) == normalize_base_url(base_url)
                        })
                    }),
                })
        })
        .map(KeyEntry::id)
        .collect()
}

/// 明文才算「能对账」：`$NAME` / `${NAME}` / `!command` 要运行时才求值，值不在文件里。
///
/// `$$` / `$!` 是 pi 的转义写法（分别表示字面上的 `$` 和开头的 `!`），按字面值比：
/// 一个 `$$sk-1` 的真实值就是 `$sk-1`（转义只吃掉一个字符）。
fn plaintext(raw: &str, base_url: Option<String>) -> Credential {
    if raw.starts_with("$$") || raw.starts_with("$!") {
        return Credential {
            token: Some(raw[1..].to_string()),
            base_url,
        };
    }
    let token = if raw.starts_with('$') || raw.starts_with('!') {
        None // 看不见明文，只能靠地址
    } else {
        Some(raw.to_string())
    };
    Credential { token, base_url }
}
