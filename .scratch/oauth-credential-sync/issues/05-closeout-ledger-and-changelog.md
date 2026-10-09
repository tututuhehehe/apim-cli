# 05: 收尾 —— 台账与 CHANGELOG 归位

Status: ready-for-agent
Blocked by: 01, 02, 04

**What to build:** 两件都做完之后，按 Doc map 的生命周期收口：台账里的条目删掉、结论进 `CHANGELOG.md`、`docs/TODO.md` 不再留已完成的条目。

- [ ] `docs/TODO.md` 删掉 `TODO-2`（官方路不回写同步）与 `TODO-12`（last-writer-wins 竞争）
- [ ] 若 01 已纳入：同时删掉 `TODO-13`（`refresh` 档位门与落盘顺序无测试）；若未纳入就别删，并在此记录原因
- [ ] `CHANGELOG.md` 记一笔（按既有格式「修复 / 内部」）：登录与在途探针的写回竞争、Codex → apim 的 token 回写（含 ownership 与 fail-closed 口径）
- [ ] 核对该做的事都做了：`README` 双语不再声称「不回写」；`ADR-0008` / `ADR-0006` 的口径没有被改动；`~/.codex/auth.json` 依旧只读
- [ ] `.scratch/oauth-credential-sync/` 保留（工作过程留在原地，按 Doc map）
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
