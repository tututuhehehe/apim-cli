//! pi 适配层的测试：两份 JSON 的保字段改写、写入即 600、端到端导入、校验与回滚、★ 现场识别。
//!
//! 全部在 `target/` 下的临时目录里跑，`pi` 用假脚本（打印一张模型表），不碰真实 `~/.pi`。

mod active;
mod config;
mod helpers;
mod import;
mod real_pi;
mod verify;
