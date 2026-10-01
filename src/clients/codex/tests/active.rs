//! 「回读客户端现场」的测试：★ 只认 codex `config.toml` 里现在生效的东西。
//!
//! 这里守的核心行为：**apim 不存台账** —— 用户手改 codex 配置（换 token、切走
//! `model_provider`、删掉 provider 块）后，★ 必须跟着消失，而不是留在旧密钥上。

use std::collections::HashMap;
use std::fs;

use crate::clients::codex::active::{active_key_ids, active_provider};
use crate::clients::codex::provider_key;
use crate::config::KeyEntry;
use crate::recipe::{Auth, Recipe};

use super::helpers::temp_dir;

/// 一把 apim 密钥。
fn key(provider: &str, alias: &str, token: &str) -> KeyEntry {
    KeyEntry {
        provider: provider.into(),
        alias: alias.into(),
        group: None,
        token: token.into(),
    }
}

/// 假 recipe（只需要 base_url 参与「没写 token 时按地址比」那条退路）。
fn recipes(provider: &str, base_url: &str) -> HashMap<String, Recipe> {
    HashMap::from([(
        provider.to_string(),
        Recipe {
            id: provider.into(),
            name: provider.into(),
            base_url: base_url.into(),
            homepage: None,
            models_url: None,
            supports_groups: false,
            vars: HashMap::new(),
            auth: Auth::default(),
            health: None,
            balance: None,
            origin: None,
        },
    )])
}

/// 造一个 codex home，`body` 是 `config.toml` 的内容。
fn home_with(name: &str, body: &str) -> std::path::PathBuf {
    let dir = temp_dir(name);
    fs::write(dir.join("config.toml"), body).unwrap();
    dir
}

/// 导入刚写完的样子：激活 ikun，token 是 sk-1。
const IMPORTED: &str = "\
model = \"glm-5\"
model_provider = \"ikun\"

[model_providers.ikun]
name = \"ikun\"
base_url = \"https://api.ikuncode.cc/v1\"
wire_api = \"responses\"
experimental_bearer_token = \"sk-1\"
";

#[test]
fn reads_the_active_provider_from_the_live_config() {
    let home = home_with("active-read", IMPORTED);
    let active = active_provider(Some(&home)).expect("应读出激活项");
    assert_eq!(active.provider_key, "ikun");
    assert_eq!(active.base_url, "https://api.ikuncode.cc/v1");
    assert_eq!(active.token.as_deref(), Some("sk-1"));
}

#[test]
fn matches_the_key_whose_token_is_in_the_config() {
    let home = home_with("active-match", IMPORTED);
    let keys = [key("ikun", "codex", "sk-1"), key("ikun", "other", "sk-2")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active_key_ids(Some(&home), &keys, &recipes),
        vec!["ikun.codex".to_string()],
        "只有 token 一模一样的那把算在用"
    );
}

/// 用户手改了 codex 里的 token（比如自己换成了别家的 key）→ apim 里没有一把对得上。
#[test]
fn hand_edited_token_matches_nothing() {
    let home = home_with(
        "active-hand-edited-token",
        &IMPORTED.replace("sk-1", "sk-someone-else"),
    );
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(
        active_key_ids(Some(&home), &keys, &recipes).is_empty(),
        "配置里的 token 不是 apim 这把了，不该再打 ★"
    );
}

/// 用户手改 `model_provider` 切回内置 openai → 一个 apim 密钥都不在用。
#[test]
fn switching_provider_away_matches_nothing() {
    let home = home_with(
        "active-switched-away",
        &IMPORTED.replace("model_provider = \"ikun\"", "model_provider = \"openai\""),
    );
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active_key_ids(Some(&home), &keys, &recipes).is_empty());
}

/// 撞上保留名的厂商 id 在配置里是 `apim-` 前缀的表名，反查时也要能对上。
#[test]
fn reserved_provider_id_is_matched_through_its_prefix() {
    let home = home_with(
        "active-reserved",
        &IMPORTED
            .replace("ikun", "apim-openai")
            .replace("name = \"apim-openai\"", "name = \"OpenAI 中转\""),
    );
    // 表名带前缀，但 apim 侧的厂商 id 是原来的 `openai`
    assert_eq!(
        provider_key("openai"),
        "apim-openai",
        "表名对得上（保留名加前缀）"
    );
    let keys = [key("openai", "relay", "sk-1")];
    let recipes = recipes("openai", "https://api.ikuncode.cc");
    assert_eq!(
        active_key_ids(Some(&home), &keys, &recipes),
        vec!["openai.relay".to_string()]
    );
}

/// 用户把 token 换成 `env_key`（codex 支持从环境变量取）→ 读不到 token，
/// 退化成「表名 + base_url 一致」。
#[test]
fn env_key_falls_back_to_base_url() {
    let home = home_with(
        "active-env-key",
        "\
model_provider = \"ikun\"

[model_providers.ikun]
name = \"ikun\"
base_url = \"https://api.ikuncode.cc/v1\"
wire_api = \"responses\"
env_key = \"IKUN_API_KEY\"
",
    );
    let keys = [key("ikun", "codex", "sk-1")];
    let map = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active_key_ids(Some(&home), &keys, &map),
        vec!["ikun.codex".to_string()],
        "地址对得上也算在用（token 在环境变量里，读不到）"
    );

    // 地址也对不上 → 什么都不算
    let other = recipes("ikun", "https://other.example.com");
    assert!(active_key_ids(Some(&home), &keys, &other).is_empty());
}

/// 没有 config.toml / 没写 `model_provider` / provider 块已被删 → 都没在读 apim 的东西。
#[test]
fn missing_or_incomplete_config_reads_as_nothing() {
    let empty = temp_dir("active-empty");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active_provider(Some(&empty)).is_none());
    assert!(active_key_ids(Some(&empty), &keys, &recipes).is_empty());

    let no_pointer = home_with(
        "active-no-pointer",
        "[model_providers.ikun]\nbase_url = \"x\"\n",
    );
    assert!(active_key_ids(Some(&no_pointer), &keys, &recipes).is_empty());

    let block_removed = home_with("active-block-gone", "model_provider = \"ikun\"\n");
    assert!(active_key_ids(Some(&block_removed), &keys, &recipes).is_empty());
}
