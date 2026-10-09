# 07: AGENTS.md 瘦身 + 行数 ratchet

Status: done
Blocked by: 03, 04, 05

**What to build:** AGENTS.md 从「每次运行都得先读 226 行的参考书」变回「导航指针 + 活着的决策」：三条已迁走的约定不再复述，背景与依赖清单这类不改变行为的行删掉，目录树不再与约定互抄；并定下一个行数上限由守卫钉住，从此只允许变小或保持。

- [x] 「项目背景」压成一行（并指向 `README.md` 的面向用户部分）
- [x] 「技术选型」删依赖清单，只留活着的决定：Rust 单二进制、数据化范围只到厂商协议
- [x] 删掉第三遍重复的 `cargo install --path .` 提示（约定 10 已说过一次）
- [x] 「目录结构」与「运行时数据」：与 `docs/clients/*.md` / `docs/provider-kinds.md` 重复的行为细节已收成名词或指针
- [x] 约定 13 里那句改成指针措辞（**`B06` 的决议**：契约的家是 `docs/clients/<id>.md`，模块文档只留一行指针）
- [x] 客户端子模块的模块头压成指针：`clients/codex/{mod,official}.rs`、`clients/pi/mod.rs`（保留「负责什么 + 契约在哪 + 子模块分工」）
- [x] Doc map 补一行 skill（`.agents/skills/*/SKILL.md`）；另补一行 `docs/agents/*.md`（Domain docs 小节收进去的）
- [x] 定下 AGENTS.md 的上限并写进守卫：**181 行 / 12508 字符**，注释写明「只允许变小或保持」（另加了一个字符上限，见下方偏离）
- [x] 目录树仍与 `src/` 一致（票 01 的 R3 保持绿：4 个守卫全绿）
- [x] 「运行时数据」与 README「Where the data lives」的重叠按审计结果处置：纯用户向的解释不再重复，文件路径与权限留着
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `fc4ad00`（`fc4ad00d547f`；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `4 passed`（含新 ratchet）
  - `cargo +1.88 check --locked --all-targets` → 编过
  - **ratchet 会红**（临时把行数上限调成 180，验完恢复）：`agents_md_stays_within_its_budget` FAILED，报「行数 181 超过上限 180（多了 1 行）」+ 怎么修。字符上限同样验过（调成 12611 时会报「字符数 12612 超过上限 12611」）
  - **八条红线复核**（你点名的都在）：`sk-...` 占位、含密钥文件 600 且建文件时即 600、内置 `openai` 不可复制、数据化范围只到厂商协议、改完必跑、git 流程、提交体例、台账规矩 —— 各 1 处
- **体量**：**181 行 / 12508 字符**（本轮起点 196 / 14246；本仓峰值 226 行 / ~21.5k 字符）。删掉的行去了哪儿：依赖清单交回 `Cargo.toml`、浏览器登录红线搬进 `docs/clients/codex.md`、快照矩阵交给「验证命令速查」、约定 9 的 `GITHUB_TOKEN` 教训交给 `docs/RELEASING.md`、树里与新文档重复的注解交给 `docs/clients/*.md`、「Domain docs」小节交给 Doc map。
- **偏离 spec：四处，均为加严或「不给橡皮筋」的取舍。**
  1. **ratchet 除了行数还钉了字符数**（票面只写「行数上限」）。理由：AGENTS.md 大部分是长单行段落，把一段拆十行反而更容易过线，而**常驻上下文花的是字符/token**，所以两个一起钉（`AGENTS_LINES_MAX = 181` / `AGENTS_CHARS_MAX = 12508`）。这层你可以拍掉，只留行数。
  2. **「Domain docs」小节删掉、换成了 Doc map 一行**：那一节的内容本来就该由 `docs/agents/domain.md` 自己拥有；顺带把 `docs/agents/*` 纳入 Doc map（之前它没有任何行，票 09 的孤儿文档守卫会报到）。
  3. **补回 `--snapshot-form`**：我在把约定 5 的快照矩阵改成指针时把它弄丢了（它原来只在约定 5 里被列过），发现在后补进「验证命令速查」这个唯一家；并顺手修了一处**既有陈旧**：厂商详情快照是 `APIM_SNAPSHOT_INSPECTOR=provider`（环境变量），不是 `--snapshot-inspector=provider`（那个形式会被 main.rs 当成未知参数）。
  4. **模块头只压了你点名的那三个**（`clients/codex/{mod,official}.rs`、`clients/pi/mod.rs`）。剩下的客户端子模块头注释仍与 `docs/clients/*.md` 重复（`B06` 的剩余部分，括号里是它当前的 `//!` 行数）：`clients/mod.rs`(16)、`clients/codex/catalog.rs`(18)、`clients/codex/import.rs`(13)、`clients/codex/restart.rs`(7)、`clients/codex/active.rs`(6)、`clients/pi/active.rs`(17)、`clients/pi/import.rs`(10)、`clients/pi/config.rs`(9)、`clients/pi/verify.rs`(4)。它们在同一个模块树里、都不是常驻上下文，所以没在本轮做；要不要单开一票由你定。
- **没做、但记在这儿的**：约定 12（`apim update` 只认三条渠道）现在是全文最大的一行（~1000 字符）。它的家更该是 `docs/RELEASING.md`（渠道表与发布教训都在那里），但那条不在本票清单里、当时也没有现成的容纳处，所以留着 —— 想做的话单开一票更干净。
