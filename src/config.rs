use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::recipe::Recipe;

#[derive(Debug, Clone)]
pub struct KeyEntry {
    pub provider: String,
    pub alias: String,
    pub group: Option<String>,
    pub token: String,
}

impl KeyEntry {
    pub fn id(&self) -> String {
        format!("{}.{}", self.provider, self.alias)
    }

    pub fn group_label(&self) -> &str {
        match &self.group {
            Some(g) if !g.is_empty() => g.as_str(),
            _ => "—",
        }
    }

    pub fn masked_token(&self) -> String {
        mask_token(&self.token)
    }
}

impl std::fmt::Display for KeyEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} / {}", self.provider, self.alias)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    keys: Vec<KeyConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyConfig {
    provider: String,
    alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SecretsFile {
    #[serde(default)]
    tokens: HashMap<String, String>,
}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("APIM_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    home_dir().join(".config/apim")
}

fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h);
    }
    PathBuf::from(".")
}

pub fn load_keys(recipes: &HashMap<String, Recipe>) -> Result<Vec<KeyEntry>> {
    load_keys_from(&config_dir(), recipes)
}

pub fn save_keys(keys: &[KeyEntry]) -> Result<()> {
    save_keys_to(&config_dir(), keys)
}

pub fn load_keys_from(dir: &Path, recipes: &HashMap<String, Recipe>) -> Result<Vec<KeyEntry>> {
    let config_path = dir.join("config.toml");
    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let cfg: ConfigFile =
        toml::from_str(&raw).with_context(|| format!("parse {}", config_path.display()))?;

    let secrets_path = dir.join("secrets.toml");
    let secrets: SecretsFile = if secrets_path.exists() {
        let raw = fs::read_to_string(&secrets_path)
            .with_context(|| format!("read {}", secrets_path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parse {}", secrets_path.display()))?
    } else {
        SecretsFile::default()
    };

    let mut keys = Vec::new();
    for item in cfg.keys {
        if !recipes.contains_key(&item.provider) {
            anyhow::bail!(
                "config.toml 里的 provider `{}` 没有对应 recipe。把 YAML 放到 recipes/ 或 ~/.config/apim/recipes/",
                item.provider
            );
        }
        let id = format!("{}.{}", item.provider, item.alias);
        let token = secrets
            .tokens
            .get(&id)
            .cloned()
            .filter(|s| !s.is_empty())
            .with_context(|| format!("secrets.toml 缺少 [{id}]，键名应写成 tokens.\"{id}\""))?;
        keys.push(KeyEntry {
            provider: item.provider,
            alias: item.alias,
            group: item.group.filter(|s| !s.is_empty()),
            token,
        });
    }
    Ok(keys)
}

pub fn save_keys_to(dir: &Path, keys: &[KeyEntry]) -> Result<()> {
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

pub fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 12 {
        return "••••".into();
    }
    let prefix: String = chars.iter().take(8).collect();
    let suffix: String = chars.iter().rev().take(4).rev().collect();
    format!("{prefix}…{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe;

    fn deepseek_recipes() -> HashMap<String, Recipe> {
        recipe::load_recipes().unwrap()
    }

    fn temp_dir() -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_config_is_empty() {
        let dir = temp_dir().join("empty");
        fs::create_dir_all(&dir).unwrap();
        let keys = load_keys_from(&dir, &deepseek_recipes()).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir().join("roundtrip");
        let original = vec![
            KeyEntry {
                provider: "deepseek".into(),
                alias: "home".into(),
                group: None,
                token: "sk-home-token".into(),
            },
            KeyEntry {
                provider: "deepseek".into(),
                alias: "work".into(),
                group: Some("vip".into()),
                token: r#"sk-"quoted"\token"#.into(),
            },
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
        let dir = temp_dir().join("rename");
        save_keys_to(
            &dir,
            &[KeyEntry {
                provider: "deepseek".into(),
                alias: "default".into(),
                group: None,
                token: "sk-keep".into(),
            }],
        )
        .unwrap();
        save_keys_to(
            &dir,
            &[KeyEntry {
                provider: "deepseek".into(),
                alias: "home".into(),
                group: None,
                token: "sk-keep".into(),
            }],
        )
        .unwrap();
        let loaded = load_keys_from(&dir, &deepseek_recipes()).unwrap();
        assert_eq!(loaded[0].alias, "home");
        assert_eq!(loaded[0].token, "sk-keep");
        let secrets = fs::read_to_string(dir.join("secrets.toml")).unwrap();
        assert!(!secrets.contains("deepseek.default"));
        assert!(secrets.contains("\"deepseek.home\""));
    }
}
