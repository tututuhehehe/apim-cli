//! codex 适配层的测试：目录生成、config.toml 保注释改写、端到端导入、导入锁、守护进程匹配、
//! 以及「回读现场认出正在用的密钥」（★）。
//!
//! 全部在 `target/` 下的临时目录里跑，且用假 codex 脚本，不碰真实 `~/.codex`。
//! 共享夹具在 [`helpers`]；按被测的源文件分子模块（`official.rs` 的单测也来取 [`helpers`]）。

mod active;
mod catalog;
mod config;
// `pub(super)`：`official.rs` 的测试也要用 `temp_dir` 这类夹具
pub(super) mod helpers;
mod import;
mod real_codex;
// 这个模块的测试全部驱动 unix-only 的东西（`is_codex_server` / `looks_like_wrapped_codex_server`
// 本身就是 `#[cfg(unix)]`，靠 `ps` 认进程）→ 整模块带门，别让它的 import 在 Windows 上悬空。
#[cfg(unix)]
mod restart;
