//! 密钥清单写回磁盘：原子写入，权限 600。

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::{ConfigFile, KeyConfig, KeyEntry, config_dir};

pub fn save_keys(keys: &[KeyEntry]) -> Result<()> {
    save_keys_to(&config_dir(), keys)
}

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

fn write_private(path: &Path, contents: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents).with_context(|| format!("write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod {}", tmp.display()))?;
    }
    fs::rename(&tmp, path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    use super::super::{KeyEntry, load_keys_from};
    use super::*;
    use crate::recipe;

    fn deepseek_recipes() -> HashMap<String, recipe::Recipe> {
        recipe::load_recipes().unwrap()
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
