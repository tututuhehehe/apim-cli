//! 真机端到端（默认忽略：CI 没有 codex）。

use std::fs;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

use crate::clients::codex::catalog::{CATALOG_FILE, locate_codex};
use crate::clients::codex::codex_home;
use crate::clients::codex::import::import_in;
use crate::clients::codex::official::{self, AUTH_FILE};
use crate::openai_auth::Credential;

use super::helpers::{request_for, temp_dir};

/// 需要本机真实 codex 的端到端校验：以**真实的 `~/.codex/config.toml`** 为输入，
/// 生成目录 → 改写配置 → 让 codex 自己解析这份新配置并确认勾选的模型都在。
///
/// 手动跑：`cargo test -- --ignored codex_real_end_to_end --nocapture`
/// 全程只写 target/ 下的临时目录，不碰真实 `~/.codex`。
#[cfg(unix)]
#[test]
#[ignore = "需要本机安装 codex；用 cargo test -- --ignored 手动跑"]
fn codex_real_end_to_end() {
    let Some(bin) = locate_codex() else {
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

    let request = request_for(&["deepseek-flash", "deepseek-chat"]);
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

/// 真机官方路端到端（默认忽略）：用**结构合法的合成凭据**（不读你真实的 token ——
/// 免得把 refresh token 写进 `target/`）走一遍 `official::import_in`，
/// 让真 codex 的 `login status` 认下这份 `auth.json`，并确认第三方路由被摘掉、手写段没丢。
///
/// 手动跑：`cargo test -- --ignored codex_real_official_end_to_end --nocapture`
/// 全程只写 target/ 下的临时目录，不碰真实 `~/.codex`，也不重启任何守护进程。
#[cfg(unix)]
#[test]
#[ignore = "需要本机安装 codex；用 cargo test -- --ignored 手动跑"]
fn codex_real_official_end_to_end() {
    let Some(bin) = locate_codex() else {
        eprintln!("跳过：没找到 codex 可执行文件");
        return;
    };
    let dir = temp_dir("real-official");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();

    // 拿用户的真实配置当输入：官方路要摘掉第三方路由，而注释 / projects / tui 一个不能丢
    let original = fs::read_to_string(codex_home().join("config.toml")).ok();
    if let Some(text) = &original {
        fs::write(home.join("config.toml"), text).unwrap();
    }

    let report = official::import_in(&home, &synthetic_credential(), Some(&bin))
        .expect("真机官方路导入应成功（codex 必须认这份 auth.json）");
    eprintln!(
        "codex {} → 摘掉 {:?}；备份 {:?}",
        bin.display(),
        report.removed,
        report.backups
    );

    let auth: Value =
        serde_json::from_str(&fs::read_to_string(home.join(AUTH_FILE)).unwrap()).unwrap();
    assert_eq!(auth["auth_mode"], "chatgpt");
    assert_eq!(auth["tokens"]["account_id"], "acct-synthetic");

    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    assert!(!written.contains("model_provider = "), "{written}");
    // 目录指针只在它指向 apim 自己生成的那个文件时才被摘；手写的目录按约定原样保留
    // （`strip_keeps_a_hand_written_catalog` + README「手写的 model_catalog_json 也会保留」）。
    // 这里喂进来的是开发者真实机器上的配置，所以断言必须按输入决定：别再改回无条件断言，
    // 那会诱使下一个人为了“修好”这个测试去改 strip_third_party 删掉用户手写的目录。
    if original
        .as_deref()
        .is_some_and(|text| text.contains(CATALOG_FILE))
    {
        assert!(!written.contains("model_catalog_json"), "{written}");
    }
    if let Some(original) = original {
        for marker in ["[projects.", "[tui]", "notify", "[model_providers."] {
            if original.contains(marker) {
                assert!(
                    written.contains(marker),
                    "改写后丢了原有内容 {marker}\n{written}"
                );
            }
        }
    }
}

/// 结构合法的合成凭据：三段式 JWT（base64url 可解） + 全套字段。
/// 真 codex 的 `login status` 会真的解 JWT，乱写字符串它直接报错。
fn synthetic_credential() -> Credential {
    let jwt = |payload: Value| {
        format!(
            "{}.{}.{}",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
            URL_SAFE_NO_PAD.encode(payload.to_string()),
            URL_SAFE_NO_PAD.encode(b"sig")
        )
    };
    let access = jwt(json!({
        "sub": "auth0|synthetic",
        "iat": 1_700_000_000u64,
        "exp": 4_000_000_000u64,
        "https://api.openai.com/auth": { "chatgpt_account_id": "acct-synthetic" },
    }));
    Credential {
        client_id: "app_EMoamEEZ73f0CkXaXp7hrann".into(),
        host_id: "urn:uuid:synthetic".into(),
        subject: "auth0|synthetic".into(),
        email: None,
        id_token: jwt(json!({"sub": "auth0|synthetic"})),
        access_token: access,
        refresh_token: "test-refresh-token".into(),
        expires_at: 4_000_000_000,
        scopes: vec!["openid".into(), "offline_access".into()],
        account_id: "acct-synthetic".into(),
        codex_family: true,
    }
}
