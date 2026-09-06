//! 厂商的保存与删除：写 ~/.config/apim/recipes/<id>.yaml。

use std::collections::HashMap;
use std::time::Instant;

use super::{App, Focus, Modal};
use crate::form::Form;
use crate::recipe::{self, Auth, AuthKind, HttpCall, Recipe, ScriptSpec};

impl App {
    pub(crate) fn save_provider_form(&mut self, form: &Form, original: Option<&str>) {
        use crate::form::{PF_BASE, PF_HEALTH, PF_HOMEPAGE, PF_ID, PF_NAME, PF_SCRIPT};

        let id = match original {
            Some(id) => id.to_string(),
            None => form.text(PF_ID).trim().to_string(),
        };
        let name = form.text(PF_NAME).trim().to_string();
        let mut base_url = form.text(PF_BASE).trim().to_string();
        let homepage = form.text(PF_HOMEPAGE).trim().to_string();
        let health_path = normalize_path(form.text(PF_HEALTH).trim());
        let script_cmd = form.text(PF_SCRIPT).trim().to_string();

        if original.is_none() {
            if id.is_empty() {
                self.set_form_error("ID 必填");
                return;
            }
            if !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                self.set_form_error("ID 只能用小写字母、数字、-");
                return;
            }
            if self.recipes.contains_key(&id) {
                self.set_form_error(&format!("厂商 {id} 已存在"));
                return;
            }
        }
        if name.is_empty() {
            self.set_form_error("名称必填");
            return;
        }
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            self.set_form_error("Base URL 要以 http(s):// 开头");
            return;
        }
        let homepage_ok = homepage.is_empty()
            || homepage.starts_with("http://")
            || homepage.starts_with("https://");
        if !homepage_ok {
            self.set_form_error("主页 URL 要以 http(s):// 开头");
            return;
        }
        base_url = base_url.trim_end_matches('/').to_string();
        if !script_cmd.is_empty()
            && !std::path::Path::new(&crate::probe::expand_tilde(&script_cmd)).is_file()
        {
            self.set_form_error(&format!("脚本不存在：{script_cmd}"));
            return;
        }

        let mut recipe = match original {
            Some(id) => self
                .recipes
                .get(id)
                .cloned()
                .unwrap_or_else(|| default_recipe(id, &name, &base_url)),
            None => default_recipe(&id, &name, &base_url),
        };
        recipe.name = name;
        recipe.base_url = base_url;
        recipe.homepage = (!homepage.is_empty()).then_some(homepage);
        recipe.normalize();

        // 探活：路径没变就保留原有 headers/body
        let existing_health = recipe.health.clone();
        recipe.health = if health_path.is_empty() {
            None
        } else {
            let unchanged = existing_health
                .as_ref()
                .is_some_and(|h| url_suffix(&h.url) == health_path);
            if unchanged {
                existing_health
            } else {
                Some(HttpCall::get(format!("{{base_url}}{health_path}")))
            }
        };

        // 额度：脚本路径是唯一入口（决策见 merged_balance）。
        recipe.balance = merged_balance(&recipe.balance, &script_cmd);

        let path = match recipe::save_user_recipe(&recipe) {
            Ok(path) => path,
            Err(err) => {
                self.toast = Some((format!("保存失败: {err}"), Instant::now()));
                self.modal = Modal::None;
                return;
            }
        };
        recipe.origin = Some(path);
        self.recipes.insert(id.clone(), recipe);
        self.modal = Modal::None;
        self.toast = Some((format!("已保存厂商 {id}"), Instant::now()));
        self.rebuild_provider_list();
        self.selected_provider = self
            .provider_ids_filtered()
            .iter()
            .position(|p| p == &id)
            .unwrap_or(self.selected_provider);
        self.selected_key = 0;
        self.focus = Focus::Keys;
    }

    pub(crate) fn confirm_delete_provider(&mut self) {
        let Modal::ConfirmDeleteProvider { provider_id, path } = &self.modal else {
            return;
        };
        let provider_id = provider_id.clone();
        let path = path.clone();
        match recipe::delete_user_recipe(&path) {
            Ok(()) => self.toast = Some((format!("已删除厂商 {provider_id}"), Instant::now())),
            Err(err) => self.toast = Some((format!("删除失败: {err}"), Instant::now())),
        }
        self.recipes.remove(&provider_id);
        self.states
            .retain(|id, _| !id.starts_with(&format!("{provider_id}.")));
        self.modal = Modal::None;
        self.rebuild_provider_list();
    }
}

