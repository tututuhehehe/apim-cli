//! 密钥清单写回磁盘：原子写入，权限 600。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{ConfigFile, KeyConfig, KeyEntry};

pub(crate) fn save_keys_to(dir: &Path, keys: &[KeyEntry]) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;

    let cfg = ConfigFile {
        keys: keys
            .iter()
            .map(|k| KeyConfig {
                provider: k.provider.clone(),
                alias: k.alias.clone(),
                group: k.group.clone().filter(|s| !s.is_empty()),
            })
            .collect(),
    };
    let config_toml = toml::to_string_pretty(&cfg).context("serialize config.toml")?;
    write_private(&dir.join("config.toml"), &config_toml)?;
    write_private(&dir.join("secrets.toml"), &render_secrets(keys))?;
    Ok(())
}

fn render_secrets(keys: &[KeyEntry]) -> String {
    let mut out = String::from("[tokens]\n");
    for key in keys {
        out.push_str(&format!("\"{}\" = {}\n", key.id(), toml_string(&key.token)));
    }
    out
}

fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// tmp 文件名带进程 id（`<原名>.<pid>.tmp`）：两个 apim 实例并发保存时
/// 不互踩同一个 tmp，中断残留的 tmp 也不会互相干扰。
pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("apim");
    path.with_file_name(format!("{name}.{}.tmp", std::process::id()))
}

/// 原子私有写入（tmp 带 pid + 600 权限 + rename）。密钥类文件统一走它。
pub(crate) fn write_private(path: &Path, contents: &str) -> Result<()> {
    let tmp = tmp_path(path);
    write_new_private(&tmp, contents).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

/// 新建/覆盖一个私有文件：unix 上**建文件时就带 0600**。
///
/// 不能写成「先 `fs::write` 再 `chmod`」：那中间有一个「文件已存在但仍是 0644」的窗口，
/// 进程在这时被 Ctrl-C / 杀软中断，就会把密钥文件永久留成全局可读。
fn write_new_private(path: &Path, contents: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())?;
        // 先把内容刷到盘再让调用方 rename：否则崩溃后可能留下「文件在、内容是空的」
        // 的密钥文件（ext4 等会暴露 rename 之后但数据未落盘的窗口）。
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::super::{KeyEntry, load_keys_from};
    use super::*;
    use crate::recipe;

    fn deepseek_recipes() -> HashMap<String, recipe::Recipe> {
        // 沙盒目录加载 builtin，绝不读真实 ~/.config/apim（用户 recipe 会污染测试）
        recipe::load_recipes_with(Path::new("target/apim-store-tests-no-user-dir")).unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-config-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(alias: &str, group: Option<&str>, token: &str) -> KeyEntry {
        KeyEntry {
            provider: "deepseek".into(),
            alias: alias.into(),
            group: group.map(String::from),
            token: token.into(),
        }
    }

    #[test]
    fn missing_config_is_empty() {
        let dir = temp_dir("empty");
        fs::create_dir_all(&dir).unwrap();
        let keys = load_keys_from(&dir, &deepseek_recipes()).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let original = vec![
            entry("home", None, "sk-home-token"),
            entry("work", Some("vip"), r#"sk-"quoted"\token"#),
        ];
        save_keys_to(&dir, &original).unwrap();
        let loaded = load_keys_from(&dir, &deepseek_recipes()).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].alias, "home");
        assert_eq!(loaded[0].group, None);
        assert_eq!(loaded[0].token, "sk-home-token");
        assert_eq!(loaded[1].alias, "work");
        assert_eq!(loaded[1].group.as_deref(), Some("vip"));
        assert_eq!(loaded[1].token, r#"sk-"quoted"\token"#);
        let secrets = fs::read_to_string(dir.join("secrets.toml")).unwrap();
        assert!(secrets.contains("\"deepseek.home\""));
        assert!(secrets.contains("\"deepseek.work\""));
        assert_no_tmp_leftover(&dir);
    }

    fn assert_no_tmp_leftover(dir: &Path) {
        let leftovers: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "残留 tmp 文件: {leftovers:?}");
    }

    #[test]
    fn rename_is_just_save_with_new_alias() {
        let dir = temp_dir("rename");
        save_keys_to(&dir, &[entry("default", None, "sk-keep")]).unwrap();
        save_keys_to(&dir, &[entry("home", None, "sk-keep")]).unwrap();
        let loaded = load_keys_from(&dir, &deepseek_recipes()).unwrap();
        assert_eq!(loaded[0].alias, "home");
        assert_eq!(loaded[0].token, "sk-keep");
        let secrets = fs::read_to_string(dir.join("secrets.toml")).unwrap();
        assert!(!secrets.contains("deepseek.default"));
        assert!(secrets.contains("\"deepseek.home\""));
    }
}
