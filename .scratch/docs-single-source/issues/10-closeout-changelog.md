# 10: 收尾 —— CHANGELOG 记账 + 遗留项归位

Status: done
Blocked by: 01, 02, 03, 04, 05, 06, 07, 08, 09

**What to build:** 这次文档重构按 Doc map 规定的生命周期真的跑通一次：结论进 `CHANGELOG.md`，明确不做的相邻发现回 `docs/TODO.md`，工作过程留在 `.scratch/`。不留「未完成但又对不上任何地方」的东西。

- [x] `CHANGELOG.md` **不动** —— 这一轮没有用户可见的变化（新增的是守卫、dev 文档与指针，README 只多一行 `apim tui`），按「只记用户可见变化、不硬凑」处理（理由见下方 Done）
- [x] spec 里被排除的相邻发现不留脑子里：新开 `TODO-17` 记 `docs/TODO.md` 里指向 `target/apim-review/` 的悬空引用（`B08` 的落位；编号顺延：原定的 16 被 oauth 那条占了）
- [x] 本轮十张票的遗留项也挪进台账：`TODO-18`（客户端子模块的模块头注释，9 个）、`TODO-19`（SKILL 导入节）、`TODO-20`（约定 12）
- [x] 对照 spec 决定 4 逐条核对：**8 条守卫全绿**、`AGENTS.md` 在 ratchet 内（181 行 / 12503 字符 ≤ 181 / 12508）、SKILL 已无命令表、两份 README 与 `apim help` 一致
- [x] `.scratch/docs-single-source/` 保留（工作过程留在原地，按 Doc map）
- [x] **退役 `.scratch/open-questions.md`**（先确认没有答复只活在那里面；结论见下方 Done）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `0f80df1`（`0f80df1ff4e5`，`docs/adr/0010-*` + `docs/adr/README.md` + `docs/TODO.md` + 删 `open-questions.md`；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `8 passed`
  - 决定 4 逐条（手跑）：①8 条守卫全绿；②`AGENTS.md` **181 行 / 12503 字符**（上限 181 / 12508）；③SKILL 里旧命令表首行出现 **0 次**；④两份 README 与 `apim help` 的命令面差集为空
- **CHANGELOG 没记，理由**：这轮（`docs-single-source`）**没有用户可见的变化** —— 交付的是 8 条守卫、`docs/clients/*.md` / `docs/provider-kinds.md` 两个新家、`AGENTS.md` 瘦身与指针、SKILL 命令表下线；唯一碰 README 的是多了一行 `apim tui`（文档补缺，不是行为变化）。按你的口径「若没有用户可见变化就不硬凑条目」，`CHANGELOG.md` 一个字没动（工作区核实过）。用户可见的两件事（登录不被覆盖、Codex 刷新的 token 被采纳）已经在上游 `.scratch/oauth-credential-sync/` 票 05 记在 `[Unreleased]` 里。
- **`open-questions.md` 退役过程**（你的第 4 条）：逐条核了 28 条答复的落位 —— A 组随 `TODO-1` 关闭而失效/落进 `ADR-0008` + `0009` 与 `.scratch/oauth-credential-sync/`（五票全 done）；B01/B02 进 spec 的数字、B03→`docs/provider-kinds.md`、B04→README 的 `apim tui`、B05→Doc map 的 skill 行、B06→约定 13 + 三份模块头（余下 9 份 → `TODO-18`）、B08→`TODO-17`、B09→运行时数据瘦身；C01–C04→票结构、C05→`.scratch/` 已提交、C06→`docs/agents/issue-tracker.md`（你写的）。**只有 `B07(b)` 还只活在这个文件里**（「新开一条 ADR 记文档事实单一来源」；`ADR-0009` 当时被 OAuth 决定占用了）→ 先落成 **`ADR-0010`**（+ 索引行）再删。
- **偏离 spec：两处。**
  1. **`ADR-0010` 不在本票 AC 里**：它是退役 `open-questions.md` 的**前置条件**（不落位就删 = 丢决策），按你本批次第 4 条「先落位再删」。
  2. **`TODO-17` 而不是 `TODO-16`**：`B08` 的答复写的是「新开 `TODO-16`」，但那个号已被 oauth 那条（「写入闸跳过 → 重读重试一次」没有自动化测试）占掉 —— 你在批次 2 验收时也提过这个撞号。所以 `B08` 顺延到 `TODO-17`，并在票面写清原因（免得下次有人按答复去找 `TODO-16`）。
