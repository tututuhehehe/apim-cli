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

/// 造一份「apim 刚导入完」的 pi 现场。
fn imported(dir: &std::path::Path, token: &str) {
    fs::write(
        config::models_path(dir),
        format!(
            r#"{{
  "providers": {{
    "apim-ikun": {{ "baseUrl": "https://api.ikuncode.cc/v1", "api": "openai-completions", "apiKey": "{token}" }}
  }}
}}
"#
        ),
    )
    .unwrap();
    fs::write(
        config::settings_path(dir),
        r#"{ "defaultProvider": "apim-ikun", "defaultModel": "glm-5" }"#,
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

/// 用户把默认 provider 切回 pi 内置的（没有 `apim-` 前缀）→ 不算 apim 写的。
#[test]
fn builtin_provider_is_not_ours() {
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
