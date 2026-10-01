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

/// 生成目录：照官方文档手写的迷你条目（不再克隆 codex 内置的 GPT 模板）。
#[test]
fn built_catalog_matches_official_minimal_shape() {
    let models = vec!["deepseek-flash".to_string(), "deepseek-chat".to_string()];
    let built = catalog::build(&models, "deepseek-chat").unwrap();
    let entries = built["models"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    // 默认模型排第一（priority 决定 codex 选择器里的排序）
    assert_eq!(entries[0]["slug"], "deepseek-chat");
    assert_eq!(entries[1]["slug"], "deepseek-flash");
    assert_eq!(entries[0]["priority"], 1);
    assert_eq!(entries[0]["display_name"], "deepseek-chat");
    // /model 只看 visibility=list；supported_in_api 决定会不会被 auth 过滤掉
    assert_eq!(entries[0]["visibility"], "list");
    assert_eq!(entries[0]["supported_in_api"], true);
    // 思考强度：四档固定，默认档跟着选
    assert_eq!(entries[0]["default_reasoning_level"], "high");
    let efforts: Vec<&str> = entries[0]["supported_reasoning_levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|level| level["effort"].as_str().unwrap())
        .collect();
    assert_eq!(efforts, ["medium", "high", "xhigh", "max"]);
    // 工具形态用官方文档的写法
    assert_eq!(entries[0]["shell_type"], "shell_command");
    assert_eq!(entries[0]["apply_patch_tool_type"], "freeform");
    // 必须有：两者都缺会让 codex 解析整个目录失败
    // base_instructions 是 codex 的必填字段，不能缺也不能给空串
    assert!(
        entries[0]["base_instructions"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "base_instructions 不能为空"
    );
    // 跨 codex 版本的两个必填字段名（cc-switch 记录过踩坑）
    assert_eq!(entries[0]["supports_reasoning_summaries"], true);
    assert_eq!(entries[0]["supports_reasoning_summary_parameter"], true);
    assert_eq!(entries[0]["supports_parallel_tool_calls"], true);
    // 不夹带 GPT 模板的专属字段（上一版就是被这些坑了）
    for key in [
        "model_messages",
        "tool_mode",
        "use_responses_lite",
        "node_repl_auto_review_required",
    ] {
        assert!(entries[0].get(key).is_none(), "{key} 不该出现在条目里");
    }
    // fail-open：与 DeepSeek 官方目录 / cc-switch 模板一致，免得粘图片被拦
    assert_eq!(entries[0]["input_modalities"], json!(["text", "image"]));
    assert_eq!(entries[0]["supports_search_tool"], false);
    // 严格网关会 400 的全分辨率图片能力显式关掉
    assert_eq!(entries[0]["supports_image_detail_original"], false);
    assert_eq!(entries[0]["context_window"], 272000);
    assert_eq!(entries[0]["max_context_window"], 272000);
    // OpenAI 专属的迁移/加速档提示显式清空
    assert_eq!(entries[0]["upgrade"], Value::Null);
    assert_eq!(entries[0]["availability_nux"], Value::Null);
    assert_eq!(entries[0]["service_tiers"], json!([]));
    // 体积：官方迷你条目应该在 1KB 量级（克隆 GPT 模板时是 64KB/模型）
    let size = serde_json::to_string(&built).unwrap().len();
    assert!(size < 4000, "目录应该很小，实际 {size} 字节");
}

#[test]
fn build_rejects_empty_model_list() {
    let built = catalog::build(&[], "m");
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
            // 特意传一个与原文件里 "high" 不同的值，下面断言才能证明"被改写"
            reasoning_effort: "xhigh",
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
    // （`model_reasoning_effort` 的值由 apim 接管，键本身必须还在 —— 值见下面）
    assert!(text.contains("model_reasoning_effort = "));
    assert!(text.contains("[model_providers.custom]"));
    assert!(text.contains("https://old.example/v1"));
    assert!(text.contains("[projects.\"/tmp/demo\"]"));
    assert!(text.contains("screen_reader_detection_done = true"));
    // 新内容写进去了（model_reasoning_effort 被写成请求里的强度，原值是 high）
    assert!(text.contains("model_provider = \"ikun\""));
    assert!(
        text.contains("model_reasoning_effort = \"xhigh\""),
        "{text}"
    );
    assert!(text.contains("model_catalog_json = \"apim-models.json\""));
    // 新建 model_catalog_json 时带一行「模型在哪个文件」的注释，且不能把值挤到下一行
    assert!(text.contains("# 勾选的模型写在这个文件里"), "{text}");
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
            reasoning_effort: "high",
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

/// 造一个假 codex：`debug models` 回吐当前 `CODEX_HOME` 下的目录
/// （apim 用它做「导入是否成功」的端到端校验）。
#[cfg(unix)]
fn fake_codex(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("codex");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then cat \"$CODEX_HOME/{CATALOG_FILE}\"; exit 0; fi\n\
             exit 1\n",
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
    let bin = fake_codex(&dir);

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
    // 目录与配置都写完、正常收尾后不该留任何 tmp
    let leftovers: Vec<String> = fs::read_dir(&home)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "残留 tmp: {leftovers:?}");
}

#[cfg(unix)]
#[test]
fn import_fails_when_codex_does_not_recognize_a_model() {
    let dir = temp_dir("import-verify-fail");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    // 假 codex 的 `debug models` 永远吐空目录 → 校验必须失败
    let bin = dir.join("codex");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then echo '{\"models\":[]}'; exit 0; fi\n\
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
        reasoning_effort: "high".into(),
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
        models: vec!["deepseek-flash".into(), "deepseek-chat".into()],
        default_model: "deepseek-flash".into(),
    };
    let report = import_in(&home, &request, Some(&bin)).expect("真机导入应成功");

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

// ---- 重启 codex 守护进程的进程匹配 --------------------------------------

/// 匹配必须严格：只杀真正的 codex app-server，不能误杀用户会话或无关进程。
#[cfg(unix)]
#[test]
fn codex_server_matching_is_strict() {
    // 真 daemon（daemon + pid-update-loop 两个形态）
    assert!(is_codex_server(
        " 87965 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex app-server --listen unix:// --managed-daemon"
    ));
    assert!(is_codex_server(
        " 87990 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex app-server daemon pid-update-loop"
    ));
    // 用户的 codex 会话：没有 app-server 子命令 → 不杀
    assert!(!is_codex_server(" 86760 node /opt/homebrew/bin/codex"));
    assert!(!is_codex_server(
        " 86761 /opt/homebrew/lib/node_modules/@openai/codex/vendor/codex"
    ));
    // 命令行里恰好含这两个词的无关进程（apim 自己的测试/ps|grep）→ 绝不能杀
    assert!(!is_codex_server(
        " 92087 /opt/homebrew/bin/bash -c echo \"codex app-server\" | grep foo"
    ));
    assert!(!is_codex_server(
        " 92091 awk /codex/ && /app-server/ {print}"
    ));
    // code-mode host 是另一个二进制名 → 不在范围内
    assert!(!is_codex_server(
        " 8272 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex-code-mode-host"
    ));
    // 用户自己的会话里恰好出现 app-server 这个词 → 绝不能杀（子命令位不是它）
    assert!(!is_codex_server(
        " 100 /opt/homebrew/bin/codex --profile app-server"
    ));
    assert!(!is_codex_server(
        " 101 /opt/homebrew/bin/codex exec app-server"
    ));
    assert!(!is_codex_server(
        " 102 /opt/homebrew/bin/codex -c model_provider=app-server"
    ));
    // 反向：真实守护形态（子命令就在 argv[2]）必须命中
    assert!(is_codex_server(
        " 103 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex app-server --listen unix://"
    ));
}

// ---- 权限与回滚（review 修的两条 P0/P1 的回归守卫）-------------------------

/// 备份里装着原 config.toml（**含 API key**），权限必须建文件时就是 600。
/// 之前的 `fs::write` 会按 umask 落成 0644 = 把密钥复制一份给全机可读。
#[cfg(unix)]
#[test]
fn backup_and_config_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("perms");
    let path = dir.join("config.toml");
    fs::write(
        &path,
        "model = \"a\"\nexperimental_bearer_token = \"sk-old\"\n",
    )
    .unwrap();

    let mut doc = config_file::read(&path).unwrap();
    config_file::apply(
        &mut doc,
        &config_file::ProviderWrite {
            key: "ikun",
            name: "ikun",
            base_url: "https://api.ikuncode.cc/v1",
            api_key: "sk-new",
            catalog_file: CATALOG_FILE,
            model: "m1",
            reasoning_effort: "high",
        },
    )
    .unwrap();
    let backup = config_file::write(&path, &doc.to_string())
        .unwrap()
        .unwrap();

    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600, "config.toml 必须 600");
    assert_eq!(mode(&backup), 0o600, "备份里有旧 token，必须 600");
    assert!(fs::read_to_string(&backup).unwrap().contains("sk-old"));
    // 收尾不留 tmp
    let leftovers: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "残留 tmp: {leftovers:?}");
}

