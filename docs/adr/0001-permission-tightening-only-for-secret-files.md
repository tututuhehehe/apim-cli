# ADR-0001：权限收紧只针对含密钥的文件

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §2.1

## Context

`~/.codex/apim-models.json` 里只有模型 slug、上下文窗口和思考等级，**不含密钥**。它由 Codex 读取，而用户可能以别的用户身份、或用 `sudo codex` 运行。

## Decision

`apim-models.json` 一律保持 644。权限收紧（600，且用 `OpenOptions::mode(0o600)` **建文件时即 600**）**只针对含密钥的文件**：`~/.codex/config.toml`、`config.toml.apim.bak`、`~/.config/apim/secrets.toml`。

## Consequences

- 给模型目录上 600 会让 `sudo codex` 或别的用户读不到，反而制造故障。
- 新增任何「带 token 的落盘文件」时，默认应当是 600 建文件，而不是 `fs::write` + `chmod`（中间有 0644 窗口）。
- **何时该重开**：如果模型目录将来开始含密钥（按设计不该发生），这条要推翻。
