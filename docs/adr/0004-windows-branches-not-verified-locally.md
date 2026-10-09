# ADR-0004：Windows 分支不在本机验证

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §2.4

## Context

本机只有 `aarch64-apple-darwin` 一个目标，交叉 `cargo check` 跑不了。

## Decision

Windows 分支（`cleanup::remove_binary` 的 `bail!`、`run_npm_uninstall` 的 EBUSY 提示、相关常量返回）**不在本机验证**，靠 `release.yml` 的 Windows 构建覆盖编译。

## Consequences

- 分支很短，风险可控；但**编译错要到发版才暴露** —— 这正是 `docs/TODO.md` TODO-11（CI 加 Windows `cargo check`）的由来。
- 本机补验的办法：`rustup target add x86_64-pc-windows-msvc` 后 `cargo check --target x86_64-pc-windows-msvc`（`check` 不需要链接）。
