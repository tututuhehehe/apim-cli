//! 测试夹具：临时目录、假 pi 脚本、请求字面量。

use std::fs;
use std::path::{Path, PathBuf};

use crate::clients::ImportRequest;

/// 本次测试专用的临时目录（`target/` 下，名字带测试名 + pid，每次重建）。
pub(super) fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("apim-pi-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 测试用的导入请求：厂商固定 ikun，模型由调用方给（pi 不需要默认模型，恒为 `None`）。
pub(super) fn request_for(models: &[&str]) -> ImportRequest {
    ImportRequest {
        provider_id: "ikun".into(),
        provider_name: "ikun".into(),
        base_url: "https://api.ikuncode.cc".into(),
        api_key: "sk-placeholder".into(),
        models: models.iter().map(|m| (*m).to_string()).collect(),
        // pi 不需要默认模型（面板也不问）
        default_model: None,
    }
}

/// 造一个假 pi：`--list-models` 时把 `<agent-dir>/fake-list.txt` 原样打印（没有就什么都不打）。
///
/// 校验逻辑是「解析 pi 列出来的表」，所以让测试用一张假表来精确控制「列出哪些模型」。
#[cfg(unix)]
pub(super) fn fake_pi(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("pi");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--list-models\" ]; then\n\
           cat \"${PI_CODING_AGENT_DIR}/fake-warning.txt\" >&2 2>/dev/null\n\
           cat \"${PI_CODING_AGENT_DIR}/fake-list.txt\" 2>/dev/null\n\
           exit 0\n\
         fi\n\
         exit 1\n",
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// 让假 pi 退出码为 1（模拟「pi 读不懂这份配置」）。
#[cfg(unix)]
pub(super) fn fake_pi_failing(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("pi-fail");
    fs::write(&bin, "#!/bin/sh\necho 'no such config' >&2\nexit 1\n").unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// 假 pi 会列出的那张表（表头 + 我们的 provider + 勾选的全部模型）。
#[cfg(unix)]
pub(super) fn write_model_table(dir: &Path, key: &str, models: &[&str]) {
    let mut text = String::from("provider   model        context  max-out  thinking  images\n");
    for model in models {
        text.push_str(&format!("{key}  {model}  128K  16.4K  no  no\n"));
    }
    text.push_str("opencode  gpt-6-sol  1.1M  128K  yes  yes\n");
    fs::write(dir.join("fake-list.txt"), text).unwrap();
}
