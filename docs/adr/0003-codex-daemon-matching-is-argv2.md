# ADR-0003：Codex daemon 匹配严格用 argv[2]

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §2.3

## Context

导入后需要重启 Codex 才能生效，重启要杀在跑的 daemon。评审提过一个风险：「父 shell 误杀」。

## Decision

`is_codex_server` 只认「**可执行文件名正好是 codex** + **子命令位 argv[2] 正好是 `app-server`**」。不做针对「父 shell 误杀」的额外加固。

## Consequences

- 父 shell 的 argv[1] 是 `-c` / `-zsh`，文件名也不是 codex，严格匹配本来就不会命中 —— 加固没有真实收益。
- 要更严就得引 `getppid()` 的 libc 依赖，不值当。
- 匹配规则已有单测守住（`src/clients/codex/tests/restart.rs`）；`docs/TODO.md` 的 TODO-3 记的两条相关文案缺口仍待修。
