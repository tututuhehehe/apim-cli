# 04: Pi 契约成家（`docs/clients/pi.md`）+ 约定 14 一行化

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 与 03 同形，对象换成 Pi：谁要改「一键导入到 Pi」，能整段读完它需要知道的一切。AGENTS.md 约定 14 从 14 行缩成一行硬约束 + 指针。

- [ ] 新建 `docs/clients/pi.md`，骨架与 `docs/clients/codex.md` 同序（写到哪 → 写什么形状 → 怎么校验 → 怎么重载 → 怎么认出正在用的密钥 → 真机踩过的坑 → 何时该重开）；03 若已落地就与它对齐，否则按 spec 决定 2 的骨架
- [ ] 内容来源是 AGENTS.md 约定 14 与 `docs/TODO.md` TODO-3 里与 Pi 相关的条目；迁移时**不得丢失任何一条实测约束**（`settings.json` 一个字不动的代价与理由、`apim-` 前缀只约束写不约束认、模型条目不编数字、`pi --list-models` 两列都要对、★ 扫每一份凭据、`auth.json` 只读不写、与 codex 侧的语义差别是有意为之）
- [ ] AGENTS.md 约定 14 只剩一行硬约束 + 指向本文档的指针
- [ ] **不产生第三份逐字副本**：README 里已有的**用户向**摘要保留，重叠处用指针而不是复述
- [ ] 文档里提到的客户端适配取向指向 `docs/adr/0002`，不复制其内容
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
