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

/// 读 pi 的两份配置，回答「它现在默认用哪个 provider」。
///
/// 读不到 / 没写 `defaultProvider` / 那个 provider 不在 `models.json` 里 → `None`
/// （等价于「pi 现在没在用 apim 写进去的东西」）。
pub fn active_provider(dir: Option<&Path>) -> Option<ActiveProvider> {
    let dir: PathBuf = match dir {
        Some(dir) => dir.to_path_buf(),
        None => agent_dir(),
    };
    let settings = config::read(&config::settings_path(&dir)).ok()?;
    // defaultProvider 是 pi 的启动 provider；defaultModel 只是模型 id（这里用不上）
    let provider_key = settings.get("defaultProvider")?.as_str()?.to_string();
    let models = config::read(&config::models_path(&dir)).ok()?;
    let entry = models.get("providers")?.get(&provider_key)?;
    let base_url = entry.get("baseUrl").and_then(Value::as_str)?.to_string();
    let token = entry
        .get("apiKey")
        .and_then(Value::as_str)
        .and_then(plaintext_api_key)
        .map(str::to_string);
    Some(ActiveProvider {
        provider_key,
        base_url,
        token,
    })
}

/// pi 的 `apiKey` 可以是明文，也可以是 `$NAME` / `${NAME}` / `!command`（请求时才求值，
/// 值不在文件里）。只有**明文**才能拿来跟 apim 的密钥对账；取不到明文就返回 `None`，
/// 调用方退化成比 base_url。
///
/// 转义写法（`$$` / `$!`，pi 用来表示字面上的 `$` 或开头的 `!`）算明文 —— apim 写下去
/// 的永远是明文，所以这两种开头直接按原串比。
fn plaintext_api_key(raw: &str) -> Option<&str> {
    if raw.starts_with("$$") || raw.starts_with("$!") {
        return Some(raw);
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
