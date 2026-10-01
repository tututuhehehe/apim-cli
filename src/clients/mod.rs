//! 客户端（agent）适配：把 apim 里的密钥 + 厂商 + 模型一键写进外部 CLI 的配置。
//!
//! 现在只有 Codex。**加一个新客户端（Claude Code / pi …）要动的地方是固定的三处**：
//!
//! 1. 下面给 [`Agent`] 加一个变体（所有 `match` 都会被编译器强制补全）；
//! 2. 加一个 `clients/<id>/` 子模块，实现「写哪里 / 怎么写 / 怎么写完后校验 / 怎么重新加载」；
//! 3. `Agent` 的新方法里加一条分派（通常一行）。
//!
//! 面板（`app/import`、`ui/import`）**不需要改**：它只经 `Agent` 的这些方法调客户端，
//! 文案也全部取自 `label()/config_hint()/note()`。
//!
//! 各家的「导入规则」不一样（配置格式、鉴权变量名、有没有模型清单、生效方式都不同），
//! 所以**不用 YAML 配方把客户端数据化**：那是厂商协议层的事（见 AGENTS.md 约定 2 ——
//! 同一协议的参数差异才走数据），客户端之间不是同一套协议，各写一个 Rust 子模块更直白。

pub mod codex;

pub use codex::{DEFAULT_EFFORT, ImportReport, ImportRequest, RestartReport};

use std::collections::HashMap;
use std::path::Path;

/// 一键导入的目标客户端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Agent {
    Codex,
}

/// 「上次导入」的摘要：面板用它显示「上次导入的是谁」，密钥栏用它打 ★。
///
/// 由各客户端适配层自己产出（它才知道自家配置里写了什么），面板不解析客户端细节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastImport {
    /// apim 的密钥 id（`厂商.别名`）。
    pub key_id: String,
    /// 一句话摘要（模型数 / 默认模型 / 思考强度 / 配置表名…）。
    pub summary: String,
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
    ///
    /// 返回 `String` 而不是字面量：有些客户端的位置由环境变量决定（`CODEX_HOME`），
    /// 写死 `~/.codex/...` 在那种情况下会骗人。
    pub fn config_hint(self) -> String {
        match self {
            Agent::Codex => crate::clients::codex::config_hint(),
        }
    }

    /// 这个客户端的一条现状说明，面板里显示。
    pub fn note(self) -> &'static str {
        match self {
            Agent::Codex => {
                "同一时刻只激活一个厂商；旧厂商的配置块会保留（方便切回），里面的旧 token 需手动删"
            }
        }
    }

    /// 真正写盘：把请求落到客户端配置（各家自己实现格式与校验）。
    pub fn import(self, request: &ImportRequest) -> Result<ImportReport, String> {
        match self {
            Agent::Codex => codex::import(request),
        }
    }

    /// 导入成功后记录 apim 侧状态（各客户端写自己的文件）。返回写失败的说明（不影响导入结果）。
    pub fn remember(
        self,
        config_dir: &Path,
        request: &ImportRequest,
        report: &ImportReport,
    ) -> Option<String> {
        match self {
            Agent::Codex => codex::remember(config_dir, request, report).1,
        }
    }

    /// 读「上次导入」的摘要（面板与 ★ 用）。
    pub fn last_import(self, config_dir: &Path) -> Option<LastImport> {
        match self {
            Agent::Codex => codex::current_state(config_dir).map(|state| LastImport {
                key_id: state.key_id(),
                summary: format!(
                    "{}（表名 {}）· 默认 {} · 强度 {} · {} 个模型",
                    state.provider_name,
                    state.provider_key,
                    state.default_model,
                    state.reasoning_effort,
                    state.models.len()
                ),
            }),
        }
    }

    /// 导入后要不要让客户端重新加载配置（有的客户端要重启进程，有的不用）。
    pub fn needs_reload(self) -> bool {
        match self {
            Agent::Codex => true,
        }
    }

    /// 让客户端重新加载配置（杀 daemon / 重载 / 什么都不做，各家自己实现）。
    pub fn reload(self) -> Result<RestartReport, String> {
        match self {
            Agent::Codex => codex::restart_daemon(),
        }
    }

    /// 一次性读全部客户端的「上次导入」——`App` 启动时用它填面板状态。
    pub fn load_all_last_imports(config_dir: &Path) -> HashMap<Agent, LastImport> {
        Agent::ALL
            .iter()
            .filter_map(|agent| agent.last_import(config_dir).map(|record| (*agent, record)))
            .collect()
    }
}
