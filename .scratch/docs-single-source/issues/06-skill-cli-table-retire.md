# 06: SKILL 命令表下线 + README 指针 + 禁令修句

Status: done
Blocked by: 无（可立即开工）

**What to build:** 用 skill 的 agent 不再读到一份**已不存在的 CLI**：`.agents/skills/apim/SKILL.md` 删掉那份手抄的命令表，改为指向 README 的 CLI 章节；它只保留别处没有的知识（额度脚本契约）。同时修掉 SKILL 里那句与 AGENTS.md 约定 10 相反的安装说明。

- [x] 删掉「CLI 命令速查」表；SKILL 里残留的 `apim` 调用只作**流程示例**（注册厂商 + 绑定脚本、`apim status <id> --json` 验证），不定义签名（新的小节点明「本 skill 只讲额度脚本；下面的 `apim …` 都是示例」）
- [x] SKILL 顶部（「apim 是什么」那句附近）加一条指向 `README.md` CLI 章节的指针（并说明 `apim help` 是同一份事实的机器可读版）
- [x] `cargo install --path .` / `target/debug/apim` 那句改为「跑本地代码用 `cargo run -- <args>`（见 AGENTS.md 约定 10）」（措辞微调见下方偏离 ①）
- [x] `docs/quota-script-prompt.md` 里同一句一并改掉（两处现在都不含那个字面串，票 08 的 R4 能直接过）
- [x] recipe 字段表裁到只剩 `balance` / `vars`，其余指向 README 的 *Adding a provider*
- [x] frontmatter 的 `description` 收敛为「为新厂商写 / 绑定 / 调试额度脚本 + `apim status` 查额度」（副作用见偏离 ③）
- [x] 额度脚本独有的知识**一条不丢**（机器核对：`APIM_TOKEN` / `APIM_VAR_<大写名>` / 输出契约 / 超时 / 截断 / 已知坑 / mock / 自查清单 / `sh -n` 均在）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `878135c`（`878135c5748c`，`SKILL.md` + `docs/quota-script-prompt.md`；票面回填在其后一次提交）
- 验证（本机，本票只动 markdown）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `5 passed`
  - 守卫细项：目录树 / ratchet / CLI 命令面三条均绿（票 01/02/07 的成果未被本票扰动）
  - SKILL 行数：**226 → 197**（diff：-40 / +11）
- **偏离 spec：四处（均为「票面没覆盖的边界」，逐条说明）。**
  1. **SKILL 第 8 行没写回那个字面串**：票面 AC 写的是改成「跑本地代码用 `cargo run -- <args>`（见约定 10）」，而**约束是「别用那个命令」，不是「写上它的名字」**（`AGENTS.md` 里也写了一遍「负向指令 = 把被禁的东西拉进上下文」）。改后的句子是「跑本仓库的代码用 `cargo run -- <args>`；不要把开发副本 `cargo install` 进 `~/.cargo/bin`」，好处是**票 08 的 R4（白名单：README + AGENTS 约定 10）不会被自己的禁令绊倒** —— 如果 SKILL 里出现那个字面串，R4 会报一条假红。
  2. **「SKILL 里不在 README 的要点」判定结果与你举的三个例子不完全一致**（你要我判断并说明理由）：`--script none` 的语义、token 只走 stdin —— **README 两版都有**；`APIM_CONFIG_DIR` —— **README.md 有、README.zh-CN.md 没有**（既有的双版不对称，本票未动：改 README 正文是票 02 的领地，它当时被限定为「只补缺」）。真正 README 没有、所以我**留在 SKILL** 的是：`APIM_CONFIG_DIR` 沙盒跑命令的纪律、坏配置不锁死 CLI、以及 README 只写了 `--script none` 的那半句（空串 `--script ""` 同义）。理由：这三条都是「agent 在跑 CLI 时要知道的操作知识」，而 SKILL 正是 agent 跑 CLI 时被触发的文档；把它们塞进 README 的 CLI 一节反而是把「怎么用」与「怎么被 agent 用」混在一起。
  3. **`description` 收敛的副作用（有意，但要说清）**：去掉「或提到 apim 项目本身」之后，skill 不再因「提到 apim」而触发 —— agent 被要求「加个厂商 / 配密钥」时不会自动加载 SKILL，而是应该去看 README。这正是 spec 决定 1 要的「只描述它真正专长的事」；但如果你希望那类任务也自动命中，就在 description 里把「要接一个新厂商」那句留得再宽一点（现在只写了「要给新厂商配额度查询」）。
  4. **SKILL 的「一键导入到 Codex / Pi」整节（~21 行）没删**：它不在本票 AC 里，但确实与 `docs/clients/{codex,pi}.md` 重复（批次 1 刚建的家）。没有一并处理，因为它既不是「命令表」也不是「禁令」，属另一次取舍；要删的话**先注意两处只有 SKILL 有**：①`ps -o pid,lstart,command -p $(pgrep -f "app-server" …)` 那条查 daemon 启动时间的诊断命令；②`codex --profile <name>` 多套并存的提示。要删就把这两条先并进 `docs/clients/codex.md`。
- 另（给票 08 的预检）：SKILL 里 `apim` 调用行上的动词是 `{help, key, provider, status}`、开关是 `{--json, --name, --base-url, --homepage, --health, --script}` —— **全在 README**，所以 R2 应能直接过。注意 R2 必须按 AC 那样**只扫 `apim` 调用行**：SKILL 里还有 curl（`--connect-timeout` / `--max-time`）、cargo（`--all-targets` / `--ignored` / `--nocapture`）、codex（`--profile`）的开关，全文件扫会假红。
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
