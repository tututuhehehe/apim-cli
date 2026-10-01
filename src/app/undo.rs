//! 面板内的写操作历史与 Ctrl+Z 撤销。
//!
//! 只记录**可逆的写操作**（增删改厂商 / 密钥，含复制产生的文件），
//! 探活、复制到剪贴板、打开主页、浏览与搜索等只读动作不进历史。
//! 历史是本次打开面板的会话内状态，退出即清空。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};

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
    /// path / script_copy 都是写盘时记录的真实路径，不靠 id 反推。
    ProviderCreated {
        id: String,
        path: PathBuf,
        script_copy: Option<PathBuf>,
        copied: bool,
    },
    /// 编辑厂商：撤销 = 写回旧 recipe（按其 origin 真实路径）；
    /// 旧的是内置（origin=None）时删掉本次新建的覆盖 YAML 即可。
    ProviderUpdated {
        id: String,
        before: Recipe,
        path: PathBuf,
    },
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
                id,
                script_copy,
                copied,
                ..
            } => {
                let what = if *copied { "复制" } else { "新增" };
                let script = script_copy
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|f| format!("（含脚本 {}）", f.to_string_lossy()))
                    .unwrap_or_default();
                format!("{what}厂商 {id}{script}")
            }
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
            Err(err) => {
                // 失败不丢历史：放回栈尾让用户可重试（删除/写盘都幂等）
                self.undo_stack.push_back(action);
                self.toast = Some((format!("撤销 {label} 失败: {err}"), Instant::now()));
            }
        }
        self.rebuild_provider_list();
        // 撤销可能把「客户端正在用的那把密钥」放回来（或改回去），★ 要跟着重算
        self.refresh_active_keys();
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
                id,
                path,
                script_copy,
                ..
            } => {
                // 尽力而为：两个文件都尝试删，最后统一报错（内存状态照样回退）
                let mut failed: Vec<String> = Vec::new();
                let mut targets = vec![path.clone()];
                targets.extend(script_copy.clone());
                for file in &targets {
                    if let Err(err) = remove_file_if_exists(file) {
                        failed.push(err.to_string());
                    }
                }
                self.recipes.remove(id);
                self.invalidate_provider(id);
                if !failed.is_empty() {
                    bail!("{}", failed.join("；"));
                }
            }
            UndoAction::ProviderUpdated { id, before, path } => {
                match before.origin.as_deref() {
                    // 旧内容在手写的另一个文件里（.yml）：写回原文件并清掉本次新建的
                    Some(origin) if origin != path.as_path() => {
                        recipe::write_recipe_file(origin, before)?;
                        remove_file_if_exists(path)?;
                    }
                    Some(_) => recipe::write_recipe_file(path, before)?,
                    // 之前是内置：删掉覆盖 YAML，内置定义自动恢复
                    None => remove_file_if_exists(path)?,
                }
                self.recipes.insert(id.clone(), before.clone());
                self.invalidate_provider(id);
                // 配置回退也是配置变更：按回退后的配置重探，避免面板停在「未检查」
                self.refresh_provider(&before.id.clone());
            }
            UndoAction::ProviderDeleted { recipe: deleted } => {
                // 删除只允许用户 recipe（origin 必为 Some），按原路径写回
                let path = match deleted.origin.clone() {
                    Some(path) => path,
                    None => self.recipes_dir().join(format!("{}.yaml", deleted.id)),
                };
                recipe::write_recipe_file(&path, deleted)?;
                let mut restored = deleted.clone();
                restored.origin = Some(path);
                self.recipes.insert(restored.id.clone(), restored);
                self.invalidate_provider(&deleted.id);
            }
        }
        Ok(())
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
