//! 测试共用的夹具：临时目录、假 codex 脚本、请求 / 写入内容 / 状态的字面量构造。

use std::fs;
use std::path::{Path, PathBuf};

use crate::clients::codex::ImportRequest;
use crate::clients::codex::catalog::CATALOG_FILE;
use crate::clients::codex::config_file;

/// 本次测试专用的临时目录（`target/` 下，名字带测试名 + pid，每次重建）。
pub(super) fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("apim-codex-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 收尾后目录里不该留任何 `.tmp`（原子写的中间产物）。
#[cfg(unix)]
pub(super) fn assert_no_tmp(dir: &Path) {
    let leftovers: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "残留 tmp: {leftovers:?}");
}

/// 测试用的导入请求：厂商固定 ikun，模型由调用方给（第一个 = 默认模型）。
pub(super) fn request_for(models: &[&str]) -> ImportRequest {
    ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder".into(),
        models: models.iter().map(|m| (*m).to_string()).collect(),
        default_model: models[0].to_string(),
    }
}

/// 测试用的 `config.toml` 写入内容：厂商固定 ikun，只有密钥 / 模型 / 思考强度可变。
pub(super) fn provider_write<'a>(
    api_key: &'a str,
    model: &'a str,
    effort: &'a str,
) -> config_file::ProviderWrite<'a> {
    config_file::ProviderWrite {
        key: "ikun",
        name: "ikun",
        base_url: "https://api.ikuncode.cc/v1",
        api_key,
        catalog_file: CATALOG_FILE,
        model,
        reasoning_effort: effort,
    }
}

/// 造一个假 codex：`debug models` 时执行 `stdout` 那段 shell，其余子命令退出 1。
#[cfg(unix)]
pub(super) fn fake_codex_with(dir: &Path, stdout: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("codex");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then {stdout}; exit 0; fi\n\
             exit 1\n",
        ),
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// 回吐当前 `CODEX_HOME` 下模型目录的假 codex → 端到端校验通过。
#[cfg(unix)]
pub(super) fn fake_codex(dir: &Path) -> PathBuf {
    fake_codex_with(dir, &format!("cat \"$CODEX_HOME/{CATALOG_FILE}\""))
}

/// 永远回吐空目录的假 codex → 端到端校验必然失败（测回滚）。
#[cfg(unix)]
pub(super) fn fake_codex_with_empty_catalog(dir: &Path) -> PathBuf {
    fake_codex_with(dir, "echo '{\"models\":[]}'")
}
