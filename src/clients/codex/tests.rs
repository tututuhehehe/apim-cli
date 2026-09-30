//! codex 适配层的测试：模板挑选、目录生成、config.toml 保注释改写、端到端导入。
//! 全部在 target/ 下的临时目录里跑，且用假 codex 脚本，不碰真实 `~/.codex`。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;

// ---- 临时目录 ----------------------------------------------------------

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("apim-codex-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ---- 纯函数 ------------------------------------------------------------

#[test]
fn reserved_ids_get_prefixed() {
    assert_eq!(provider_key("ikun"), "ikun");
    assert_eq!(provider_key("openai"), "apim-openai");
    assert_eq!(provider_key("ollama"), "apim-ollama");
    assert_eq!(provider_key("amazon-bedrock"), "apim-amazon-bedrock");
}

#[test]
fn base_url_gets_v1_suffix_when_missing() {
    assert_eq!(
        normalize_base_url("https://api.ikuncode.cc"),
        "https://api.ikuncode.cc/v1"
    );
    assert_eq!(
        normalize_base_url("https://api.ikuncode.cc/"),
        "https://api.ikuncode.cc/v1"
    );
    assert_eq!(
        normalize_base_url("https://api.example.com/v1"),
        "https://api.example.com/v1"
    );
    assert_eq!(
        normalize_base_url("https://api.example.com/v1/"),
        "https://api.example.com/v1"
    );
}

/// 合成一份「codex 内置模型表」：两条模板，priority 小的应被选中。
fn fake_bundled() -> Value {
    json!({
        "models": [
            {
                "slug": "tpl-slow",
                "display_name": "Slow",
                "priority": 9,
                "visibility": "list",
                "supported_in_api": true,
                "apply_patch_tool_type": "freeform",
                "shell_type": "unified_exec",
                "service_tiers": [{"id": "priority"}],
                "upgrade": {"model": "next"},
                "availability_nux": {"message": "hello"},
                "experimental_supported_tools": ["clock"],
                "base_instructions": "TEMPLATE-PROMPT",
                "model_messages": {
                    "instructions_template": "TEMPLATE-PROMPT",
                    "instructions_variables": null,
                    "approvals": null,
                    "collaboration_modes": null,
                    "auto_review": null,
                    "permissions": null,
                    "multi_agent": null
                }
            },
            {
                "slug": "tpl-flagship",
                "display_name": "Flagship",
                "priority": 1,
                "visibility": "list",
                "supported_in_api": true,
                "apply_patch_tool_type": "freeform",
                "shell_type": "unified_exec",
                "base_instructions": "FLAGSHIP-PROMPT",
                "model_messages": { "instructions_template": "FLAGSHIP-PROMPT" }
            },
            {
                "slug": "tpl-hidden",
                "display_name": "Hidden",
                "priority": 0,
                "visibility": "hide",
                "supported_in_api": true,
                "base_instructions": "HIDDEN-PROMPT",
                "model_messages": { "instructions_template": "HIDDEN-PROMPT" }
            }
        ]
    })
}

#[test]
fn template_prefers_lowest_priority_visible_entry() {
    let bundled = fake_bundled();
    let template = catalog::pick_template(&bundled).unwrap();
    assert_eq!(template["slug"], "tpl-flagship");
}

#[test]
fn built_catalog_overrides_ids_and_clears_openai_only_fields() {
    let bundled = fake_bundled();
    let template = catalog::pick_template(&bundled).unwrap();
    let models = vec!["deepseek-v4".to_string(), "glm-5".to_string()];
    let built = catalog::build(template, &models, "glm-5").unwrap();

    let entries = built["models"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    // 默认模型排第一
    assert_eq!(entries[0]["slug"], "glm-5");
    assert_eq!(entries[1]["slug"], "deepseek-v4");
    assert_eq!(entries[0]["priority"], 1);
    assert_eq!(entries[0]["visibility"], "list");
    assert_eq!(entries[0]["supported_in_api"], true);
    assert_eq!(entries[0]["default_reasoning_level"], "high");
    // 四档思考等级，顺序固定
    let efforts: Vec<&str> = entries[0]["supported_reasoning_levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|level| level["effort"].as_str().unwrap())
        .collect();
    assert_eq!(efforts, ["medium", "high", "xhigh", "max"]);
    // 模板里的 instructions 原样带过来（codex 认这个字段而不是旧的 base_instructions）
    assert_eq!(
        entries[0]["model_messages"]["instructions_template"],
        "FLAGSHIP-PROMPT"
    );
    assert!(entries[0].get("base_instructions").is_none());
    // model_messages 的必填键一个都不能缺（缺失会让 codex 解析整个目录失败）
    for key in [
        "instructions_template",
        "instructions_variables",
        "approvals",
        "collaboration_modes",
        "auto_review",
        "permissions",
        "multi_agent",
    ] {
        assert!(
            entries[0]["model_messages"].get(key).is_some(),
            "model_messages 缺键 {key}"
        );
    }
    // 模板自带的 OpenAI 专属迁移/加速档信息全部清掉
    for key in [
        "upgrade",
        "availability_nux",
        "service_tiers",
        "experimental_supported_tools",
    ] {
        assert!(
            entries[0].get(key).is_none() || entries[0][key] == json!([]),
            "{key} 没清干净"
        );
    }
    // 工具形态跟着模板走（instructions 与实际声明的工具必须一致）
    assert_eq!(entries[0]["apply_patch_tool_type"], "freeform");
    assert_eq!(entries[0]["shell_type"], "unified_exec");
}

#[test]
fn legacy_base_instructions_are_promoted_when_template_messages_missing() {
    let template = json!({
        "slug": "tpl",
        "display_name": "Tpl",
        "priority": 1,
        "visibility": "list",
        "supported_in_api": true,
        "base_instructions": "LEGACY-PROMPT"
    });
    let built = catalog::build(&template, &["m1".to_string()], "m1").unwrap();
    assert_eq!(
        built["models"][0]["model_messages"]["instructions_template"],
        "LEGACY-PROMPT"
    );
}

#[test]
fn build_rejects_empty_model_list() {
    let built = catalog::build(&fake_bundled()["models"][0], &[], "m");
    assert!(built.is_err());
}

// ---- config.toml 改写 ---------------------------------------------------

#[test]
fn config_write_preserves_comments_and_other_providers() {
    let dir = temp_dir("config-preserve");
    let path = dir.join("config.toml");
    let original = r#"# 顶栏注释：别丢
model = "gpt-6-sol"   # 手写的行尾注释
model_provider = "custom"
model_reasoning_effort = "high"

[model_providers.custom]
name = "老的"
base_url = "https://old.example/v1"
wire_api = "responses"

[projects."/tmp/demo"]
trust_level = "trusted"

[tui]
screen_reader_detection_done = true
"#;
    fs::write(&path, original).unwrap();

    let mut doc = config_file::read(&path).unwrap();
    config_file::apply(
        &mut doc,
        &config_file::ProviderWrite {
            key: "ikun",
            name: "ikun",
            base_url: "https://api.ikuncode.cc/v1",
            api_key: "sk-placeholder",
            catalog_file: CATALOG_FILE,
            model: "gpt-6-sol",
        },
    )
    .unwrap();
    let backup = config_file::write(&path, &doc.to_string())
        .unwrap()
        .unwrap();
    let text = fs::read_to_string(&path).unwrap();

    // 注释留着
    assert!(text.contains("# 顶栏注释：别丢"), "{text}");
    assert!(text.contains("# 手写的行尾注释"), "{text}");
    // 用户自己的键与旧 provider 块一个都没丢
    assert!(text.contains("model_reasoning_effort = \"high\""));
    assert!(text.contains("[model_providers.custom]"));
    assert!(text.contains("https://old.example/v1"));
    assert!(text.contains("[projects.\"/tmp/demo\"]"));
    assert!(text.contains("screen_reader_detection_done = true"));
    // 新内容写进去了
    assert!(text.contains("model_provider = \"ikun\""));
    assert!(text.contains("model_catalog_json = \"apim-models.json\""));
    assert!(text.contains("[model_providers.ikun]"));
    assert!(text.contains("wire_api = \"responses\""));
    assert!(text.contains("experimental_bearer_token = \"sk-placeholder\""));
    // 备份是改写前的原文
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    // 重新解析必须还是合法 TOML
    let reparsed: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(reparsed["model_provider"].as_str(), Some("ikun"));
}

#[test]
fn config_write_rejects_unparseable_file() {
    let dir = temp_dir("config-broken");
    let path = dir.join("config.toml");
    fs::write(&path, "model = = =").unwrap();
    let err = config_file::read(&path).unwrap_err();
    assert!(err.contains("未改动"), "{err}");
    // 原文件没被动过
    assert_eq!(fs::read_to_string(&path).unwrap(), "model = = =");
}

#[test]
fn config_write_creates_file_from_scratch() {
    let dir = temp_dir("config-fresh");
    let path = dir.join("config.toml");
    let mut doc = config_file::read(&path).unwrap();
    config_file::apply(
        &mut doc,
        &config_file::ProviderWrite {
            key: "ikun",
            name: "ikun",
            base_url: "https://api.ikuncode.cc/v1",
            api_key: "sk-placeholder",
            catalog_file: CATALOG_FILE,
            model: "m1",
        },
    )
    .unwrap();
    assert!(
        config_file::write(&path, &doc.to_string())
            .unwrap()
            .is_none()
    );
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("[model_providers.ikun]"), "{text}");
    // 隐式表不该产生空的 [model_providers] 头
    assert!(!text.contains("\n[model_providers]\n"), "{text}");
}

// ---- 端到端（假 codex）-------------------------------------------------

/// 造一个假 codex：`debug models --bundled` 吐模板，`debug models` 吐当前
/// CODEX_HOME 下的目录，用真实子进程跑通「生成 → 校验」整条链路。
#[cfg(unix)]
fn fake_codex(dir: &Path, bundled: &Value) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bundled_path = dir.join("bundled.json");
    fs::write(&bundled_path, serde_json::to_string(bundled).unwrap()).unwrap();
    let bin = dir.join("codex");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then\n\
             \x20 if [ \"$3\" = \"--bundled\" ]; then cat '{bundled}'; exit 0; fi\n\
             \x20 cat \"$CODEX_HOME/{catalog}\"; exit 0\n\
             fi\n\
             exit 1\n",
            bundled = bundled_path.display(),
            catalog = CATALOG_FILE,
        ),
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

#[cfg(unix)]
#[test]
fn import_end_to_end_writes_config_catalog_and_verifies() {
    let dir = temp_dir("import-e2e");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join("config.toml"),
        "model = \"gpt-6-sol\"\nmodel_provider = \"old\"\n\n[model_providers.old]\nname = \"old\"\nbase_url = \"https://old.example/v1\"\n",
    )
    .unwrap();
    let bundled = fake_bundled();
    let bin = fake_codex(&dir, &bundled);

    let request = ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder".into(),
        alias: "codex".into(),
        models: vec!["gpt-6-sol".into(), "glm-5".into()],
        default_model: "glm-5".into(),
    };
    let report = import_in(&home, &request, Some(&bin)).unwrap();

    assert!(report.verified);
    assert_eq!(report.provider_key, "ikun");
    assert_eq!(report.model, "glm-5");
    assert_eq!(
        report.backup_path.as_deref(),
        Some(home.join("config.toml.apim.bak").as_path())
    );
    assert!(home.join("config.toml.apim.bak").exists(), "改写前应留备份");

    let text = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(text.contains("model = \"glm-5\""), "{text}");
    assert!(text.contains("model_provider = \"ikun\""), "{text}");
    assert!(
        text.contains("base_url = \"https://api.ikuncode.cc/v1\""),
        "{text}"
    );
    // 旧 provider 块保留（codex 允许多块共存，只是不激活）
    assert!(text.contains("[model_providers.old]"), "{text}");

    let catalog: Value =
        serde_json::from_str(&fs::read_to_string(home.join(CATALOG_FILE)).unwrap()).unwrap();
    let slugs: Vec<&str> = catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["glm-5", "gpt-6-sol"]);
    assert!(!dir.join("codex-home").join("catalog.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn import_fails_when_codex_does_not_recognize_a_model() {
    let dir = temp_dir("import-verify-fail");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    // 假 codex 的 verify 分支永远吐空目录 → 校验必须失败
    let bin = dir.join("codex");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then\n\
         \x20 if [ \"$3\" = \"--bundled\" ]; then echo '{\"models\":[{\"slug\":\"tpl\",\"priority\":1,\"visibility\":\"list\",\"supported_in_api\":true,\"base_instructions\":\"P\",\"model_messages\":{\"instructions_template\":\"P\"}}]}'; exit 0; fi\n\
         \x20 echo '{\"models\":[]}'; exit 0\n\
         fi\n\
         exit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();

    let request = ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder".into(),
        alias: "codex".into(),
        models: vec!["gpt-6-sol".into()],
        default_model: "gpt-6-sol".into(),
    };
    let err = import_in(&home, &request, Some(&bin)).unwrap_err();
    assert!(err.contains("未识别"), "{err}");
}

#[test]
fn import_without_codex_binary_reports_actionable_error() {
    let dir = temp_dir("import-no-codex");
    let request = ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder".into(),
        alias: "codex".into(),
        models: vec!["m1".into()],
        default_model: "m1".into(),
    };
    let err = import_in(&dir, &request, None).unwrap_err();
    assert!(err.contains("APIM_CODEX_BIN"), "{err}");
    // 失败时不能留下半截配置
    assert!(!dir.join("config.toml").exists());
}

