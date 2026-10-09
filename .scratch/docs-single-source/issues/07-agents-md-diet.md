# 07: AGENTS.md 瘦身 + 行数 ratchet

Status: ready-for-agent
Blocked by: 03, 04, 05

**What to build:** AGENTS.md 从「每次运行都得先读 226 行的参考书」变回「导航指针 + 活着的决策」：三条已迁走的约定不再复述，背景与依赖清单这类不改变行为的行删掉，目录树不再与约定互抄；并定下一个行数上限由守卫钉住，从此只允许变小或保持。

- [ ] 「项目背景」压成一行（README 首段与 `docs/agents/domain.md` 已覆盖其余）
- [ ] 「技术选型」删依赖清单（`Cargo.toml` 是环境事实），只留活着的决定：Rust、数据化范围只到厂商协议、不扩 DSL
- [ ] 删掉第三遍重复的 `cargo install --path .` 提示（约定 10 已说过一次）
- [ ] 「目录结构」与「运行时数据」：凡 `docs/clients/*.md` / `docs/provider-kinds.md` 已详述的行为，在树里只留名词。**批次 1 验收后仍重叠的具体位置**（当时按「不要提前动 07」没动）：树里第 34 行 `pi/import.rs`（`apim-` 前缀）、第 36 行 `pi/verify.rs`（`pi --list-models`）、第 28 行 `restart.rs`（`argv[2]`）、第 29 行 `config_file.rs`（`toml_edit`）、以及运行时数据第 109/110 行（两份客户端文件清单，含「`settings.json`/`auth.json` 都不写」）
- [ ] **落地 `B06` 的决议**（批次 1 验收时抓到的漏项）：约定 13 里「要写就写在客户端子模块的文档里」改成**指针措辞** —— 契约的家是 `docs/clients/*.md`（能整段读完、能被 diff 命中），Rust 子模块的**模块文档只放一行指针**；同一轮把客户端子模块里与两份新文档重复的**模块头注释**压成一行指针（至少 `clients/codex/{mod,official}.rs` 与 `clients/pi/mod.rs` 头顶那几段；具体范围 07 定），并在 Doc map / 约定 13 里口径一致
- [ ] Doc map 补一行「skill：`.agents/skills/*/SKILL.md`」（今天 SKILL 没有任何 Doc map 行）
- [ ] 定下 AGENTS.md 的行数上限并写进守卫（ratchet：只允许变小或保持）；上限值按**实际收敛结果**定，不为了凑数删活的信息
- [ ] 目录树仍与 `src/` 一致（01 的 R3 保持绿）
- [ ] 「运行时数据」与 README「Where the data lives」的重叠按审计结果处置：纯用户向的留给 README
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
