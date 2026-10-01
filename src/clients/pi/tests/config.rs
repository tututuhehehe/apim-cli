//! 两份 JSON 的读写：保留未知字段、缺文件当空对象、坏文件拒写、权限 600、符号链接跟随。

use std::fs;

use serde_json::json;

use super::helpers::temp_dir;
use crate::clients::pi::config;

#[test]
fn missing_file_reads_as_empty_object() {
    let dir = temp_dir("config-missing");
    assert!(config::read(&config::models_path(&dir)).unwrap().is_empty());
}

#[test]
fn broken_json_is_refused_instead_of_overwritten() {
    let dir = temp_dir("config-broken");
    let path = config::models_path(&dir);
    fs::write(&path, "{ not json").unwrap();
    let err = config::read(&path).unwrap_err();
    assert!(err.contains("未改动"), "{err}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{ not json",
        "不碰坏文件"
    );
}

#[test]
fn top_level_non_object_is_refused() {
    let dir = temp_dir("config-array");
    let path = config::models_path(&dir);
    fs::write(&path, "[1, 2]").unwrap();
    assert!(
        config::read(&path)
            .unwrap_err()
            .contains("顶层不是 JSON 对象")
    );
}

#[test]
fn write_creates_file_with_600_and_a_backup() {
    let dir = temp_dir("config-write");
    let path = config::models_path(&dir);
    let mut map = config::read(&path).unwrap();
    map.insert("providers".into(), json!({}));

    assert_eq!(config::write(&path, &map).unwrap(), None, "新文件没有备份");

    map.insert("extra".into(), json!(1));
    let backup = config::write(&path, &map).unwrap().expect("第二次写要备份");
    assert_eq!(backup, crate::clients::file_io::backup_path_of(&path));
    assert!(backup.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in [&path, &backup] {
            let mode = fs::metadata(file).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "{} 里可能有 apiKey，必须建文件时就是 600",
                file.display()
            );
        }
    }
}

/// 符号链接（dotfiles 常这么管 `~/.pi`）要跟随到真实文件，别把链接换成普通文件。
#[cfg(unix)]
#[test]
fn write_follows_a_symlinked_config() {
    let dir = temp_dir("config-symlink");
    let real = dir.join("dotfiles/models.json");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, "{}\n").unwrap();
    let link = config::models_path(&dir);
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let mut map = config::read(&link).unwrap();
    map.insert("providers".into(), json!({}));
    config::write(&link, &map).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "链接必须还在"
    );
    assert!(
        fs::read_to_string(&real).unwrap().contains("providers"),
        "写到了链接指向的真身"
    );
}

/// 读改写一轮后，用户手写的字段一个都不能少。
#[test]
fn roundtrip_preserves_unknown_fields() {
    let dir = temp_dir("config-preserve");
    let path = config::models_path(&dir);
    fs::write(
        &path,
        r#"{
  "providers": {
    "sensenova": { "apiKey": "sk-x", "unknownThing": [1, 2] }
  },
  "someFutureKey": { "a": true }
}
"#,
    )
    .unwrap();

    let map = config::read(&path).unwrap();
    config::write(&path, &map).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("unknownThing"), "{text}");
    assert!(text.contains("someFutureKey"), "{text}");
    assert!(text.contains("sensenova"), "{text}");
}
