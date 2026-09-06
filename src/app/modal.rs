//! 弹窗生命周期：打开、分发保存/删除确认、取消。

use std::path::PathBuf;
use std::time::Instant;

use super::{App, Focus};
use crate::clipboard;
use crate::form::{self, Form};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Key,
    Provider,
}

/// 检查器的目标：存 ID，数据在渲染时从 App 现查（自动刷新后内容同步更新）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorTarget {
    Provider(String),
    Key(String),
}

pub enum Modal {
    None,
    Form {
        kind: FormKind,
        form: Form,
        original: Option<String>,
    },
    ConfirmDeleteKey {
        key_id: String,
    },
    ConfirmDeleteProvider {
        provider_id: String,
        path: PathBuf,
    },
    /// 只读详情检查器（i 键打开）。reveal_token 只对 Key 目标有意义。
    Inspector {
        target: InspectorTarget,
        reveal_token: bool,
    },
}

impl App {
    pub fn open_add(&mut self) {
        match self.focus {
            Focus::Providers => {
                self.modal = Modal::Form {
                    kind: FormKind::Provider,
                    form: form::provider_add(),
                    original: None,
                };
            }
            Focus::Keys => {
                let providers = self.provider_ids.clone();
                if providers.is_empty() {
                    self.toast = Some(("先添加一个厂商".into(), Instant::now()));
                    return;
                }
                self.modal = Modal::Form {
                    kind: FormKind::Key,
                    form: form::key_add(providers, self.selected_provider),
                    original: None,
                };
            }
        }
    }

    pub fn open_edit(&mut self) {
        match self.focus {
            Focus::Providers => self.open_edit_provider(),
            Focus::Keys => self.open_edit_key(),
        }
    }

    fn open_edit_provider(&mut self) {
        let Some(id) = self.current_provider_id().map(String::from) else {
            return;
        };
        let Some(recipe) = self.recipes.get(&id) else {
            return;
        };
        let strip = |url: &str| url.strip_prefix("{base_url}").unwrap_or(url).to_string();
        let health_path = recipe
            .health
            .as_ref()
            .map(|h| strip(&h.url))
            .unwrap_or_default();
        self.modal = Modal::Form {
            kind: FormKind::Provider,
            form: form::provider_edit(recipe, &health_path),
            original: Some(id),
        };
    }

    fn open_edit_key(&mut self) {
        let Some(key) = self.selected_key_entry().cloned() else {
            self.toast = Some(("没有可编辑的密钥".into(), Instant::now()));
            return;
        };
        let providers = self.provider_ids.clone();
        let selected = providers
            .iter()
            .position(|p| p == &key.provider)
            .unwrap_or(0);
        self.modal = Modal::Form {
            kind: FormKind::Key,
            form: form::key_edit(
                providers,
                selected,
                &key.alias,
                key.group.as_deref().unwrap_or(""),
                &key.token,
            ),
            original: Some(key.id()),
        };
    }

    pub fn open_delete(&mut self) {
        match self.focus {
            Focus::Providers => {
                let Some(id) = self.current_provider_id().map(String::from) else {
                    return;
                };
                let n = self.keys.iter().filter(|k| k.provider == id).count();
                if n > 0 {
                    self.toast = Some((
                        format!("该厂商下还有 {n} 条密钥，先删除密钥"),
                        Instant::now(),
                    ));
                    return;
                }
                let Some(recipe) = self.recipes.get(&id) else {
                    return;
                };
                match &recipe.origin {
                    Some(path) => {
                        self.modal = Modal::ConfirmDeleteProvider {
                            provider_id: id,
                            path: path.clone(),
                        };
                    }
                    None => {
                        self.toast =
                            Some(("内置厂商不可删除，可用 e 编辑覆盖".into(), Instant::now()));
                    }
                }
            }
            Focus::Keys => {
                let Some(key) = self.selected_key_entry() else {
                    self.toast = Some(("没有可删除的密钥".into(), Instant::now()));
                    return;
                };
                self.modal = Modal::ConfirmDeleteKey { key_id: key.id() };
            }
        }
    }

    pub fn cancel_modal(&mut self) {
        self.modal = Modal::None;
    }

    pub fn form(&mut self) -> Option<&mut Form> {
        match &mut self.modal {
            Modal::Form { form, .. } => Some(form),
            _ => None,
        }
    }

    pub fn save_form(&mut self) {
        let Modal::Form {
            kind,
            form,
            original,
        } = &self.modal
        else {
            return;
        };
        let kind = *kind;
        let form = form.clone();
        let original = original.clone();
        match kind {
            FormKind::Key => self.save_key_form(&form, original.as_deref()),
            FormKind::Provider => self.save_provider_form(&form, original.as_deref()),
        }
    }

    pub(crate) fn set_form_error(&mut self, msg: &str) {
        if let Some(form) = self.form() {
            form.error = Some(msg.into());
        }
    }