/// 校验失败必须把两处改动都还原：config.toml 回到导入前，目录文件删掉（原本来没有）。
#[cfg(unix)]
#[test]
fn failed_verify_rolls_back_config_and_catalog() {
    let dir = temp_dir("rollback");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    let original = "model = \"old-model\"\nmodel_provider = \"old\"\n\n[model_providers.old]\nname = \"old\"\nbase_url = \"https://old.example/v1\"\n";
    fs::write(home.join("config.toml"), original).unwrap();

    // 假 codex：`debug models` 永远吐空目录 → 校验必然失败
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("codex");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then echo '{\"models\":[]}'; exit 0; fi\n\
         exit 1\n",
    )
    .unwrap();
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
    assert!(err.contains("已还原"), "错误里应说明已还原：{err}");
    // config.toml 逐字节回到导入前
    assert_eq!(
        fs::read_to_string(home.join("config.toml")).unwrap(),
        original
    );
    // 目录文件本来不存在 → 还原后也不该存在
    assert!(!home.join(CATALOG_FILE).exists(), "目录文件应被删掉");
}

/// dotfiles 常把 `~/.codex/config.toml` 做成符号链接。写入必须跟随到真实文件，
/// 不能把链接本身 rename 成普通文件（否则仓库版本与现场分叉）；
/// 而备份要留在链接旁边（`~/.codex/`），别把含 token 的备份写进用户的 dotfiles 仓库。
#[cfg(unix)]
#[test]
fn write_follows_symlink_and_keeps_backup_out_of_dotfiles() {
    use std::os::unix::fs::symlink;
    let dir = temp_dir("symlink");
    let dotfiles = dir.join("dotfiles");
    let codex_home = dir.join("codex-home");
    fs::create_dir_all(&dotfiles).unwrap();
    fs::create_dir_all(&codex_home).unwrap();

    let real = dotfiles.join("codex-config.toml");
    fs::write(&real, "model = \"old\"\n").unwrap();
    let link = codex_home.join("config.toml");
    symlink(&real, &link).unwrap();

    let mut doc = config_file::read(&link).unwrap();
    config_file::apply(
        &mut doc,
        &config_file::ProviderWrite {
            key: "ikun",
            name: "ikun",
            base_url: "https://api.ikuncode.cc/v1",
            api_key: "sk-placeholder",
            catalog_file: CATALOG_FILE,
            model: "m1",
            reasoning_effort: "high",
        },
    )
    .unwrap();
    let backup = config_file::write(&link, &doc.to_string())
        .unwrap()
        .unwrap();

    // 链接还在（没被替换成普通文件），指向的还是那个真实文件
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "符号链接被替换成了普通文件"
    );
    assert_eq!(fs::read_link(&link).unwrap(), real);
    // 新内容写进了真实文件
    assert!(
        fs::read_to_string(&real)
            .unwrap()
            .contains("model = \"m1\"")
    );
    // 备份在 codex-home 里（不是 dotfiles 仓库），内容是导入前的旧配置
    assert_eq!(backup, codex_home.join("config.toml.apim.bak"));
    assert!(
        fs::read_to_string(&backup)
            .unwrap()
            .contains("model = \"old\"")
    );
    assert!(!dotfiles.join("codex-config.toml.apim.bak").exists());
}

