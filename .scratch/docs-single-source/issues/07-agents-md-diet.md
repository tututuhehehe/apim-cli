# 07: AGENTS.md 瘦身 + 行数 ratchet

Status: ready-for-agent
Blocked by: 03, 04, 05

**What to build:** AGENTS.md 从「每次运行都得先读 226 行的参考书」变回「导航指针 + 活着的决策」：三条已迁走的约定不再复述，背景与依赖清单这类不改变行为的行删掉，目录树不再与约定互抄；并定下一个行数上限由守卫钉住，从此只允许变小或保持。

- [ ] 「项目背景」压成一行（README 首段与 `docs/agents/domain.md` 已覆盖其余）
- [ ] 「技术选型」删依赖清单（`Cargo.toml` 是环境事实），只留活着的决定：Rust、数据化范围只到厂商协议、不扩 DSL
- [ ] 删掉第三遍重复的 `cargo install --path .` 提示（约定 10 已说过一次）
- [ ] 「目录结构」：凡约定 11/14/15 已详述的行为在树里只留名词，不再出现同一约束的短句版
- [ ] 约定 13 里「要写就写在客户端子模块的文档里」改为指向 `docs/clients/*.md` 的指针（解决它与 Doc map 的冲突）
- [ ] Doc map 补一行「skill：`.agents/skills/*/SKILL.md`」（今天 SKILL 没有任何 Doc map 行）
- [ ] 定下 AGENTS.md 的行数上限并写进守卫（ratchet：只允许变小或保持）；上限值按**实际收敛结果**定，不为了凑数删活的信息
- [ ] 目录树仍与 `src/` 一致（01 的 R3 保持绿）
- [ ] 「运行时数据」与 README「Where the data lives」的重叠按审计结果处置：纯用户向的留给 README
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
