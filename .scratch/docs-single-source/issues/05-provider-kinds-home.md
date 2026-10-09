# 05: 厂商类型成家（`docs/provider-kinds.md`）+ Doc map 补行

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 厂商类型（模型 / 非模型）的语义与「不适用面」有自己的家：改分页、表单、快捷键或状态口径的人，看一份文档就知道哪些东西对该类型不成立。AGENTS.md 约定 15 从 10 行缩成一行 + 指针，Doc map 补上这一行。

- [ ] 新建 `docs/provider-kinds.md`：数据源只有一个字段（recipe 的 `kind`，缺省模型）→ 类型的唯一入口是创建 → 不适用面（无模型列表、不能导入客户端、不探活）→ 表单行按类型显隐 → 分页状态归 App → 快捷键四个分支 → 状态口径走额度脚本成败 → `health` / `models_url` 保留但不生效
- [ ] **明确不放进 `docs/clients/`**：它讲的是厂商类型，不是客户端契约，按客户端归类会误导读者（在文档开头一句话说明它的归属）
- [ ] AGENTS.md 约定 15 只剩一行 + 指针
- [ ] Doc map 补一行「厂商类型（模型 / 非模型）的语义与不适用面 → `docs/provider-kinds.md`」
- [ ] 迁移不丢约束（含手改 `kind:` 也不会拿旧 `health` 去发请求、`save_provider_form` 一律取原值这一处规则落点）
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
