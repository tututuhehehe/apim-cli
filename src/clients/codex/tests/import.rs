//! 一次导入的编排（`import`）的测试：纯函数、端到端、校验失败回滚、并发导入锁。

use std::fs;

use serde_json::Value;

use crate::clients::codex::catalog::CATALOG_FILE;
use crate::clients::codex::import::{import_in, normalize_base_url, provider_key};

// 未带门的三个测试只用跨平台的 `temp_dir` / `request_for`（纯函数与「没装 codex」的报错），
// 其余测试驱动假 codex 脚本（unix）→ import 拆两行，别整模块带门。
#[cfg(unix)]
use super::helpers::{assert_no_tmp, fake_codex, fake_codex_with_empty_catalog};
use super::helpers::{request_for, temp_dir};

/// 写进去的东西必须能被「回读现场」认出来：writer 与 reader 不能各说各话
/// （导入侧字段名 / base_url 归一化一旦漂移，★ 会静默失效而测试全绿）。
#[cfg(unix)]
#[test]
fn imported_config_is_recognized_as_in_use() {
    let dir = temp_dir("import-recognized");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    let bin = fake_codex(&dir);
    import_in(&home, &request_for(&["glm-5"]), Some(&bin)).unwrap();

    let keys = [crate::config::KeyEntry {
        provider: "ikun".into(),
        alias: "codex".into(),
        group: None,
        token: "sk-placeholder".into(),
    }];
    assert_eq!(
        crate::clients::codex::active::active_key_ids(
            Some(&home),
            &keys,
            &std::collections::HashMap::new()
        ),
        vec!["ikun.codex".to_string()],
        "导入写下的配置，回读现场时必须认出同一把密钥"
    );
}

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

    let request = request_for(&["glm-5", "gpt-6-sol"]);
    let report = import_in(&home, &request, Some(&bin)).unwrap();

    assert_eq!(report.provider_key, "ikun");
    assert_eq!(report.model.as_deref(), Some("glm-5"));
    assert_eq!(report.backups, vec![home.join("config.toml.apim.bak")]);
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
    assert_no_tmp(&home);
}

#[cfg(unix)]
#[test]
fn import_fails_when_codex_does_not_recognize_a_model() {
    let dir = temp_dir("import-verify-fail");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();
    // 假 codex 的 `debug models` 永远吐空目录 → 校验必须失败
    let bin = fake_codex_with_empty_catalog(&dir);

    let err = import_in(&home, &request_for(&["gpt-6-sol"]), Some(&bin)).unwrap_err();
    assert!(err.contains("未识别"), "{err}");
}

#[test]
fn import_without_codex_binary_reports_actionable_error() {
    let dir = temp_dir("import-no-codex");
    let err = import_in(&dir, &request_for(&["m1"]), None).unwrap_err();
    assert!(err.contains("APIM_CODEX_BIN"), "{err}");
    // 失败时不能留下半截配置
    assert!(!dir.join("config.toml").exists());
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
    let bin = fake_codex_with_empty_catalog(&dir);

    let err = import_in(&home, &request_for(&["gpt-6-sol"]), Some(&bin)).unwrap_err();
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

// ---- 并发导入锁（原来地笔记 2.2）------------------------------------------

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

    let err = import_in(&home, &request_for(&["m1"]), Some(&bin)).unwrap_err();
    assert!(err.contains("另一个 apim"), "应提示被锁挡住：{err}");
    assert!(!home.join("config.toml").exists(), "被挡住时不该写任何东西");
    assert!(!home.join(CATALOG_FILE).exists(), "被挡住时不该写目录");

    // 释放后能正常导入，且导入结束要把锁清掉（否则下一次永远进不来）
    fs::remove_file(home.join(".apim-import.lock")).unwrap();
    import_in(&home, &request_for(&["m1"]), Some(&bin)).unwrap();
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

    import_in(&home, &request_for(&["m1"]), Some(&bin)).expect("陈旧锁应被认领");
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

    import_in(&home, &request_for(&["m1"]), Some(&bin)).expect("坏锁应被认领");
    assert!(!home.join(".apim-import.lock").exists());
}
