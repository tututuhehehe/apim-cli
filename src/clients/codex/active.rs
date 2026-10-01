//! 回读 codex 自己的 `config.toml`：它现在真正在用哪个 provider / 哪把密钥。
//!
//! ★ 标记只认这里读出来的**现场**（apim 不再另存一份「上次导入了谁」）：顶层
//! `model_provider` 指向 `[model_providers.<key>]`，表里的 `base_url` 与
//! `experimental_bearer_token` 才是现在生效的东西。用户手改配置（换 token、把
//! `model_provider` 切走、删掉那个块）后 ★ 跟着变；读不到 / 没配 → 什么都没在用。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{codex_home, config_file, normalize_base_url, provider_key};
use crate::clients::ActiveProvider;
use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// 读 `config.toml`，回答「codex 现在激活的是哪个 provider」。
///
/// `home` 为 `None` 时按 `CODEX_HOME` / `~/.codex` 解析。配置文件不存在、解析失败、
/// 没写 `model_provider`、或那个 provider 块不在，都返回 `None`：都等价于
/// 「codex 现在没在用 apim 导入过的东西」，不该在密钥行上打 ★。
pub fn active_provider(home: Option<&Path>) -> Option<ActiveProvider> {
    let home: PathBuf = match home {
        Some(dir) => dir.to_path_buf(),
        None => codex_home(),
    };
    let doc = config_file::read(&home.join("config.toml")).ok()?;
    let key = doc.get("model_provider")?.as_str()?.to_string();
    let provider = doc
        .get("model_providers")?
        .as_table_like()?
        .get(&key)?
        .as_table_like()?;
    let base_url = provider.get("base_url")?.as_str()?.to_string();
    let token = provider
        .get("experimental_bearer_token")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    Some(ActiveProvider {
        provider_key: key,
        base_url,
        token,
    })
}

/// 现场 → apim 密钥 id：provider 表名对得上，且 **token 一模一样**，才算
/// 「这个 API 真的被导入了」。同一个 token 换个厂商不成立，所以表名也要对。
///
/// 客户端改用 `env_key` 从环境变量取 token 时（apim 不这么写，但用户可能手改），
/// token 读不到，退化成「表名 + base_url 一致」。
/// 返回按 `keys` 顺序（多把密钥共用一个 token 时都能命中，这是实话）。
pub fn active_key_ids(
    home: Option<&Path>,
    keys: &[KeyEntry],
    recipes: &HashMap<String, Recipe>,
) -> Vec<String> {
    let Some(active) = active_provider(home) else {
        return Vec::new();
    };
    keys.iter()
        .filter(|key| provider_key(&key.provider) == active.provider_key)
        .filter(|key| match &active.token {
            Some(token) => key.token == *token,
            None => recipes.get(&key.provider).is_some_and(|recipe| {
                normalize_base_url(&recipe.base_url) == normalize_base_url(&active.base_url)
            }),
        })
        .map(KeyEntry::id)
        .collect()
}
