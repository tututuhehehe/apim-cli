//! `apim uninstall` 的沙盒测试：都在临时目录里造文件，绝不动真实的 apim / ~/.config。

use super::cleanup;
use super::*;

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("apim-uninstall-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `--purge` 删掉整个配置目录（密钥也在里面）。
#[test]
fn purge_removes_config_dir() {
    let dir = tmp("purge").join("apim");
    fs::create_dir_all(dir.join("recipes")).unwrap();
    fs::write(dir.join("secrets.toml"), "sk-nope").unwrap();
    assert_eq!(
        cleanup::purge_config_dir(&dir).unwrap(),
        Some(dir.display().to_string())
    );
    assert!(!dir.exists(), "配置目录应当被删掉");
}

/// 目录名不是 apim（APIM_CONFIG_DIR 指到别处）= 拒绝，且原样保留。
#[test]
fn purge_refuses_foreign_dir_name() {
    let dir = tmp("purge-foreign").join("not-apim");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("keep.txt"), "keep").unwrap();
    assert!(cleanup::purge_config_dir(&dir).is_err());
    assert!(dir.join("keep.txt").exists(), "拒绝后必须原样保留");
}

#[test]
fn purge_missing_dir_is_noop() {
    assert_eq!(
        cleanup::purge_config_dir(&tmp("purge-missing").join("apim")).unwrap(),
        None
    );
}

/// 软链（`~/.local/bin/apim` → 别处）要连链一起删：只删真身会留下断链。
#[cfg(unix)]
#[test]
fn removes_binary_and_its_symlink() {
    let base = tmp("symlink");
    let real = base.join("real").join("apim");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, "#!/bin/sh\n").unwrap();
    let link = base.join("bin").join("apim");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(cleanup::binary_targets(&link).len(), 2, "软链 + 真身都要删");

    cleanup::remove_binary(&link).unwrap();
    assert!(fs::symlink_metadata(&real).is_err(), "真身应当被删");
    assert!(fs::symlink_metadata(&link).is_err(), "软链不该留下断链");
    // 幂等：再删一次不报错
    cleanup::remove_binary(&link).unwrap();
}

/// 实名（非软链）的裸二进制只报一条路径，别把 `/var` → `/private/var` 这类
/// 系统软链解析出来的另一个名字也算成「删了两个文件」。
#[test]
fn plain_binary_has_one_target() {
    let path = tmp("plain").join("apim");
    fs::write(&path, "x").unwrap();
    assert_eq!(cleanup::binary_targets(&path), vec![path.clone()]);
}

/// 文件名不是 apim 时拒绝删除（防误删）。
#[test]
fn refuses_unexpected_binary_name() {
    let path = tmp("unexpected-name").join("somethingelse");
    fs::write(&path, "x").unwrap();
    assert!(cleanup::remove_binary(&path).is_err());
    assert!(path.exists());
}

#[test]
fn uninstall_commands_match_channel() {
    let exe = Path::new("/usr/local/bin/apim");
    assert_eq!(
        uninstall_command(Channel::Npm, exe),
        "npm uninstall -g apim-cli"
    );
    assert_eq!(
        uninstall_command(Channel::Homebrew, exe),
        "brew uninstall apim"
    );
    assert_eq!(
        uninstall_command(Channel::Binary, exe),
        "rm /usr/local/bin/apim"
    );
}

/// Codex 残留只看 apim 自己写过的两处：模型目录 JSON + config.toml 里的指针。
#[test]
fn codex_leftovers_follows_apim_files() {
    let home = tmp("codex-home");
    assert!(
        cleanup::codex_leftovers_in(&home).is_empty(),
        "没导入过就没有残留"
    );

    fs::write(home.join("apim-models.json"), "{}").unwrap();
    fs::write(
        home.join("config.toml"),
        "model_catalog_json = \"apim-models.json\"\n",
    )
    .unwrap();
    let found = cleanup::codex_leftovers_in(&home);
    assert_eq!(
        found.len(),
        2,
        "目录文件与 config.toml 指针各报一条：{found:?}"
    );
}
