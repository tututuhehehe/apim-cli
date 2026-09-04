//! 密钥清单：~/.config/apim/config.toml + secrets.toml 的读取。

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::recipe::Recipe;

pub use store::save_keys;

mod store;

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
pub(crate) struct ConfigFile {
    #[serde(default)]
    keys: Vec<KeyConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct KeyConfig {
    pub provider: String,
    pub alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SecretsFile {
    #[serde(default)]
    tokens: HashMap<String, String>,
}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = env::var("APIM_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    home_dir().join(".config/apim")
}

fn home_dir() -> PathBuf {
    if let Ok(h) = env::var("HOME") {
        return PathBuf::from(h);
    }
    PathBuf::from(".")
}

pub fn load_keys(recipes: &HashMap<String, Recipe>) -> Result<Vec<KeyEntry>> {
    load_keys_from(&config_dir(), recipes)
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

pub fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 12 {
        return "••••".into();
    }
    let prefix: String = chars.iter().take(8).collect();
    let suffix: String = chars.iter().rev().take(4).rev().collect();
    format!("{prefix}…{suffix}")
}