    pub fn confirm_delete(&mut self) {
        match &self.modal {
            Modal::ConfirmDeleteKey { .. } => self.confirm_delete_key(),
            Modal::ConfirmDeleteProvider { .. } => self.confirm_delete_provider(),
            Modal::None | Modal::Form { .. } | Modal::Inspector { .. } => {}
        }
    }

    /// 焦点处按 i：打开对应详情检查器（厂商栏→当前厂商；密钥栏→选中密钥）。
    pub fn open_inspector(&mut self) {
        match self.focus {
            Focus::Providers => {
                let Some(id) = self.current_provider_id().map(String::from) else {
                    return;
                };
                self.modal = Modal::Inspector {
                    target: InspectorTarget::Provider(id),
                    reveal_token: false,
                };
            }
            Focus::Keys => {
                let Some(key) = self.selected_key_entry() else {
                    self.toast = Some(("没有可查看的密钥".into(), Instant::now()));
                    return;
                };
                self.modal = Modal::Inspector {
                    target: InspectorTarget::Key(key.id()),
                    reveal_token: false,
                };
            }
        }
    }

    /// 检查器里按 r：仅密钥详情且有 token 时在 遮掩/完整 间切换。
    pub fn inspector_toggle_reveal(&mut self) {
        let Modal::Inspector { target, .. } = &self.modal else {
            return;
        };
        let InspectorTarget::Key(id) = target else {
            return;
        };
        let has_token = self
            .keys
            .iter()
            .any(|k| &k.id() == id && !k.token.is_empty());
        if !has_token {
            return;
        }
        if let Modal::Inspector { reveal_token, .. } = &mut self.modal {
            *reveal_token = !*reveal_token;
        }
    }

    /// 检查器里按 c：复制完整 token（仅密钥详情）。toast 只带别名，不带密钥原文。
    pub fn inspector_copy_token(&mut self) {
        let Modal::Inspector { target, .. } = &self.modal else {
            return;
        };
        let InspectorTarget::Key(id) = target else {
            return;
        };
        let Some(key) = self.keys.iter().find(|k| &k.id() == id) else {
            return;
        };
        let alias = key.alias.clone();
        let token = key.token.clone();
        match clipboard::copy(&token) {
            Ok(()) => self.toast = Some((format!("已复制 {alias} 的完整密钥"), Instant::now())),
            Err(err) => self.toast = Some((format!("复制失败: {err}"), Instant::now())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeyEntry;
    use crate::recipe::Recipe;
    use std::collections::{HashMap, HashSet};

    fn one_key_app() -> App {
        let mut recipes = HashMap::new();
        recipes.insert(
            "p".to_string(),
            Recipe {
                id: "p".into(),
                name: "P".into(),
                base_url: "https://p.example".into(),
                homepage: None,
                supports_groups: false,
                vars: HashMap::new(),
                auth: Default::default(),
                health: None,
                balance: None,
                origin: None,
            },
        );
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App {
            recipes,
            keys: vec![KeyEntry {
                provider: "p".into(),
                alias: "main".into(),
                group: None,
                token: "sk-full-token-value".into(),
            }],
            provider_ids: Vec::new(),
            selected_provider: 0,
            selected_key: 0,
            focus: Focus::Keys,
            states: HashMap::new(),
            toast: None,
            last_refresh: None,
            modal: Modal::None,
            inflight: HashSet::new(),
            tx,
            client: crate::probe::client().expect("client"),
            next_auto_refresh: Instant::now() + std::time::Duration::from_secs(300),
        };
        app.rebuild_provider_list();
        app
    }

    #[test]
    fn inspector_reveal_resets_on_reopen_and_provider_target_ignores_r() {
        let mut app = one_key_app();

        // 打开 → 显隐 → 关闭 → 再打开：reveal 必须回落为默认遮掩
        app.focus = Focus::Keys;
        app.open_inspector();
        app.inspector_toggle_reveal();
        assert!(matches!(
            &app.modal,
            Modal::Inspector {
                reveal_token: true,
                ..
            }
        ));
        app.cancel_modal();
        app.open_inspector();
        assert!(
            matches!(
                &app.modal,
                Modal::Inspector {
                    reveal_token: false,
                    ..
                }
            ),
            "重开检查器必须回到遮掩态"
        );

        // Provider 目标按 r 是无操作（不切换、不 panic）
        app.cancel_modal();
        app.focus = Focus::Providers;
        app.open_inspector();
        app.inspector_toggle_reveal();
        assert!(matches!(
            &app.modal,
            Modal::Inspector {
                reveal_token: false,
                ..
            }
        ));
    }

    #[test]
    fn inspector_copy_requires_key_target() {
        let mut app = one_key_app();
        // Provider 目标按 c：无操作（不复制、无 toast）
        app.focus = Focus::Providers;
        app.open_inspector();
        app.inspector_copy_token();
        assert!(app.toast.is_none(), "厂商详情按 c 不应触发复制");
    }
}
