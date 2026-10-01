//! codex 适配层的测试：目录生成、config.toml 保注释改写、端到端导入、导入锁、守护进程匹配、
//! 以及「回读现场认出正在用的密钥」（★）。
//!
//! 全部在 `target/` 下的临时目录里跑，且用假 codex 脚本，不碰真实 `~/.codex`。
//! 共享夹具在 [`helpers`]；按被测的源文件分子模块。

mod active;
mod catalog;
mod config;
mod helpers;
mod import;
mod real_codex;
mod restart;
