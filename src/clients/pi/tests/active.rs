//! ★ 现场识别：pi 配置里**每一份**凭据（`auth.json` 的 `/login` 凭据 + `models.json` 的 `apiKey`）
//! 拿来和 apim 的密钥对账。
//!
//! 关键行为：pi 没有「唯一激活的 provider」，`defaultProvider` 只是启动默认值，所以**不**只看它 ——
//! auth.json 里登过的 `opencode-go`、models.json 里导入过的每个 provider，都算在用。

use std::collections::HashMap;
use std::fs;

use super::helpers::temp_dir;
use crate::clients::pi::{active, config};
use crate::config::KeyEntry;
use crate::recipe::{Auth, Recipe};

fn key(provider: &str, alias: &str, token: &str) -> KeyEntry {
    KeyEntry {
        provider: provider.into(),
        alias: alias.into(),
        group: None,
        token: token.into(),
    }
}

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

/// 造一份「apim 刚导入完」的 pi 现场（key 在 `models.json` 里）。
fn imported(dir: &std::path::Path, token: &str) {
    imported_as(dir, "apim-ikun", token);
}

/// 同上，但 provider 键由调用方给（用来测「不带 apim- 前缀也能认」）。
fn imported_as(dir: &std::path::Path, provider_key: &str, token: &str) {
    fs::write(
        config::models_path(dir),
        format!(
            r#"{{
  "providers": {{
    "{provider_key}": {{ "baseUrl": "https://api.ikuncode.cc/v1", "api": "openai-completions", "apiKey": "{token}" }}
  }}
}}
"#
        ),
    )
    .unwrap();
    // 注意：这里**不写** settings.json —— 判定根本不读它（`defaultProvider` 只是 pi 的启动默认值）
}

/// 往 pi 自己的凭据库（`auth.json`）里放一条 API key —— 模拟用户用 `/login` 存过。
fn auth_api_key(dir: &std::path::Path, provider_key: &str, token: &str) {
    fs::write(
        dir.join(config::AUTH_FILE),
        format!(
            r#"{{
  "{provider_key}": {{ "type": "api_key", "key": "{token}" }}
}}
"#
        ),
    )
    .unwrap();
}

#[test]
fn reads_every_credential_from_both_files() {
    let dir = temp_dir("active-read");
    imported_as(&dir, "apim-ikun", "sk-models");
    auth_api_key(&dir, "opencode-go", "sk-auth");
    let found = active::credentials_in_use(Some(&dir));
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .any(|c| c.token.as_deref() == Some("sk-models"))
    );
    assert!(found.iter().any(|c| c.token.as_deref() == Some("sk-auth")));
}

