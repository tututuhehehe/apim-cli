# ADR-0002：客户端适配走 Rust 子模块，不做 YAML 配方、不引 trait

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §2.2

## Context

内置厂商走数据化（recipe YAML）。自然会有人问：客户端（Codex / Pi / …）为什么不也数据化？

## Decision

数据化的范围**只到厂商协议**（同一套协议内的参数差异）。客户端之间**不是同一套协议**（JSON vs TOML、env 变量 vs provider 块、要不要重启 daemon 各不相同），各写一个 Rust 子模块（`clients/codex`、`clients/pi`）更直白。**不引 `trait ClientAdapter`。**

## Consequences

- `Agent::ALL` 是编译期常量；`ImportRequest` / `ImportReport` 目前就是 codex 形状 —— `Option` 化与提示文本下沉是后续工作（`docs/TODO.md` 的 TODO-5~TODO-7），不改变本条的取向。
- 引 trait 只会产出 `Option` + `dyn` 噪声，收益为零（第二轮 review 三条独立结论一致）。
- 不再扩 DSL（与 `AGENTS.md` 约定 2 / 15 同旨）。
- **何时该重开**：出现第三个客户端，且三个客户端的请求形状真的开始收敛时，才值得重新评估。
