//! 厂商的保存与删除：写 ~/.config/apim/recipes/<id>.yaml。

use std::collections::HashMap;
use std::time::Instant;

use super::{App, Focus, Modal};
use crate::form::Form;
use crate::recipe::{
    self, Auth, AuthKind, BalanceSpec, HttpCall, ParseSpec, Recipe, RenderField, RenderSpec,
};

impl App {
    pub(crate) fn save_provider_form(&mut self, form: &Form, original: Option<&str>) {
        let id = match original {
            Some(id) => id.to_string(),
            None => form.text(0).trim().to_string(),
        };
        let name = form.text(1).trim().to_string();
        let mut base_url = form.text(2).trim().to_string();
        let health_path = normalize_path(form.text(3).trim());
        let balance_path = normalize_path(form.text(4).trim());
        let balance_json = form.text(5).trim().to_string();

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
        base_url = base_url.trim_end_matches('/').to_string();
        if !balance_path.is_empty() && balance_json.is_empty() {
            self.set_form_error("配了额度路径就要填取值路径");
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

        // 额度：路径和取值都没动就保留原有 parse/render（DeepSeek 这类）
        let existing_balance = recipe.balance.clone();
        recipe.balance = if balance_path.is_empty() {
            None
        } else {
            let unchanged = existing_balance.as_ref().is_some_and(|b| {
                url_suffix(&b.request.url) == balance_path && balance_json.is_empty()
            });
            if unchanged {
                existing_balance
            } else {
                Some(generic_balance(&balance_path, &balance_json))
            }
        };

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
            .provider_ids
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
        supports_groups: false,
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

fn generic_balance(path: &str, json_path: &str) -> BalanceSpec {
    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "application/json".to_string());
    BalanceSpec {
        request: HttpCall {
            method: "GET".into(),
            url: format!("{{base_url}}{path}"),
            headers,
            body: None,
        },
        parse: ParseSpec {
            available: None,
            items: None,
            fields: HashMap::from([("total_balance".to_string(), json_path.to_string())]),
        },
        render: RenderSpec {
            headline: "{total_balance}".into(),
            fields: vec![RenderField {
                label: "额度".into(),
                value: "{total_balance}".into(),
            }],
        },
    }
}
