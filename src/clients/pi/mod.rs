//! Pi 适配：把密钥 / 厂商 / 模型一键写进 Pi（`<agent-dir>/models.json`）。
//!
//! **契约与真机实测坑（`models.json` 写什么形状、`apim-` 前缀的理由、`pi --list-models` 校验、
//! `★` 怎么认、`auth.json` 为什么只读不写）在 `docs/clients/pi.md` —— 改这个模块之前先读它。**
//!
//! 子模块分工：`active` 回读现场算 ★ / `config` `models.json` 读写（`auth.json` 只读）/ `import`
//! 导入编排 / `verify` 跑 `pi` 校验。
mod active;
mod config;
mod import;
#[cfg(test)]
mod tests;
mod verify;

pub(crate) use active::active_key_ids;
pub use import::import;

use std::path::PathBuf;

/// `<agent-dir>` 里我们写的那份配置（面板上显示的路径）。
pub fn config_hint() -> String {
    let text = agent_dir().join(config::MODELS_FILE).display().to_string();
    // 在 HOME 下就缩成 `~/…`：面板里铺一整条绝对路径太吵。
    // 设了 `PI_CODING_AGENT_DIR`（不在 HOME 下）时仍如实显示那条路径。
    match std::env::var_os("HOME").and_then(|home| home.to_str().map(str::to_string)) {
        Some(home) if !home.is_empty() && text.starts_with(&home) => text.replacen(&home, "~", 1),
        _ => text,
    }
}

/// pi 的 `<agent-dir>`：默认 `~/.pi/agent`，`PI_CODING_AGENT_DIR` 可改写（pi 文档的官方开关）。
/// 环境变量里写的前导 `~/` 要自己展开：pi 解析时会展开（`config.js` → `utils/paths.js`），
/// 不展开就会写到一个名字叫 `~` 的目录里去。
pub fn agent_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PI_CODING_AGENT_DIR") {
        return PathBuf::from(crate::util::expand_tilde(&dir.to_string_lossy()));
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".pi/agent")
}

/// 这个 apim 厂商在 pi 里用的 provider 键（**一律带前缀**，理由见模块文档）。
pub fn pi_provider_key(provider_id: &str) -> String {
    format!("apim-{provider_id}")
}

/// 定位本机 `pi`：导入后要用它列出模型来校验。
pub fn locate_pi() -> Option<PathBuf> {
    for var in ["APIM_PI_BIN", "PI_BIN"] {
        if let Some(value) = std::env::var_os(var) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in ["pi", "pi.exe"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    ["/opt/homebrew/bin/pi", "/usr/local/bin/pi", "/usr/bin/pi"]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}
