//! 密钥清单：~/.config/apim/config.toml + secrets.toml 的读取。

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::recipe::Recipe;

pub use store::save_keys;
pub(crate) use store::{save_keys_to, tmp_path};

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
    let (items, tokens) = read_config_files(dir)?;
    let mut keys = Vec::new();
    for item in items {
        if !recipes.contains_key(&item.provider) {
            anyhow::bail!(
                "config.toml 里的 provider `{}` 没有对应 recipe。把 YAML 放到 recipes/ 或 ~/.config/apim/recipes/",
                item.provider
            );
        }
        let id = format!("{}.{}", item.provider, item.alias);
        let token = tokens
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

/// CLI 自救用的宽松版：坏条目（provider 无对应 recipe、secrets 缺 token）跳过
/// 而不是整体失败，让 `key ls` / `key rm` 还能跑起来清理现场；被跳过的条目
/// 打 stderr 警告，且下次写盘（save_keys 只写本次返回的条目）会顺带移除。
/// TUI 仍走严格版 load_keys——起不来就提示修配置是合理行为。
pub fn load_keys_lenient(dir: &Path, recipes: &HashMap<String, Recipe>) -> Result<Vec<KeyEntry>> {
    let (items, tokens) = read_config_files(dir)?;
    let mut keys = Vec::new();
    for item in items {
        let id = format!("{}.{}", item.provider, item.alias);
        if !recipes.contains_key(&item.provider) {
            eprintln!(
                "警告：跳过 {id}：provider `{}` 没有对应 recipe；下次写盘会移除该条目",
                item.provider
            );
            continue;
        }
        let Some(token) = tokens.get(&id).cloned().filter(|s| !s.is_empty()) else {
            eprintln!("警告：跳过 {id}：secrets.toml 缺少 tokens.\"{id}\"；下次写盘会移除该条目");
            continue;
        };
        keys.push(KeyEntry {
            provider: item.provider,
            alias: item.alias,
            group: item.group.filter(|s| !s.is_empty()),
            token,
        });
    }
    Ok(keys)
}

/// 读 config.toml + secrets.toml 的原始条目，解析失败仍算硬错误（两个加载版共用）。
fn read_config_files(dir: &Path) -> Result<(Vec<KeyConfig>, HashMap<String, String>)> {
    let config_path = dir.join("config.toml");
    if !config_path.exists() {
        return Ok((Vec::new(), HashMap::new()));
    }

    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let cfg: ConfigFile =
        toml::from_str(&raw).with_context(|| format!("parse {}", config_path.display()))?;

    let secrets_path = dir.join("secrets.toml");
    let tokens = if secrets_path.exists() {
        let raw = fs::read_to_string(&secrets_path)
            .with_context(|| format!("read {}", secrets_path.display()))?;
        toml::from_str::<SecretsFile>(&raw)
            .with_context(|| format!("parse {}", secrets_path.display()))?
            .tokens
    } else {
        HashMap::new()
    };
    Ok((cfg.keys, tokens))
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
