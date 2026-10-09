//! Codex 适配：把密钥 / 厂商 / 模型一键写进 Codex（第三方路），以及官方路（ChatGPT 登录）的导入。
//!
//! **契约与真机实测坑（`config.toml` 怎么写、模型目录条目、`auth.json`、`★` 回读、`AUTH` 行、内置
//! `openai` 不可复制）在 `docs/clients/codex.md` —— 改这个模块之前先读它。**
//!
//! 子模块分工：`active` 回读现场算 ★ / `catalog` 模型目录生成与校验 / `config_file`
//! `config.toml` 保注释读写 / `import` 第三方导入编排 / `official` 官方路导入 / `restart` 重启
//! app-server 守护进程。
mod active;
mod catalog;
mod config_file;
mod import;
mod official;
mod restart;

pub(crate) use active::active_key_ids;
pub use catalog::DEFAULT_EFFORT;
pub use import::{import, normalize_base_url, provider_key};
pub use official::import as import_official;
pub use official::{CodexRoute, OfficialReport, codex_route, read_local_login};
pub use restart::{RestartReport, restart_daemon};

use std::path::PathBuf;

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
