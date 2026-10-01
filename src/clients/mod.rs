//! 客户端（agent）适配：把 apim 里的密钥 + 厂商 + 模型一键写进外部 CLI 的配置，
//! 以及**回读客户端现场**回答「它现在真正在用哪把密钥」（密钥行的 ★ 角标）。
//!
//! 现在只有 Codex。**加一个新客户端（Claude Code / pi …）要动的地方是固定的三处**：
//!
//! 1. 下面给 [`Agent`] 加一个变体（所有 `match` 都会被编译器强制补全）；
//! 2. 加一个 `clients/<id>/` 子模块，实现「写哪里 / 怎么写 / 怎么写完后校验 / 怎么重新加载 /
//!    怎么从自家配置里认出正在用的密钥」；
//! 3. `Agent` 的新方法里加一条分派（通常一行）。
//!
//! 面板（`app/import`、`ui/import`）**不需要改**：它只经 `Agent` 的这些方法调客户端，
//! 文案也全部取自 `label()/config_hint()`。客户端细节（配置文件长什么样、要重启什么）
//! 不进面板文案 —— 选择面板就是一行一个客户端名。
//!
//! 各家的「导入规则」不一样（配置格式、鉴权变量名、有没有模型清单、生效方式都不同），
//! 所以**不用 YAML 配方把客户端数据化**：那是厂商协议层的事（见 AGENTS.md 约定 2 ——
//! 同一协议的参数差异才走数据），客户端之间不是同一套协议，各写一个 Rust 子模块更直白。

pub mod codex;

pub use codex::{DEFAULT_EFFORT, ImportReport, ImportRequest, RestartReport};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// 一键导入的目标客户端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Agent {
    Codex,
}

/// 客户端配置里**现在真正生效**的 provider（各客户端子模块读自家配置得出）。
///
/// apim **不记**「上次导入了谁」：★ 只认客户端现场 —— 每次检测都回读客户端的配置文件，
/// 再拿里面的 token / 地址和 apim 的密钥对账。所以用户手改了客户端配置（换 token、
/// 把 `model_provider` 切走、删掉那个 provider 块），★ 会跟着变，而不是留在旧密钥上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProvider {
    /// 客户端配置里激活的 provider 表名（codex：顶层 `model_provider` 指向的
    /// `[model_providers.<key>]`）。
    pub provider_key: String,
    /// 该 provider 的 API 根地址（原样，未归一化）。
    pub base_url: String,
    /// 写在该 provider 上的 bearer token；客户端改用环境变量取 token（`env_key`）时为 None。
    pub token: Option<String>,
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

    /// ★ 角标上的短标。多个客户端同时用同一把密钥时并排显示（`★C`、`★C,P`）。
    pub fn badge(self) -> &'static str {
        match self {
            Agent::Codex => "C",
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

    /// 这个客户端**现在真正在用**的 apim 密钥 id（对不上 → 空数组）。
    pub fn active_key_ids(
        self,
        keys: &[KeyEntry],
        recipes: &HashMap<String, Recipe>,
        home: Option<&Path>,
    ) -> Vec<String> {
        match self {
            Agent::Codex => codex::active_key_ids(home, keys, recipes),
        }
    }

    /// 读全部客户端的现场 → `{客户端: 正在用的密钥 id}`。TUI 启动/刷新时算一次。
    pub fn detect_active_keys(
        keys: &[KeyEntry],
        recipes: &HashMap<String, Recipe>,
        homes: &HashMap<Agent, PathBuf>,
    ) -> HashMap<Agent, Vec<String>> {
        Agent::ALL
            .iter()
            .map(|agent| {
                let ids =
                    agent.active_key_ids(keys, recipes, homes.get(agent).map(PathBuf::as_path));
                (*agent, ids)
            })
            .collect()
    }

    /// 真正写盘：把请求落到客户端配置（各家自己实现格式与校验）。
    pub fn import(self, request: &ImportRequest) -> Result<ImportReport, String> {
        match self {
            Agent::Codex => codex::import(request),
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
}
