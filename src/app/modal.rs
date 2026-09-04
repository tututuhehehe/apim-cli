//! 弹窗生命周期：打开、分发保存/删除确认、取消。

use std::path::PathBuf;
use std::time::Instant;

use super::{App, Focus};
use crate::form::{self, Form};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Key,
    Provider,
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
            Modal::None | Modal::Form { .. } => {}
        }
    }
}
