# 02: SKILL 的「一键导入」整节压成指针（`TODO-19`）

Status: done
Blocked by: 无（可立即开工）—— 但**票内顺序不能反**：先并两条事实进 `codex.md`，再删 SKILL 那一节

**What to build:** `.agents/skills/apim/SKILL.md` 只讲额度脚本（它在 Doc map 里的唯一角色）。它现在那节「一键导入到 Codex / Pi」讲的是客户端契约，而那一类信息的家是 `docs/clients/{codex,pi}.md` —— 把整节压成一行指针，并加守卫钉住。

**① 重复位置**：`SKILL.md` 第 **174–193 行**（小节标题 `## 一键导入到 Codex / Pi（TUI \`x\` 键，暂无 CLI）`，约 21 行）。

- 约 95% 的内容已经在两份客户端文档里：步骤与 `★` 语义、Pi 只写一处 / `settings.json` 不动 / `apim-` 前缀 / 模型条目不编数字 / 不重启 / 只支持 API key、Codex 的「模型不在 `config.toml` 里」/ 四档思考等级 / 重启 daemon、模型列表与 `m` 键一致 / 不做端点能力筛选、迷你条目 / 保留名。
- **两处只有它才有，必须先并进 `docs/clients/codex.md`**（否则删节即丢信息）：
  1. 查 daemon 启动时间的诊断命令 `ps -o pid,lstart,command -p $(pgrep -f "app-server" | tr '\n' ',')` → 加到「怎么重载」；
  2. 「想要多套并存就用官方的 `codex --profile <name>` + `~/.codex/<name>.config.toml`」→ 加到「写到哪」。

**② 新家**：`docs/clients/codex.md`（上面两条 + 已有内容核对）、`docs/clients/pi.md`（已有内容核对）、`README.md` 的 *One-click import into Codex / Pi*（用户向步骤，**不动**）。

**③ 原地留什么指针**：一行，且必须保住两条信息 —— **入口是 TUI 的 `x` 键（暂无 CLI；`apim import` 见 `docs/TODO.md` 的 `TODO-9`）**；契约与真机坑见 `docs/clients/codex.md` / `docs/clients/pi.md`（用户向步骤见 README）。例：

> 一键导入到 Codex / Pi 的入口是密钥栏按 `x`（TUI only，暂无 CLI）。
> 契约（写哪几个键、怎么校验、怎么重载、`★` 怎么认）见 `docs/clients/codex.md` / `pi.md`；用户向步骤见 README 的 *One-click import into Codex / Pi*。

**④ 守卫**：**新增第 10 条** `the_skill_only_keeps_what_no_one_else_documents`：
- 断言 (a) SKILL 含指向 `docs/clients/` 的指针；(b) SKILL **行数 ratchet**（上限 = 实施后的实测值，注释写「只允许变小或保持」）；
- 失败信息：`problem(".agents/skills/apim/SKILL.md", RULE, "<缺指针 / 超了 N 行>", "把客户端那一节压成指针；契约细节在 `docs/clients/*.md`")`。
- 与现有第 6 条（`the_skill_does_not_redefine_the_cli_surface`）的分工：那条管**命令面**，这条管**体量与指针**。

- [x] 先把两条只有 SKILL 有的事实并进 `docs/clients/codex.md`（「怎么重载」+「写到哪」）—— grep 确认已在
- [x] 核对那节约 21 行的其余内容在 `docs/clients/{codex,pi}.md` 里逐条都在（README 已覆盖用户向步骤与按键）
- [x] 整节替换为一行指针（`x` 是唯一入口这条没丢）；2716 字符 → 275 字符，SKILL 226 → 183 行
- [x] 新增第 10 条守卫（指针 + 行数 ratchet 183），失败信息三件套齐全
- [x] 反向变更各自单独验：①删掉指针 → FAILED（报「没有指向客户端契约的家」）；②加 7 行 → FAILED（报「行数 190 超过上限 183」）；验完恢复 10 passed
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
- [x] SKILL 的额度脚本部分一个字不动（env 注入表 / 输出契约 / 已知坑 / mock 三步 / 自查清单）

## Done

- commit: `75d3407`（`75d34078f199`，`SKILL.md` + `docs/clients/codex.md` + `tests/docs.rs`；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `10 passed`
  - **「先搬后删」的证据**：`grep` 到 `docs/clients/codex.md` 第 24 行 `codex --profile <name>`、第 135 行 `pgrep -f "app-server"` —— 两条都在文档里之后才删 SKILL 那一节
  - SKILL：226 → **183 行**（导入节 2716 字符 → 指针 275 字符）
  - **反向验证**（各自单独跑，实测报错如下，验完恢复）：
    - A：删掉指针句 → `the_skill_only_keeps_what_no_one_else_documents` **FAILED**，原文：`.agents/skills/apim/SKILL.md：违反「SKILL 只讲额度脚本（命令面与客户端契约都在别处）」 / 没有指向客户端契约的家（docs/clients/）的指针 / 怎么修：补一句：契约与真机坑见 docs/clients/codex.md / pi.md`
    - B：末尾加 7 行 → **FAILED**，原文：`行数 190 超过上限 183（多了 7 行） / 怎么修：把重复的内容压成指针（客户端契约 → docs/clients/*.md；命令面 → README）；**不要**直接调大上限`
- **偏离 spec：无（但流程上记一笔）。** 指针初稿里我写了 `apim import <agent>`（把 `TODO-9` 的**未来**命令当成现有命令引用），**现有守卫 6（`the_skill_does_not_redefine_the_cli_surface`）当场红给我看** —— R2a 要求 SKILL 调用行上的动词在 README 出现过，而 `import` 还没做。改成「CLI 入口还没做，见 `TODO-9`」后过。这一笔说明第 6 条确实在干活（也说明「未来命令不要写成示例」）。
- [ ] 核对那节约 21 行的其余内容在 `docs/clients/{codex,pi}.md` 里逐条都在（缺的补进文档）
- [ ] 整节替换为上面那行指针（`x` 是唯一入口这条不许丢）
- [ ] 新增第 10 条守卫（指针 + 行数 ratchet），失败信息三件套齐全
- [ ] 反向变更各自单独验：①删掉指针那句 → 应红；②给 SKILL 添 5 行 → 应红；验完恢复
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
- [ ] SKILL 的额度脚本部分一个字不动（env 注入表 / 输出契约 / 已知坑 / mock 三步 / 自查清单）

## Done

（实施后回填：commit sha / 三条验证命令与结果 / 反变换记录 / 偏离）
