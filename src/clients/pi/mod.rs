//! 把 apim 的密钥 / 厂商 / 模型一键写进 Pi（`~/.pi/agent/`）。
//!
//! Pi 加一个 API-key 厂商是**纯数据**的事，不需要扩展（Pi 文档 `models.md` 的
//! "Configure a compatible endpoint"）：在 `<agent-dir>/models.json` 里加一个
//! `providers.<id>` 条目（`baseUrl` + `api: "openai-completions"` + `apiKey` + `models`）就够了。
//! 默认 provider / 默认模型由用户自己在 `/model` 里挑，apim 不动 `settings.json`。
//! Pi 只有 API key 这一条路能自动化（订阅是 `/login` 的 OAuth，apim 拿不到，也不该碰）。
//!
//! 三条来自 pi 源码（`dist/core/provider-composer.js` + `model-config.d.ts`，0.x 实测）的硬约束：
//!
//! 1. **provider 键一律加 `apim-` 前缀**：pi 自带一大批同名 provider（`deepseek` / `openai` /
//!    `openrouter` …）。`models.json` 里同名的条目会**覆盖那个内置 provider 的 baseUrl**
//!    （`applyModelsJson` 会把 config.baseUrl 套到它的全部模型上）——不差分毫地把用户的
//!    OpenAI 指向我们的中转站。加前缀就永远不会撞名，`/model` 里也一眼看出是 apim 写的。
//! 2. **只动我们写的字段**：`providers.<键>` 里的 `headers` / `compat` / `modelOverrides` /
//!    `authHeader` 是用户手写的东西，改写时原样保留（models 列表整体替换成这次勾选的）。
//! 3. **模型条目用 pi 的默认值兜底**：只写 `id` / `name` / `input` / `reasoning`，
//!    不写 `contextWindow` / `maxTokens` / `cost` —— pi 对缺省字段用自己的保守默认
//!    （128000 / 16384 / 零价），apim 不替它编数字。要按模型写真值就自己改 models.json。
//!
//! 子模块分工：
//! - `active`：回读 `auth.json` + `models.json` 的凭据，回答「现在在用哪把密钥」（★ 角标）
//! - `config`：`models.json` 的读写（保留未知字段 + 备份 + 原子 600）；`auth.json` 只读
//! - `import`：一次导入的编排（锁 → 改内存 → 写盘 → 校验 → 回滚）
//! - `verify`：跑 `pi --list-models` 让 pi 自己确认勾选的模型都在

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
