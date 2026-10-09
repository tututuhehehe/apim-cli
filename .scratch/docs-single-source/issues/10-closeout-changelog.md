# 10: 收尾 —— CHANGELOG 记账 + 遗留项归位

Status: ready-for-agent
Blocked by: 01, 02, 03, 04, 05, 06, 07, 08, 09

**What to build:** 这次文档重构按 Doc map 规定的生命周期真的跑通一次：结论进 `CHANGELOG.md`，明确不做的相邻发现回 `docs/TODO.md`，工作过程留在 `.scratch/`。不留「未完成但又对不上任何地方」的东西。

- [ ] `CHANGELOG.md` 记一笔（按本仓既有格式：新增 / 变更 / 内部）：Doc map 落地、AGENTS.md 瘦身与行数上限、`docs/clients/` 与 `docs/provider-kinds.md` 成家、SKILL 命令表下线、5 条文档守卫
- [ ] spec 里被排除的相邻发现不留脑子里：`docs/TODO.md` TODO-3 指向 `target/apim-review/` 的悬空引用**单开一条 `TODO-N`**（或明确记下不做及理由）
- [ ] 对照 spec 决定 4 逐条核对：5 条守卫全绿、AGENTS.md 在行数上限内、SKILL 不含命令表、README 命令表与 `apim help` 一致
- [ ] `.scratch/docs-single-source/` 保留（工作过程留在原地，按 Doc map）
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
