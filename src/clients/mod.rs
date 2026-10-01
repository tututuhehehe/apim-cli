//! 客户端（agent）适配：把 apim 里的密钥 + 厂商 + 模型一键写进外部 CLI 的配置，
//! 以及**回读客户端现场**回答「它现在真正在用哪把密钥」（密钥行的 ★ 角标）。
//!
//! 现在支持 Codex 与 Pi。**加一个新客户端（Claude Code…）要动的地方是固定的三处**：
//!
//! 1. 下面给 [`Agent`] 加一个变体（所有 `match` 都会被编译器强制补全）；
//! 2. 加一个 `clients/<id>/` 子模块，实现「写哪里 / 怎么写 / 怎么写完后校验 / 怎么重新加载 /
//!    怎么从自家配置里认出正在用的密钥」；
//! 3. `Agent` 的新方法里加一条分派（通常一行）。
//!
//! 面板（`app/import`、`ui/import`）**不需要改**：它只经 `Agent` 的这些方法调客户端，
//! 文案也全部取自 `Agent`（`label/badge/config_hint/reload_hint/default_model_hint`）。
//!
//! 各家的「导入规则」不一样（配置格式、鉴权变量名、有没有模型清单、生效方式都不同），
//! 所以**不用 YAML 配方把客户端数据化**：那是厂商协议层的事（见 AGENTS.md 约定 2 ——
//! 同一协议的参数差异才走数据），客户端之间不是同一套协议，各写一个 Rust 子模块更直白。

pub mod codex;
pub mod pi;

pub(crate) mod file_io;
pub(crate) mod lock;

pub use codex::RestartReport;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// 一键导入的目标客户端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Agent {
    Codex,
    Pi,
}

/// 一次一键导入的输入（各客户端共用的形状）。
///
/// 字段都是「apim 这一侧的事实」：客户端自己决定怎么落进自家配置。
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// apim 的厂商 id（recipe id）。
    pub provider_id: String,
    /// 厂商显示名（写进客户端配置里的展示名）。
    pub provider_name: String,
    /// recipe 的 base_url。
    pub base_url: String,
    /// 要写进客户端配置的密钥。
    pub api_key: String,
    /// 勾选导入的模型（至少 1 个）。
    pub models: Vec<String>,
    /// 默认模型，必须是 `models` 之一。
    pub default_model: String,
}

/// 导入成功后的结果：面板只读这里的字段拼提示，不认识任何客户端细节。
#[derive(Debug, Clone)]
pub struct ImportReport {
    /// 客户端侧的 provider 表名 / 键（codex：`[model_providers.<key>]`；pi：`providers` 里的键）。
    pub provider_key: String,
    /// 写进客户端「默认模型」的那个模型。
    pub model: String,
    pub models: Vec<String>,
    /// 客户端特有的一句补充（codex：思考强度），面板原样显示；`None` 则不显示。
    pub detail: Option<String>,
    /// 写盘前留下的备份文件（可能不止一个；原来是新文件就没有备份）。
    pub backups: Vec<PathBuf>,
}

/// 客户端配置里**现在真正生效**的 provider（各客户端子模块读自家配置得出）。
///
/// apim **不记**「上次导入了谁」：★ 只认客户端现场 —— 每次检测都回读客户端的配置文件，
/// 再拿里面的 token / 地址和 apim 的密钥对账。所以用户手改了客户端配置（换 token、
/// 把默认 provider 切走、删掉那个块），★ 会跟着变，而不是留在旧密钥上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProvider {
    /// 客户端配置里的 provider 键（codex：`model_provider` 指向的表名；pi：`defaultProvider`）。
    pub provider_key: String,
    /// 该 provider 的 API 根地址（原样，未归一化）。
    pub base_url: String,
    /// 写在该 provider 上的 token；客户端改用环境变量取 token 时为 None。
    pub token: Option<String>,
}

/// 厂商 id → 客户端 base_url 的规范化：codex 与 pi 的 OpenAI 兼容端点都要求带 `/v1`
/// （已经是 `/v1` / `/v1beta` 的原样返回）。
pub fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") || trimmed.ends_with("/v1beta") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

impl Agent {
    /// 面板里按顺序列出的全部客户端。加新客户端只改这里。
    pub const ALL: &'static [Agent] = &[Agent::Codex, Agent::Pi];

    /// 面板显示名。
    pub fn label(self) -> &'static str {
        match self {
            Agent::Codex => "Codex",
            Agent::Pi => "Pi",
        }
    }

    /// ★ 角标上的短标。多个客户端同时用同一把密钥时并排显示（`★C`、`★C,P`）。
    pub fn badge(self) -> &'static str {
        match self {
            Agent::Codex => "C",
            Agent::Pi => "P",
        }
    }

    /// 目标配置文件位置，面板上给用户看的提示。
    ///
    /// 返回 `String` 而不是字面量：有些客户端的位置由环境变量决定（`CODEX_HOME`、
    /// `PI_CODING_AGENT_DIR`），写死 `~/.codex/...` 在那种情况下会骗人。
    pub fn config_hint(self) -> String {
        match self {
            Agent::Codex => codex::config_hint(),
            Agent::Pi => pi::config_hint(),
        }
    }

    /// 导入完成后，用户要在客户端里做什么才能看到新模型（面板提示的尾巴）。
    pub fn reload_hint(self) -> &'static str {
        match self {
            Agent::Codex => "重启 Codex 后才会列出新模型",
            Agent::Pi => "在 Pi 里打开 /model（或重开）即可看到新模型",
        }
    }

    /// 面板第三步的说明：选中的这个模型会被写到哪里。
    pub fn default_model_hint(self) -> String {
        match self {
            Agent::Codex => format!(
                "写进 config.toml 的 model（强度 {}）",
                codex::DEFAULT_EFFORT
            ),
            Agent::Pi => "写进 settings.json 的默认模型".to_string(),
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
            Agent::Pi => pi::active_key_ids(home, keys, recipes),
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
            Agent::Pi => pi::import(request),
        }
    }

    /// 导入后要不要让客户端重新加载配置（有的客户端要重启进程，有的不用）。
    pub fn needs_reload(self) -> bool {
        match self {
            Agent::Codex => true,
            // pi 每次启动读配置，`/model` 打开时也会重读：没有常驻进程要杀
            Agent::Pi => false,
        }
    }

    /// 让客户端重新加载配置（杀 daemon / 重载 / 什么都不做，各家自己实现）。
    pub fn reload(self) -> Result<RestartReport, String> {
        match self {
            Agent::Codex => codex::restart_daemon(),
            Agent::Pi => Ok(RestartReport::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 客户端注册表的自检：面板上的每一行都要有显示名 / 角标 / 两句话，
    /// 而且角标互不重复（多个客户端并排成 `★C,P` 时不能撞）。
    #[test]
    fn every_agent_has_its_own_label_badge_and_hints() {
        let mut badges: Vec<&str> = Vec::new();
        for agent in Agent::ALL {
            assert!(!agent.label().is_empty());
            assert!(!agent.badge().is_empty());
            assert!(!agent.config_hint().is_empty());
            assert!(!agent.reload_hint().is_empty());
            assert!(!agent.default_model_hint().is_empty());
            assert!(
                !badges.contains(&agent.badge()),
                "角标撞车：{}",
                agent.badge()
            );
            badges.push(agent.badge());
        }
        assert_eq!(badges.len(), Agent::ALL.len());
    }
}