// ---- 并发导入锁（原来地笔记 2.2）------------------------------------------

/// 测试用的导入请求。
fn request_for(alias: &str, models: &[&str]) -> ImportRequest {
    ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder".into(),
        alias: alias.into(),
        models: models.iter().map(|m| (*m).to_string()).collect(),
        default_model: models[0].to_string(),
    }
}

/// 已经有人在导入时，第二个实例必须被挡住，而不是互相覆盖 provider 块。
/// 被挡住时磁盘上一点都不能动。
#[cfg(unix)]
#[test]
fn concurrent_import_is_locked_out() {
    let dir = temp_dir("lock");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    let bin = fake_codex(&dir);

    // 手动占住锁：写当前进程的 pid → 会被判成「还活着」，不会被抢
    fs::write(
        home.join(".apim-import.lock"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    let err = import_in(&home, &request_for("codex", &["m1"]), Some(&bin)).unwrap_err();
    assert!(err.contains("另一个 apim"), "应提示被锁挡住：{err}");
    assert!(!home.join("config.toml").exists(), "被挡住时不该写任何东西");
    assert!(!home.join(CATALOG_FILE).exists(), "被挡住时不该写目录");

    // 释放后能正常导入，且导入结束要把锁清掉（否则下一次永远进不来）
    fs::remove_file(home.join(".apim-import.lock")).unwrap();
    import_in(&home, &request_for("codex", &["m1"]), Some(&bin)).unwrap();
    assert!(!home.join(".apim-import.lock").exists(), "导入结束要清除锁");
}

/// 进程崩溃留下的锁（pid 已不在）要能被下一个实例认领，别把用户永久锁在门外。
#[cfg(unix)]
#[test]
fn stale_lock_is_taken_over() {
    let dir = temp_dir("lock-stale");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    let bin = fake_codex(&dir);
    // 999_999 基本不可能是活着的进程
    fs::write(home.join(".apim-import.lock"), "999999\n").unwrap();

    import_in(&home, &request_for("codex", &["m1"]), Some(&bin)).expect("陈旧锁应被认领");
    assert!(home.join("config.toml").exists());
    assert!(!home.join(".apim-import.lock").exists());
}

/// 内容坏掉的锁也当陈旧（否则一次异常退出就永久占着）。
#[cfg(unix)]
#[test]
fn corrupt_lock_is_treated_as_stale() {
    let dir = temp_dir("lock-corrupt");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    let bin = fake_codex(&dir);
    fs::write(home.join(".apim-import.lock"), "not-a-pid").unwrap();

    import_in(&home, &request_for("codex", &["m1"]), Some(&bin)).expect("坏锁应被认领");
    assert!(!home.join(".apim-import.lock").exists());
}
