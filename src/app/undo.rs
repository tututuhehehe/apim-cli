//! 面板内的写操作历史与 Ctrl+Z 撤销。
//!
//! 只记录**可逆的写操作**（增删改厂商 / 密钥，含复制产生的文件），
//! 探活、复制到剪贴板、打开主页、浏览与搜索等只读动作不进历史。
//! 历史是本次打开面板的会话内状态，退出即清空。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};

use super::App;
use crate::config::{self, KeyEntry};
use crate::recipe::{self, Recipe};

/// 历史条数上限：只保留最近 N 步，防止长时间开着面板时无限增长。
const MAX_UNDO: usize = 50;

/// 一次可撤销的写操作，存「撤销所需的最小信息」。
#[derive(Debug, Clone)]
pub enum UndoAction {
    /// 新增密钥：撤销 = 删掉它。
    KeyAdded { key_id: String },
    /// 修改密钥：撤销 = 写回旧条目。key_id 是**改后**的 id（改名时会与旧 id 不同），
    /// 撤销时新旧 id 都清掉再放回 before。
    KeyUpdated { key_id: String, before: KeyEntry },
    /// 删除密钥：撤销 = 加回来。
    KeyDeleted { key: KeyEntry },
    /// 新增 / 复制厂商：撤销 = 删掉生成的用户 YAML（复制时连带删掉脚本副本）。
    ProviderCreated {
        id: String,
        script_copy: Option<PathBuf>,
        copied: bool,
    },
    /// 编辑厂商：撤销 = 写回旧 recipe；旧的是内置（origin=None）时删掉用户覆盖即可。
    ProviderUpdated { id: String, before: Recipe },
    /// 删除厂商：撤销 = 重新写回 YAML 与内存。
    ProviderDeleted { recipe: Recipe },
}

impl UndoAction {
    /// toast / 底栏提示用的中文描述。
    pub(crate) fn label(&self) -> String {
        match self {
            Self::KeyAdded { key_id } => format!("新增密钥 {key_id}"),
            Self::KeyUpdated { key_id, .. } => format!("修改密钥 {key_id}"),
            Self::KeyDeleted { key } => format!("删除密钥 {}", key.id()),
            Self::ProviderCreated {
                id, copied: true, ..
            } => format!("复制厂商 {id}"),
            Self::ProviderCreated { id, .. } => format!("新增厂商 {id}"),
            Self::ProviderUpdated { id, .. } => format!("编辑厂商 {id}"),
            Self::ProviderDeleted { recipe } => format!("删除厂商 {}", recipe.id),
        }
    }
}

impl App {
    /// 记录一步写操作（成功落盘后调用）。
    pub(crate) fn push_undo(&mut self, action: UndoAction) {
        if self.undo_stack.len() >= MAX_UNDO {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(action);
    }

    /// Ctrl+Z：回退最近一次写操作（内存 + 磁盘一起回退）。空历史只提示。
    pub fn undo(&mut self) {
        let Some(action) = self.undo_stack.pop_back() else {
            self.toast = Some(("没有可撤销的操作".into(), Instant::now()));
            return;
        };
        let label = action.label();
        match self.apply_undo(&action) {
            Ok(()) => self.toast = Some((format!("已撤销：{label}"), Instant::now())),
            Err(err) => self.toast = Some((format!("撤销 {label} 失败: {err}"), Instant::now())),
        }
        self.rebuild_provider_list();
    }

    fn apply_undo(&mut self, action: &UndoAction) -> Result<()> {
        match action {
            UndoAction::KeyAdded { key_id } => {
                self.keys.retain(|k| k.id() != *key_id);
                self.states.remove(key_id);
                config::save_keys_to(&self.config_dir, &self.keys)?;
            }
            UndoAction::KeyUpdated { key_id, before } => {
                // 改名换过 id：新旧两个 id 都清掉再放回旧条目，保证不留重复
                self.keys
                    .retain(|k| k.id() != *key_id && k.id() != before.id());
                self.keys.push(before.clone());
                self.states.remove(key_id);
                config::save_keys_to(&self.config_dir, &self.keys)?;
            }
            UndoAction::KeyDeleted { key } => {
                if !self.keys.iter().any(|k| k.id() == key.id()) {
                    self.keys.push(key.clone());
                }
                config::save_keys_to(&self.config_dir, &self.keys)?;
            }
            UndoAction::ProviderCreated {
                id, script_copy, ..
            } => {
                self.remove_user_recipe(id)?;
                if let Some(script) = script_copy
                    && script.exists()
                {
                    fs::remove_file(script)
                        .with_context(|| format!("delete {}", script.display()))?;
                }
                self.recipes.remove(id);
                self.drop_provider_states(id);
            }
            UndoAction::ProviderUpdated { id, before } => {
                if before.origin.is_none() {
                    // 之前是内置：删掉用户覆盖，内置定义自动恢复
                    self.remove_user_recipe(id)?;
                } else {
                    // 之前就有用户 YAML：按旧内容重写
                    recipe::save_user_recipe_to(&self.recipes_dir(), before)?;
                }
                self.recipes.insert(id.clone(), before.clone());
                self.drop_provider_states(id);
            }
            UndoAction::ProviderDeleted { recipe: deleted } => {
                let written = recipe::save_user_recipe_to(&self.recipes_dir(), deleted)?;
                let mut restored = deleted.clone();
                restored.origin = Some(written);
                self.recipes.insert(restored.id.clone(), restored);
                self.drop_provider_states(&deleted.id);
            }
        }
        Ok(())
    }

    /// 丢该厂商的缓存探测结果（配置回退后旧数字/状态不再可信，下次刷新重算）。
    fn drop_provider_states(&mut self, id: &str) {
        self.states
            .retain(|key, _| !key.starts_with(&format!("{id}.")));
    }

    /// 删用户 YAML；文件已不在就当删过了（幂等，撤销不因边界状态失败）。
    fn remove_user_recipe(&self, id: &str) -> Result<()> {
        let path = self.recipes_dir().join(format!("{id}.yaml"));
        remove_file_if_exists(&path)
    }
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("delete {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
