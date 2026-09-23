//! 用户 recipe 的读写：~/.config/apim/recipes/*.yaml。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::Recipe;

pub fn user_recipes_dir() -> PathBuf {
    crate::config::config_dir().join("recipes")
}
pub fn delete_user_recipe(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("delete {}", path.display()))
}

pub(crate) fn save_user_recipe_to(dir: &Path, recipe: &Recipe) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
    let path = dir.join(format!("{}.yaml", recipe.id));
    let yaml = serde_yaml::to_string(recipe).context("serialize recipe")?;
    // recipe 可能带 vars 里的访问令牌，按私钥文件权限写。
    let tmp = crate::config::tmp_path(&path);
    fs::write(&tmp, yaml).with_context(|| format!("write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod {}", tmp.display()))?;
    }
    fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    use super::super::{Auth, AuthKind, HttpCall, Recipe, ScriptSpec, load_dir};
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-recipe-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn sample_user_recipe() -> Recipe {
        Recipe {
            id: "my-relay".into(),
            name: "我的中转站".into(),
            base_url: "https://relay.example.com/v1".into(),
            homepage: None,
            models_url: None,
            supports_groups: false,
            vars: HashMap::new(),
            auth: Auth {
                kind: AuthKind::Bearer,
                header: None,
                prefix: None,
                query_param: None,
            },
            health: Some(HttpCall::get("{base_url}/models")),
            balance: Some(ScriptSpec {
                command: Some("~/.config/apim/scripts/my-relay-quota.sh".into()),
                run: None,
                shell: None,
                timeout_secs: Some(20),
            }),
            origin: None,
        }
    }

    #[test]
    fn user_recipe_save_roundtrip() {
        let dir = temp_dir("roundtrip");
        let recipe = sample_user_recipe();
        let path = save_user_recipe_to(&dir, &recipe).unwrap();
        assert!(path.ends_with("my-relay.yaml"));

        let mut map = HashMap::new();
        load_dir(&dir, &mut map).unwrap();
        let loaded = map.get("my-relay").expect("recipe loaded");
        assert_eq!(loaded.name, "我的中转站");
        assert_eq!(loaded.base_url, "https://relay.example.com/v1");
        assert_eq!(loaded.origin.as_deref(), Some(path.as_path()));
        let balance = loaded.balance.as_ref().unwrap();
        assert_eq!(
            balance.command.as_deref(),
            Some("~/.config/apim/scripts/my-relay-quota.sh")
        );
        assert_eq!(balance.timeout_secs, Some(20));
        delete_user_recipe(&path).unwrap();
        assert!(!path.exists());
        assert_no_tmp_leftover(&dir);
    }

    fn assert_no_tmp_leftover(dir: &PathBuf) {
        let leftovers: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "残留 tmp 文件: {leftovers:?}");
    }
}
