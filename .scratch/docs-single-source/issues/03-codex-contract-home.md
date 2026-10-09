# 03: Codex 契约成家（`docs/clients/codex.md`）+ 约定 11 一行化

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 谁要改「一键导入到 Codex」这条路，能**整段读完**它需要知道的一切，而不必在 AGENTS.md 里和别的话题交错。AGENTS.md 约定 11 从 18 行缩成一行硬约束 + 指针，细节落到新文档里。

- [ ] 新建 `docs/clients/codex.md`，按固定骨架组织：写到哪 → 写什么形状 → 怎么校验 → 怎么重载 → 怎么认出正在用的密钥 → 真机踩过的坑 → 何时该重开
- [ ] 内容来源是 AGENTS.md 约定 11 与 `docs/TODO.md` TODO-3 里与 Codex 相关的条目；迁移时**不得丢失任何一条实测约束**（含「别凭感觉改回去」的那些：argv[2] 匹配、`toml_edit` 保注释、建文件即 600、迷你条目字段、`codex login status` 两个流 + 退出码）
- [ ] AGENTS.md 约定 11 只剩一行硬约束 + 指向本文档的指针
- [ ] **不产生第三份逐字副本**：README 里已有的**用户向**摘要保留，本文档只放契约与开发视角；重叠处用指针而不是复述
- [ ] Doc map 的「客户端契约细节（Codex / Pi 的 TOML、JSON 形状）」一行确认覆盖本文档（该行已存在，必要时只改措辞）
- [ ] 文档里提到的「有意取舍」指向 `docs/adr/0006`，不复制其内容
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
