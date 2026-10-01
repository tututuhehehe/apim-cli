//! 一次 pi 导入的端到端行为：写了什么、保留了谁的字段、校验与回滚。

use std::fs;

use serde_json::Value;

use super::helpers::{fake_pi, fake_pi_failing, request_for, temp_dir, write_model_table};
use crate::clients::pi::import::import_in;
use crate::clients::pi::{config, pi_provider_key};

fn read(path: &std::path::Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[cfg(unix)]
#[test]
fn import_writes_provider_and_default_model_then_verifies() {
    let dir = temp_dir("import-e2e");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5", "gpt-6-sol"]);

    let report = import_in(&dir, &request_for(&["glm-5", "gpt-6-sol"]), Some(&bin)).unwrap();

    assert_eq!(report.provider_key, "apim-ikun");
    assert_eq!(report.model, "glm-5");
    assert_eq!(report.detail, None, "pi 侧没有思考强度要固定");

    let models = read(&config::models_path(&dir));
    let entry = &models["providers"]["apim-ikun"];
    assert_eq!(entry["api"], "openai-completions");
    assert_eq!(entry["apiKey"], "sk-placeholder");
    assert_eq!(
        entry["baseUrl"], "https://api.ikuncode.cc/v1",
        "base_url 要补 /v1"
    );
    assert_eq!(entry["name"], "ikun");
    let ids: Vec<&str> = entry["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["glm-5", "gpt-6-sol"]);
    // 模型条目留最小集合：能力交给 pi 的默认值，apim 不编数字
    assert_eq!(entry["models"][0]["reasoning"], true);
    assert_eq!(
        entry["models"][0]["input"],
        serde_json::json!(["text", "image"])
    );
    assert!(entry["models"][0].get("contextWindow").is_none());

    let settings = read(&config::settings_path(&dir));
    assert_eq!(settings["defaultProvider"], "apim-ikun");
    assert_eq!(settings["defaultModel"], "glm-5");
}

/// provider 键**一律**带 `apim-` 前缀：pi 内置了一大堆同名 provider，
/// 不加前缀会连带把内置 provider 的 baseUrl 改掉（openai 会被指到我们的中转站）。
#[cfg(unix)]
#[test]
fn provider_key_always_carries_the_prefix() {
    assert_eq!(pi_provider_key("openai"), "apim-openai");
    assert_eq!(pi_provider_key("ikun"), "apim-ikun");

    let dir = temp_dir("import-prefix");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-openai", &["gpt-6-sol"]);
    let mut request = request_for(&["gpt-6-sol"]);
    request.provider_id = "openai".into();
    import_in(&dir, &request, Some(&bin)).unwrap();

    let models = read(&config::models_path(&dir));
    assert!(models["providers"]["apim-openai"].is_object());
    assert!(
        models["providers"]["openai"].is_null(),
        "不许碰 pi 内置的 openai 条目"
    );
}

/// 只动我们认识的键：其他 provider、用户手写的 headers/compat、以及 settings 里别的设置都要留下。
#[cfg(unix)]
#[test]
fn import_preserves_everything_it_does_not_own() {
    let dir = temp_dir("import-preserve");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5"]);
    fs::write(
        config::models_path(&dir),
        r#"{
  "providers": {
    "sensenova": { "baseUrl": "https://token.sensenova.cn/v1", "apiKey": "sk-keep", "api": "openai-completions", "models": [{ "id": "m" }] },
    "apim-ikun": { "headers": { "X-Keep": "1" }, "authHeader": true, "modelOverrides": { "glm-5": { "reasoning": true } } }
  }
}
"#,
    )
    .unwrap();
    fs::write(
        config::settings_path(&dir),
        r#"{ "theme": "dark", "enabledModels": ["opencode-go/glm-5.2"] }"#,
    )
    .unwrap();

    import_in(&dir, &request_for(&["glm-5"]), Some(&bin)).unwrap();

    let models = read(&config::models_path(&dir));
    assert_eq!(
        models["providers"]["sensenova"]["apiKey"], "sk-keep",
        "别人的条目不许动"
    );
    let entry = &models["providers"]["apim-ikun"];
    assert_eq!(entry["headers"]["X-Keep"], "1", "用户手写的 headers 要留下");
    assert_eq!(entry["authHeader"], true);
    assert_eq!(entry["modelOverrides"]["glm-5"]["reasoning"], true);
    assert_eq!(entry["apiKey"], "sk-placeholder", "我们负责的键要更新");

    let settings = read(&config::settings_path(&dir));
    assert_eq!(settings["theme"], "dark", "settings.json 里别的设置不许动");
    // pi 自己存默认模型时会往 enabledModels 追加（AgentSession._addPersistedDefaultToNonEmptyScope），
    // 镜像它，否则用户设了 enabledModels 后新模型进不了启动选择
    assert_eq!(
        settings["enabledModels"],
        serde_json::json!(["opencode-go/glm-5.2", "apim-ikun/glm-5"])
    );
}

/// `enabledModels` 已经有这个模型（大小写不敏感）时不重复追加。
#[cfg(unix)]
#[test]
fn enabled_models_is_not_duplicated() {
    let dir = temp_dir("import-enabled-dup");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5"]);
    fs::write(
        config::settings_path(&dir),
        r#"{ "enabledModels": ["APIM-IKUN/GLM-5"] }"#,
    )
    .unwrap();

    import_in(&dir, &request_for(&["glm-5"]), Some(&bin)).unwrap();
    let settings = read(&config::settings_path(&dir));
    assert_eq!(
        settings["enabledModels"],
        serde_json::json!(["APIM-IKUN/GLM-5"])
    );
}

/// 校验失败要把**两处**改动都还原（否则用户看到「导入失败」，pi 其实已经切过去了）。
#[cfg(unix)]
#[test]
fn failed_verification_rolls_both_files_back() {
    let dir = temp_dir("import-rollback");
    let bin = fake_pi_failing(&dir);
    let old_models = r#"{ "providers": { "apim-ikun": { "baseUrl": "https://old.example/v1", "api": "openai-completions", "apiKey": "sk-old" } } }"#;
    fs::write(config::models_path(&dir), old_models).unwrap();
    fs::write(
        config::settings_path(&dir),
        r#"{ "defaultProvider": "opencode-go" }"#,
    )
    .unwrap();

    let err = import_in(&dir, &request_for(&["glm-5"]), Some(&bin)).unwrap_err();
    assert!(err.contains("已还原"), "{err}");

    let models = fs::read_to_string(config::models_path(&dir)).unwrap();
    assert!(models.contains("sk-old"), "models.json 要还原：{models}");
    let settings = fs::read_to_string(config::settings_path(&dir)).unwrap();
    assert!(
        settings.contains("opencode-go"),
        "settings.json 也要还原：{settings}"
    );
}

/// 原来没有这两个文件（全新 pi）时，回滚要把它们删掉，别留半份配置。
#[cfg(unix)]
#[test]
fn rollback_removes_files_that_did_not_exist() {
    let dir = temp_dir("import-rollback-new");
    let bin = fake_pi_failing(&dir);
    assert!(import_in(&dir, &request_for(&["glm-5"]), Some(&bin)).is_err());
    assert!(!config::models_path(&dir).exists());
    assert!(!config::settings_path(&dir).exists());
}

/// 勾选的模型 pi 没列出来（比如中转站根本不认这个 id）→ 报错并回滚。
#[cfg(unix)]
#[test]
fn model_missing_from_pi_listing_is_an_error() {
    let dir = temp_dir("import-missing-model");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5"]);

    let err = import_in(&dir, &request_for(&["glm-5", "no-such-model"]), Some(&bin)).unwrap_err();
    assert!(err.contains("no-such-model"), "{err}");
    assert!(
        !config::models_path(&dir).exists(),
        "校验不过就不留半份配置"
    );
}

/// 没装 pi 就直接失败：不校验的导入等于没验证过（同 codex 的硬前提）。
#[test]
fn missing_pi_binary_fails_before_writing() {
    let dir = temp_dir("import-no-pi");
    let err = import_in(&dir, &request_for(&["glm-5"]), None).unwrap_err();
    assert!(err.contains("未找到 pi"), "{err}");
    assert!(!config::models_path(&dir).exists());
}

/// 顶层结构不对（用户把 providers 写成了数组）→ 明确报错，不做破坏性覆盖。
#[cfg(unix)]
#[test]
fn broken_providers_shape_is_refused() {
    let dir = temp_dir("import-bad-shape");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5"]);
    fs::write(config::models_path(&dir), r#"{ "providers": [] }"#).unwrap();

    let err = import_in(&dir, &request_for(&["glm-5"]), Some(&bin)).unwrap_err();
    assert!(err.contains("providers 不是对象"), "{err}");
    assert_eq!(
        fs::read_to_string(config::models_path(&dir)).unwrap(),
        r#"{ "providers": [] }"#
    );
}

/// 导入期间占着锁：另一个 apim 进来要被告知等一等（而不是两边都报成功）。
#[cfg(unix)]
#[test]
fn import_takes_the_lock_and_releases_it() {
    let dir = temp_dir("import-lock");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5"]);

    import_in(&dir, &request_for(&["glm-5"]), Some(&bin)).unwrap();
    assert!(!dir.join(".apim-import.lock").exists(), "导入结束要清掉锁");
}
