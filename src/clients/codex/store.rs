//! `~/.config/apim/codex.toml`：记录 apim 最后一次导入到 codex 的内容。
//!
//! 只用于展示：TUI 给当前激活的密钥打 ★、面板上显示「当前已导入 xxx」。
//! 判定「codex 里现在是什么」以 codex 自己的 config.toml 为准，不反向依赖这个文件。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 状态文件名（放在 apim 配置目录下）。
pub const FILE: &str = "codex.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexState {
    /// apim 的厂商 id。
    pub provider: String,
    /// 厂商显示名。
    pub provider_name: String,
    /// apim 的密钥别名。
    pub alias: String,
    /// 写进 codex 的 `[model_providers.<key>]` 表名。
    pub provider_key: String,
    /// 导入的模型（按优先级序）。
    pub models: Vec<String>,
    /// 默认模型（codex 顶层 `model`）。
    pub default_model: String,
    /// 默认思考强度（codex 顶层 `model_reasoning_effort`）。
    pub reasoning_effort: String,
}

impl CodexState {
    /// 与 `KeyEntry::id()` 同构，用于在密钥行上打 ★。
    pub fn key_id(&self) -> String {
        format!("{}.{}", self.provider, self.alias)
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// 读状态；文件不存在或内容坏了都当「没有导入过」（只是展示信息，不值得报错）。
pub fn load(dir: &Path) -> Option<CodexState> {
    let text = std::fs::read_to_string(path(dir)).ok()?;
    toml::from_str(&text).ok()
}

pub fn save(dir: &Path, state: &CodexState) -> Result<(), String> {
    let text = toml::to_string_pretty(state).map_err(|err| format!("序列化状态失败：{err}"))?;
    crate::config::write_private(&path(dir), &text)
        .map_err(|err| format!("写 {} 失败：{err}", path(dir).display()))
}