fn normalize_path(p: &str) -> String {
    if p.is_empty() || p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}

fn url_suffix(url: &str) -> String {
    url.strip_prefix("{base_url}").unwrap_or(url).to_string()
}

fn default_recipe(id: &str, name: &str, base_url: &str) -> Recipe {
    Recipe {
        id: id.into(),
        name: name.into(),
        base_url: base_url.into(),
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
        health: None,
        balance: None,
        origin: None,
    }
}

/// 保存厂商时的额度合并决策：脚本路径是额度的唯一入口。
/// - 路径没变 → 原样保留（手写 recipe 的 run/timeout 等字段不动）
/// - 留空 → 解绑（None）；内联 run 脚本（表单不回显）不受影响
/// - 新路径 → 绑定为新的 Script{command}
fn merged_balance(existing: &Option<ScriptSpec>, script_cmd: &str) -> Option<ScriptSpec> {
    if script_cmd.is_empty() {
        let keep = existing
            .as_ref()
            .is_some_and(|s| s.command.is_none() && s.run.is_some());
        if keep { existing.clone() } else { None }
    } else {
        let unchanged = existing
            .as_ref()
            .is_some_and(|s| s.command.as_deref() == Some(script_cmd));
        if unchanged {
            existing.clone()
        } else {
            Some(ScriptSpec {
                command: Some(script_cmd.to_string()),
                ..ScriptSpec::default()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表单回显过的外部脚本额度；带非默认 timeout，验证「路径未变时原样保留」。
    fn command_balance(path: &str) -> Option<ScriptSpec> {
        Some(ScriptSpec {
            command: Some(path.to_string()),
            run: None,
            shell: None,
            timeout_secs: Some(30),
        })
    }

    #[test]
    fn same_script_path_keeps_existing_untouched() {
        let existing = command_balance("~/quota.sh");
        let merged = merged_balance(&existing, "~/quota.sh");
        let spec = merged.as_ref().unwrap();
        assert_eq!(spec.command.as_deref(), Some("~/quota.sh"));
        assert_eq!(spec.timeout_secs, Some(30));
    }

    #[test]
    fn empty_cmd_keeps_inline_run_script() {
        let existing = Some(ScriptSpec {
            command: None,
            run: Some("echo 1".into()),
            shell: None,
            timeout_secs: None,
        });
        let merged = merged_balance(&existing, "");
        let spec = merged.as_ref().unwrap();
        assert_eq!(spec.command, None);
        assert_eq!(spec.run.as_deref(), Some("echo 1"));
    }

    #[test]
    fn empty_cmd_unbinds_command_script() {
        assert!(merged_balance(&command_balance("~/quota.sh"), "").is_none());
    }

    #[test]
    fn empty_cmd_with_no_balance_stays_none() {
        assert!(merged_balance(&None, "").is_none());
    }

    #[test]
    fn new_script_path_binds_fresh_command() {
        let existing = command_balance("~/old.sh");
        let merged = merged_balance(&existing, "~/new.sh");
        let spec = merged.as_ref().unwrap();
        assert_eq!(spec.command.as_deref(), Some("~/new.sh"));
        // 新绑定是干净的默认 spec，不继承旧脚本的字段。
        assert_eq!(spec.timeout_secs, None);
    }
}