#[test]
fn matches_the_key_whose_token_is_in_the_config() {
    let dir = temp_dir("active-match");
    imported(&dir, "sk-1");
    let keys = [key("ikun", "codex", "sk-1"), key("ikun", "other", "sk-2")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// **用户报过的那条**：pi 的 `auth.json` 里登着 `opencode-go`（token 与 apim 里那把一样），
/// 但 `defaultProvider` 是另一个 provider —— 那把 key 也是在用的，★ 必须亮。
#[test]
fn auth_json_credentials_count_even_when_another_provider_is_the_default() {
    let dir = temp_dir("active-auth-not-default");
    imported_as(&dir, "apim-ikun", "sk-ikun");
    auth_api_key(&dir, "opencode-go", "sk-opencode");
    let keys = [
        key("opencode-go", "111", "sk-opencode"),
        key("ikun", "codex", "sk-ikun"),
    ];
    let recipes = HashMap::new();
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["opencode-go.111".to_string(), "ikun.codex".to_string()],
        "两份凭据都算在用（顺序按 keys）"
    );
}

/// 用户手改了 pi 里的凭据（换 token / 换登录）→ apim 里没有一把对得上，★ 就该消失。
#[test]
fn hand_edited_token_matches_nothing() {
    let dir = temp_dir("active-hand-edited");
    imported(&dir, "sk-hand-written");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// **`apim-` 前缀只约束「写」，不约束「认」**：用户手写的 provider（不带前缀）用着 apim 的 key，
/// ★ 照样要亮。
#[test]
fn provider_without_the_apim_prefix_is_recognized() {
    let dir = temp_dir("active-no-prefix");
    imported_as(&dir, "my-ikun-relay", "sk-1");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = HashMap::new();
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// 认的时候**只比密钥**：provider 名字与厂商 id 毫无关系也算（同一个 token 就是同一把 API key）。
#[test]
fn any_provider_name_is_recognized_by_the_key_alone() {
    let dir = temp_dir("active-token-only");
    imported_as(&dir, "whatever-the-user-called-it", "sk-1");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://some-other-host.example.com");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// 同一把 key 在 pi 里配了两处（auth.json + models.json）也只标记一次。
#[test]
fn the_same_token_in_both_files_marks_the_key_once() {
    let dir = temp_dir("active-same-again");
    imported(&dir, "sk-1");
    auth_api_key(&dir, "apim-ikun", "sk-1");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = HashMap::new();
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// `auth.json` 里是 `type: "oauth"`（订阅凭据）→ 不是 apim 的 key；
/// 同一 provider 在 models.json 里只有地址、没有 apiKey → 也不产生标记（没有能对账的凭据）。
#[test]
fn subscription_credential_is_not_apims_key() {
    let dir = temp_dir("active-oauth");
    imported(&dir, "sk-1");
    fs::write(
        dir.join(config::AUTH_FILE),
        r#"{ "apim-ikun": { "type": "oauth", "access": "a", "refresh": "r", "expires": 1 } }"#,
    )
    .unwrap();
    // models.json 里只剩地址（比如用户把 apiKey 换成了自己环境变量那套）
    fs::write(
        config::models_path(&dir),
        r#"{ "providers": { "apim-ikun": { "baseUrl": "https://api.ikuncode.cc/v1" } } }"#,
    )
    .unwrap();
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// `$ENV` / `!cmd` 看不见明文 → 退化成比 base_url。
#[test]
fn env_api_key_falls_back_to_base_url() {
    let dir = temp_dir("active-env-key");
    imported(&dir, "$IKUN_API_KEY");
    let keys = [key("ikun", "codex", "sk-1")];
    let map = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &map),
        vec!["ikun.codex".to_string()]
    );

    let other = recipes("ikun", "https://other.example.com");
    assert!(active::active_key_ids(Some(&dir), &keys, &other).is_empty());
}

/// `auth.json` 里的 key 是 `$ENV` 时看不见明文，而它没有地址可比 → 不产生标记。
#[test]
fn auth_json_env_key_without_a_base_url_matches_nothing() {
    let dir = temp_dir("active-auth-env");
    auth_api_key(&dir, "opencode-go", "$OPENCODE_API_KEY");
    let keys = [key("opencode-go", "111", "sk-1")];
    let recipes = recipes("opencode-go", "https://opencode.ai");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// `$$` / `$!` 是 pi 的转义写法：字面值是去掉一个字符后的串，对账时要按字面值比。
#[test]
fn escaped_api_key_is_compared_by_its_literal_value() {
    let dir = temp_dir("active-escaped-key");
    imported(&dir, "$$sk-1");
    let keys = [key("ikun", "codex", "$sk-1")];
    let recipes = HashMap::new();
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// 什么都没配 / 没有凭据 → 什么都没在用。
#[test]
fn missing_config_reads_as_nothing() {
    let dir = temp_dir("active-empty");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::credentials_in_use(Some(&dir)).is_empty());
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// `auth.json` 读坏了不影响从 `models.json` 认（那只是个附加来源）。
#[test]
fn broken_auth_json_falls_back_to_models_json() {
    let dir = temp_dir("active-auth-broken");
    imported(&dir, "sk-1");
    fs::write(dir.join(config::AUTH_FILE), "{ not json").unwrap();
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = HashMap::new();
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}
