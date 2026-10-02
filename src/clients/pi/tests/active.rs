//! ★ 现场识别：pi 现在默认用哪个 provider、那把密钥是不是 apim 里的这一把。

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

/// 造一份「apim 刚导入完」的 pi 现场（key 在 models.json 里）。
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
    fs::write(
        config::settings_path(dir),
        format!(r#"{{ "defaultProvider": "{provider_key}", "defaultModel": "glm-5" }}"#),
    )
    .unwrap();
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
fn reads_the_default_provider_from_the_live_config() {
    let dir = temp_dir("active-read");
    imported(&dir, "sk-1");
    let found = active::active_provider(Some(&dir)).expect("应读出默认 provider");
    assert_eq!(found.provider_key, "apim-ikun");
    assert_eq!(found.base_url, "https://api.ikuncode.cc/v1");
    assert_eq!(found.token.as_deref(), Some("sk-1"));
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

/// 用户手改了 pi 里的 apiKey → apim 里没有一把对得上，★ 就该消失。
#[test]
fn hand_edited_token_matches_nothing() {
    let dir = temp_dir("active-hand-edited");
    imported(&dir, "sk-hand-written");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// 用户把默认 provider 切到一个**在 models.json 里不存在、也没凭据**的 → 不算在用。
#[test]
fn provider_without_any_credential_is_not_in_use() {
    let dir = temp_dir("active-builtin");
    imported(&dir, "sk-1");
    fs::write(
        config::settings_path(&dir),
        r#"{ "defaultProvider": "deepseek", "defaultModel": "deepseek-v4" }"#,
    )
    .unwrap();
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// pi 内置 provider（models.json 里没有它的条目）+ 你在 `auth.json` 存了 apim 里那把 key
/// （比如用 `/login` 粘进去的）→ 那把 key 真的在用，★ 要亮。
#[test]
fn builtin_provider_with_an_apim_key_in_auth_json_is_recognized() {
    let dir = temp_dir("active-builtin-auth");
    fs::write(
        config::settings_path(&dir),
        r#"{ "defaultProvider": "deepseek", "defaultModel": "deepseek-v4" }"#,
    )
    .unwrap();
    auth_api_key(&dir, "deepseek", "sk-1");
    let keys = [key("deepseek", "1", "sk-1")];
    let recipes = recipes("deepseek", "https://api.deepseek.com");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["deepseek.1".to_string()]
    );
}

/// **`apim-` 前缀只约束「写」，不约束「认」**：用户手写的 provider（不带前缀）用着 apim 的 key，
/// ★ 照样要亮。
#[test]
fn provider_without_the_apim_prefix_is_recognized() {
    let dir = temp_dir("active-no-prefix");
    imported_as(&dir, "my-ikun-relay", "sk-1");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
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

/// pi 的凭据优先级：`auth.json`（`/login` 存的）高于 `models.json` 的 `apiKey`；
/// apim 要能看出「现在是 auth.json 里那把」。
#[test]
fn auth_json_credential_wins_over_models_json() {
    let dir = temp_dir("active-auth-wins");
    imported(&dir, "sk-in-models");
    auth_api_key(&dir, "apim-ikun", "sk-from-login");
    let keys = [
        key("ikun", "codex", "sk-from-login"),
        key("ikun", "models", "sk-in-models"),
    ];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()],
        "auth.json 里的凭据优先，models.json 那把这会儿没在用"
    );
}

/// `auth.json` 里是 `type: "oauth"`（订阅凭据）→ 用的是订阅，不是 apim 的 key，
/// 即使 models.json 里写着 apim 的 key 也不算（pi 按优先级根本不会用那个 apiKey）。
#[test]
fn subscription_credential_is_not_apims_key() {
    let dir = temp_dir("active-oauth");
    imported(&dir, "sk-1");
    fs::write(
        dir.join(config::AUTH_FILE),
        r#"{
  "apim-ikun": { "type": "oauth", "access": "a", "refresh": "r", "expires": 1 }
}
"#,
    )
    .unwrap();
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// `auth.json` 里的 key 是要运行时求值的（`$ENV`）→ 看不见明文，退化成比 base_url。
#[test]
fn auth_json_env_key_falls_back_to_base_url() {
    let dir = temp_dir("active-auth-env");
    imported(&dir, "sk-1");
    auth_api_key(&dir, "apim-ikun", "$IKUN_API_KEY");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// `auth.json` 读坏了 / 不存在都不影响检测（那只是个附加的对比来源）。
#[test]
fn broken_auth_json_falls_back_to_models_json() {
    let dir = temp_dir("active-auth-broken");
    imported(&dir, "sk-1");
    fs::write(dir.join(config::AUTH_FILE), "{ not json").unwrap();
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// apiKey 是 `$ENV` 之类的取不到明文时的退路：比 base_url。
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

/// 没配过 / 配置被删 → 什么都没在用。
#[test]
fn missing_config_reads_as_nothing() {
    let dir = temp_dir("active-empty");
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_provider(Some(&dir)).is_none());
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());

    // defaultProvider 指向的 provider 在 models.json 里不存在（用户删了那个条目）
    fs::write(
        config::settings_path(&dir),
        r#"{ "defaultProvider": "apim-ikun" }"#,
    )
    .unwrap();
    assert!(active::active_provider(Some(&dir)).is_none());
}

/// provider 条目里**根本没有 `apiKey`**（pi 会报 no authentication method configured，用不起来）
/// → 不算在用，不能因为 base_url 碰巧一样就打 ★。
#[test]
fn provider_without_api_key_is_not_in_use() {
    let dir = temp_dir("active-no-api-key");
    fs::write(
        config::models_path(&dir),
        r#"{
  "providers": {
    "apim-ikun": { "baseUrl": "https://api.ikuncode.cc/v1", "api": "openai-completions" }
  }
}
"#,
    )
    .unwrap();
    fs::write(
        config::settings_path(&dir),
        r#"{ "defaultProvider": "apim-ikun" }"#,
    )
    .unwrap();
    let keys = [key("ikun", "codex", "sk-1")];
    let recipes = recipes("ikun", "https://api.ikuncode.cc");
    assert!(active::active_key_ids(Some(&dir), &keys, &recipes).is_empty());
}

/// `$$` / `$!` 是 pi 的转义写法：字面值是去掉一个字符后的串，对账时要按字面值比。
#[test]
fn escaped_api_key_is_compared_by_its_literal_value() {
    let dir = temp_dir("active-escaped-key");
    imported(&dir, "$$sk-1");
    let keys = [key("ikun", "codex", "$sk-1")];
    let recipes = recipes("ikun", "https://other.example.com");
    assert_eq!(
        active::active_key_ids(Some(&dir), &keys, &recipes),
        vec!["ikun.codex".to_string()]
    );
}

/// 项目级 `.pi/settings.json` 会深合并到 agent 目录设置之上（项目优先）。
/// 项目里把默认 provider 切走时，★ 不能再留在 apim 写的那把上。
#[test]
fn project_settings_override_the_agent_directory() {
    let dir = temp_dir("active-project-override");
    imported(&dir, "sk-1");
    let project = dir.join("project/.pi");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join(config::SETTINGS_FILE),
        r#"{ "defaultProvider": "opencode-go" }"#,
    )
    .unwrap();

    assert!(
        active::active_provider_with(Some(&dir), Some(project.join(config::SETTINGS_FILE)))
            .is_none(),
        "项目里切走了默认 provider，就不该再说 apim 那把在用"
    );
    // 项目里没提 defaultProvider 时不影响
    fs::write(
        project.join(config::SETTINGS_FILE),
        r#"{ "theme": "dark" }"#,
    )
    .unwrap();
    assert!(
        active::active_provider_with(Some(&dir), Some(project.join(config::SETTINGS_FILE)))
            .is_some()
    );
}
