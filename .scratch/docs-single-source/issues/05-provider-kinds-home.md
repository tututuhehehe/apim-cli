# 05: 厂商类型成家（`docs/provider-kinds.md`）+ Doc map 补行

Status: done
Blocked by: 无（可立即开工）

**What to build:** 厂商类型（模型 / 非模型）的语义与「不适用面」有自己的家：改分页、表单、快捷键或状态口径的人，看一份文档就知道哪些东西对该类型不成立。AGENTS.md 约定 15 从 10 行缩成一行 + 指针，Doc map 补上这一行。

- [x] 新建 `docs/provider-kinds.md`：数据源只有一个字段（recipe 的 `kind`，缺省模型）→ 类型的唯一入口是创建 → 不适用面（无模型列表、不能导入客户端、不探活）→ 表单行按类型显隐 → 分页状态归 App → 快捷键四个分支 → 状态口径走额度脚本成败 → `health` / `models_url` 保留但不生效
- [x] **明确不放进 `docs/clients/`**（文档开头一句话说明归属，并指向 README 与 `docs/clients/*.md`）
- [x] AGENTS.md 约定 15 只剩一行 + 指针
- [x] Doc map 补一行「厂商类型（模型 / 非模型）的语义与不适用面 → `docs/provider-kinds.md`」
- [x] 迁移不丢约束（手改 `kind:` 不发旧 `health`、`save_provider_form` 取原值这一处规则落点均已搬入）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `c02fd8e`（`c02fd8e138eb`，`docs/provider-kinds.md` + `AGENTS.md`；票面回填在其后一次提交）
- 验证（本机，本票只动 markdown）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored`（文档票，测试数不变）
  - **搬干净的机器核对**：`hides(下标)` / `save_provider_form` / `script_status` / `health_call` / `rebuild_provider_list` / `ProviderKind::ALL` 在 AGENTS.md 里均为 **0 处**
  - AGENTS.md 行数：**226 → 219**
- **偏离 spec：无实质偏离。** 三处需要记一笔的实现选择：
  1. 新文档里对 `AUTH` 行的交叉引用指向 **AGENTS.md 约定 11**，而不是 `docs/clients/codex.md` —— 本批顺序是 05 → 03，票 03 落地前那个文件还不存在。指约定 11 的好处是**每一步提交上引用都成立**：03 落地后约定 11 会变成指向 `docs/clients/codex.md` 的指针，链条仍然通。
  2. Doc map 里「客户端契约细节」那行仍写着「尚未建：AGENTS 约定 11/14/15 的细节该迁过去」，其中提到的 **15 已经过期** （本票已迁完）。按你本轮的分配，那一行由票 03/04 改成实际路径，所以我没在 05 里动它 —— 本批结束时它会正确。
  3. AGENTS.md 目录结构里的 `docs/` 块**未动**：它本来就只列 3 个文件（`docs/adr/` 与 `docs/agents/` 都不在里面），说明它的 `docs/` 部分是短索引而非穷尽清单，穷尽索引本来就是 Doc map。要不要把它改成一句「见下方 Doc map」是票 07（AGENTS.md 瘦身）的事。
