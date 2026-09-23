//! 密钥的保存与删除：写回 config.toml / secrets.toml。

use std::time::Instant;

use super::undo::UndoAction;
use super::{App, Focus, Modal};
use crate::config::{self, KeyEntry};
use crate::form::Form;

impl App {
    pub(crate) fn save_key_form(&mut self, form: &Form, original: Option<&str>) {
        let provider = form.select_value(0).to_string();
        let alias = form.text(1).trim().to_string();
        let group = form.text(2).trim().to_string();
        let token = form.text(3).trim().to_string();

        if alias.is_empty() {
            self.set_form_error("别名必填");
            return;
        }
        if token.is_empty() {
            self.set_form_error("密钥必填");
            return;
        }
        let new_id = format!("{provider}.{alias}");
        if original != Some(new_id.as_str()) && self.keys.iter().any(|k| k.id() == new_id) {
            self.set_form_error(&format!("{new_id} 已存在"));
            return;
        }

        let entry = KeyEntry {
            provider: provider.clone(),
            alias,
            group: if group.is_empty() { None } else { Some(group) },
            token,
        };
        // 记撤销用：改动前的旧条目（改名的场景 id 也会变）
        let before = original.and_then(|id| self.keys.iter().find(|k| k.id() == id).cloned());
        match original {
            None => self.keys.push(entry),
            Some(orig) => {
                if let Some(slot) = self.keys.iter_mut().find(|k| k.id() == orig) {
                    *slot = entry;
                    self.states.remove(orig);
                } else {
                    self.modal = Modal::None;
                    return;
                }
            }
        }

        if let Err(err) = config::save_keys_to(&self.config_dir, &self.keys) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
            self.modal = Modal::None;
            return;
        }
        match original {
            None => self.push_undo(UndoAction::KeyAdded {
                key_id: new_id.clone(),
            }),
            // 能走到这里说明上面 find 命中了旧条目，before 必为 Some
            Some(_) => {
                if let Some(prev) = before {
                    // key_id 存改后的 id（改名时与旧 id 不同，撤销要把两边都清掉）
                    self.push_undo(UndoAction::KeyUpdated {
                        key_id: new_id.clone(),
                        before: prev,
                    });
                }
            }
        }
        self.modal = Modal::None;
        self.toast = Some((format!("已保存 {new_id}"), Instant::now()));
        self.rebuild_provider_list();
        // 定位走过滤后的视图：有过滤时选中项要落在可见行上
        self.selected_provider = self
            .provider_ids_filtered()
            .iter()
            .position(|p| p == &provider)
            .unwrap_or(self.selected_provider);
        self.selected_key = self
            .keys_in_provider_filtered()
            .iter()
            .position(|i| self.keys[*i].id() == new_id)
            .unwrap_or(0);
        self.focus = Focus::Keys;
        self.refresh_current_provider();
    }

    pub(crate) fn confirm_delete_key(&mut self) {
        let Modal::ConfirmDeleteKey { key_id } = &self.modal else {
            return;
        };
        let key_id = key_id.clone();
        let removed = self.keys.iter().find(|k| k.id() == key_id).cloned();
        self.keys.retain(|k| k.id() != key_id);
        if let Err(err) = config::save_keys_to(&self.config_dir, &self.keys) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
        } else {
            if let Some(key) = removed {
                self.push_undo(UndoAction::KeyDeleted { key });
            }
            self.toast = Some((format!("已删除 {key_id}"), Instant::now()));
        }
        self.states.remove(&key_id);
        self.modal = Modal::None;
        // rebuild 内部会把选中项钳回（过滤后的）有效范围
        self.rebuild_provider_list();
        if !self.keys_in_provider_filtered().is_empty() {
            self.refresh_current_provider();
        }
    }
}
