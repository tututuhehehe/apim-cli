//! `config_file`（`~/.codex/config.toml` 的保注释读写）的测试。

use std::fs;
use std::path::Path;

use crate::clients::codex::config_file;

use super::helpers::{assert_no_tmp, provider_write, temp_dir};

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
    // 特意传一个与原文件里 "high" 不同的强度，下面断言才能证明"被改写"
    config_file::apply(
        &mut doc,
        &provider_write("sk-placeholder", "gpt-6-sol", "xhigh"),
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
    config_file::apply(&mut doc, &provider_write("sk-placeholder", "m1", "high")).unwrap();
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
    config_file::apply(&mut doc, &provider_write("sk-new", "m1", "high")).unwrap();
    let backup = config_file::write(&path, &doc.to_string())
        .unwrap()
        .unwrap();

    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600, "config.toml 必须 600");
    assert_eq!(mode(&backup), 0o600, "备份里有旧 token，必须 600");
    assert!(fs::read_to_string(&backup).unwrap().contains("sk-old"));
    // 收尾不留 tmp
    assert_no_tmp(&dir);
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
    config_file::apply(&mut doc, &provider_write("sk-placeholder", "m1", "high")).unwrap();
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
