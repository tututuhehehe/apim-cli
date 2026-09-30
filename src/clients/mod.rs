//! 客户端（agent）适配：把 apim 里的密钥 + 厂商 + 模型一键写进外部 CLI 的配置。
//!
//! 现在只有 Codex。以后加别的客户端（Claude Code 等）只需要在这里加一个 `Agent`
//! 变体 + 一个子模块，上层面板（选择 agent → 勾选模型 → 应用）不用改。

pub mod codex;

pub use codex::{CodexState, DEFAULT_EFFORT, ImportReport, ImportRequest};

/// 一键导入的目标客户端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Codex,
}

impl Agent {
    /// 面板里按顺序列出的全部客户端。加新客户端只改这里。
    pub const ALL: &'static [Agent] = &[Agent::Codex];

    /// 面板显示名。
    pub fn label(self) -> &'static str {
        match self {
            Agent::Codex => "Codex",
        }
    }

    /// 目标配置文件位置，面板上给用户看的提示。
    pub fn config_hint(self) -> &'static str {
        match self {
            Agent::Codex => "~/.codex/config.toml",
        }
    }

    /// 这个客户端的一条现状说明，面板里显示。
    pub fn note(self) -> &'static str {
        match self {
            Agent::Codex => "同一时刻只激活一个厂商；已导入的其它厂商配置块会保留",
        }
    }
}
