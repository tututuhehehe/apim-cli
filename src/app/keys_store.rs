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
        // 先写盘再改内存：写盘失败时内存不变，不会出现“内存有了磁盘没有”的分歧
        let mut next = self.keys.clone();
        match original {
            None => next.push(entry),
            Some(orig) => {
                if let Some(slot) = next.iter_mut().find(|k| k.id() == orig) {
                    *slot = entry;
                } else {
                    self.modal = Modal::None;
                    return;
                }
            }
        }

        if let Err(err) = config::save_keys_to(&self.config_dir, &next) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
            self.modal = Modal::None;
            return;
        }
        self.keys = next;
        if let Some(orig) = original {
            // 旧 token 的读数与在途探针一并作废，让下面的 refresh 真正重发
            self.invalidate_key(orig);
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
        let fallback = self.selected_provider();
        let pos = self
            .provider_ids_filtered()
            .iter()
            .position(|p| p == &provider)
            .unwrap_or(fallback);
        self.tab_view_mut().selected = pos;
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
        // 先写盘再改内存（失败时内存保持原样）
        let next: Vec<KeyEntry> = self
            .keys
            .iter()
            .filter(|k| k.id() != key_id)
            .cloned()
            .collect();
        if let Err(err) = config::save_keys_to(&self.config_dir, &next) {
            self.toast = Some((format!("保存失败: {err}"), Instant::now()));
        } else {
            self.keys = next;
            if let Some(key) = removed {
                self.push_undo(UndoAction::KeyDeleted { key });
            }
            self.toast = Some((format!("已删除 {key_id}"), Instant::now()));
        }
        self.invalidate_key(&key_id);
        self.modal = Modal::None;
        // rebuild 内部会把选中项钳回（过滤后的）有效范围
        self.rebuild_provider_list();
        if !self.keys_in_provider_filtered().is_empty() {
            self.refresh_current_provider();
        }
    }
}
