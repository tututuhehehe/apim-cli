//! 把 apim 的密钥 / 厂商 / 模型一键写进 Codex 配置（`~/.codex/config.toml`）。
//!
//! Codex 允许在 `config.toml` 里同时定义多个 `[model_providers.*]`，但同一时刻只有
//! 顶层 `model_provider` 指向的那一个是激活的。所以这里只切换「激活项」：已导入的
//! 其它厂商配置块原样保留（用户手写的注释、`[projects]`、`[tui]` 也一律不动）。
//!
//! 两点来自 codex 源码的硬约束（0.159.2 实测）：
//! 1. `wire_api = "chat"` 已被移除，只接受 `"responses"`；
//! 2. `model_catalog_json` 的条目必须有 `base_instructions` 或
//!    `model_messages.instructions_template`，两样都缺会解析报错 —— 见 `catalog`。
//!
//! 子模块分工：
//! - `active`：回读 `config.toml` 现场，回答「现在在用哪把密钥」（★ 角标）
//! - `catalog`：模型目录的生成与校验（照官方字段手写迷你条目）+ 本机 codex 可执行文件定位
//! - `config_file`：`config.toml` 的保注释读写（toml_edit + 备份 + 原子写）
//! - `import`：一次导入的编排（锁 → 改配置 → 写目录 → 校验 → 回滚）
//! - `lock`：导入期间对 codex home 的排他锁
//! - `restart`：导入后重启 codex 的 app-server 守护进程
//!
//! 下面只再导出**模块外真的在用的**那几条 —— `clients/mod.rs` 经 [`crate::clients::Agent`]
//! 调它们；测试（在同一个模块树里）直接经子模块路径取内部项（如
//! `codex::import::import_in`），不再往外搬一层（在二进制 crate 里，没人用的 `pub use`
//! 会被 `unused_imports` 判成警告）。

mod active;
mod catalog;
mod config_file;
mod import;
mod lock;
mod restart;

pub(crate) use active::active_key_ids;
pub use catalog::DEFAULT_EFFORT;
pub use import::{ImportReport, ImportRequest, import, normalize_base_url, provider_key};
pub use restart::{RestartReport, restart_daemon};

use std::path::PathBuf;

/// 每次导入前把现有 `config.toml` 备份到 `<config.toml>.apim.bak`，作为回滚锚点。
/// （`config_file` 拼备份路径时用它。）
const BACKUP_SUFFIX: &str = "apim.bak";

/// 面板上显示的配置文件位置（尊重 `CODEX_HOME`，别写死 `~/.codex/...`）。
pub fn config_hint() -> String {
    let text = codex_home().join("config.toml").display().to_string();
    // 在 HOME 下就缩成 `~/…`：面板里铺一整条绝对路径太吵。
    // 设了 `CODEX_HOME`（不在 HOME 下）时仍如实显示那条路径，绝不写成 `~/.codex`。
    match std::env::var_os("HOME").and_then(|home| home.to_str().map(str::to_string)) {
        Some(home) if !home.is_empty() && text.starts_with(&home) => text.replacen(&home, "~", 1),
        _ => text,
    }
}

/// `~/.codex`（尊重 `CODEX_HOME`）。
pub fn codex_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".codex")
}

#[cfg(test)]
mod tests;