// ---- 状态文件 ----------------------------------------------------------

#[test]
fn state_roundtrip_and_key_id() {
    let dir = temp_dir("state");
    let state = CodexState {
        provider: "ikun".into(),
        provider_name: "ikun".into(),
        alias: "codex".into(),
        provider_key: "ikun".into(),
        models: vec!["a".into(), "b".into()],
        default_model: "a".into(),
    };
    store::save(&dir, &state).unwrap();
    assert_eq!(store::load(&dir), Some(state.clone()));
    assert_eq!(state.key_id(), "ikun.codex");
}

#[test]
fn missing_state_loads_as_none() {
    let dir = temp_dir("state-missing");
    assert_eq!(store::load(&dir), None);
}

// ---- 真机端到端（默认忽略：CI 没有 codex）-------------------------------

/// 需要本机真实 codex 的端到端校验：以**真实的 `~/.codex/config.toml`** 为输入，
/// 生成目录 → 改写配置 → 让 codex 自己解析这份新配置并确认勾选的模型都在。
///
/// 手动跑：`cargo test -- --ignored codex_real_end_to_end --nocapture`
/// 全程只写 target/ 下的临时目录，不碰真实 `~/.codex`。
#[cfg(unix)]
#[test]
#[ignore = "需要本机安装 codex；用 cargo test -- --ignored 手动跑"]
fn codex_real_end_to_end() {
    let Some(bin) = catalog::locate_codex() else {
        eprintln!("跳过：没找到 codex 可执行文件");
        return;
    };
    let dir = temp_dir("real");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();

    // 拿用户的真实配置当输入，验证注释 / projects / tui / 旧 provider 块都不丢
    let real_config = codex_home().join("config.toml");
    let original = fs::read_to_string(&real_config).ok();
    if let Some(text) = &original {
        fs::write(home.join("config.toml"), text).unwrap();
    }

    let request = ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun 中转".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder-not-a-real-key".into(),
        alias: "codex".into(),
        models: vec!["gpt-6-sol".into(), "deepseek-v4".into()],
        default_model: "gpt-6-sol".into(),
    };
    let report = import_in(&home, &request, Some(&bin)).expect("真机导入应成功");
    assert!(report.verified, "codex 应能解析新配置与生成目录");

    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    let catalog = fs::read_to_string(home.join(CATALOG_FILE)).unwrap();
    eprintln!(
        "codex {} → 目录 {} 字节 / {} 个模型；配置 {} 字节",
        bin.display(),
        catalog.len(),
        report.models.len(),
        written.len()
    );
    eprintln!("---- 生成目录里的条目 ----");
    let parsed: Value = serde_json::from_str(&catalog).unwrap();
    for entry in parsed["models"].as_array().unwrap() {
        eprintln!(
            "  {} priority={} visibility={} apply_patch={:?} levels={}",
            entry["slug"],
            entry["priority"],
            entry["visibility"],
            entry["apply_patch_tool_type"],
            entry["supported_reasoning_levels"]
                .as_array()
                .map(|levels| levels
                    .iter()
                    .map(|level| level["effort"].as_str().unwrap_or("?").to_string())
                    .collect::<Vec<_>>()
                    .join("/"))
                .unwrap_or_default()
        );
    }

    // 真实配置里原有的东西一个都不能丢
    if let Some(original) = original {
        for marker in [
            "[projects.",
            "[tui]",
            "model_reasoning_effort",
            "[model_providers.",
        ] {
            if original.contains(marker) {
                assert!(
                    written.contains(marker),
                    "改写后丢了原有内容 {marker}\n{written}"
                );
            }
        }
        assert!(
            written.contains("[model_providers.ikun]"),
            "应新增 ikun 的 provider 块\n{written}"
        );
    }
}
