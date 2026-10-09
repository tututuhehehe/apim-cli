# 文档单一来源：Doc map 落地 + AGENTS.md 瘦身

Status: ready-for-agent
Feature: docs-single-source
来源: 2026-10-09 retro 的候选 #2（SKILL 命令表与 README 漂移）、#5（AGENTS.md 常驻体量）、#6（目录树与 `src/` 不一致）、#10（no-op 行）、#11（约定 11/14/15 下沉）；外加同一不变量的相邻条目「禁令被第二份文档抄反」（retro #1）。

## Problem Statement

同一个事实在仓库里有多份手抄：维护者改一处、忘另一处，就会留下一份**会说谎**的文档 —— 而读它的人（人、实现 agent、评审 agent）无法从文件本身判断谁新谁旧。

已经发生过的三次：

- **CLI 命令面**：`.agents/skills/apim/SKILL.md` 有一份「CLI 命令速查」表，是 README 命令表的第二份手抄。SKILL 最后一次改动停在 2026-10-02；此后 `apim auth openai login|status|logout|import-codex` 整块没进 SKILL，`provider add --kind`（非模型厂商，2026-10-03 落地）也没有。按 `apim` 后的第一个词算，README 覆盖 10 个动词、SKILL 只有 7 个（缺 `auth`）—— SKILL 描述的是一个已经不存在的 CLI。
- **禁令被抄反**：SKILL 第 8 行教读者用 `cargo install --path .` 安装、开发时用 `target/debug/apim`；而 AGENTS.md 约定 10 明令禁止前者并解释了后果（PATH 里 npm 版在前，cargo 装出的二进制不会被 `apim` 命中，只会让人误判「改了没生效」）。同一句话还出现在 `docs/quota-script-prompt.md`。
- **目录树**：AGENTS.md 的「目录结构」是一份手工维护的 `src/` 索引，没有任何东西保证它跟得上拆文件。

同时 AGENTS.md 作为**每次运行常驻**的文件，被这些内容撑到 226 行 / 22.5k 字符（约 37KB），其中：

- 「目录结构」88 行（占 39%），其中相当一部分只是把「核心约定」11/14/15 已经详述的硬约束缩写成短句（`restart.rs` 的 argv[2]、`config_file.rs` 的 toml_edit、`catalog.rs` 的迷你条目、`active.rs` 的 ★、pi 的 `apim-` 前缀……）。同一件事写在两处，短句读起来还更权威。
- 约定 11 / 14 / 15 合计 42 行 —— 占「核心约定」这一节（66 行）的 64%、全文的 19%。内容是「这些是真机实测的硬约束，别凭感觉改回去」，属于**参考**，不是每次运行都要先付的**步骤**。
- 还有几条不改变行为的行：整段「项目背景」（与 README 首段同义）、「技术选型」里已经写在 `Cargo.toml` 的依赖清单、以及第三遍重复的 `cargo install` 提示。

仓库刚立好规则（`AGENTS.md` 的 Doc map：「每类信息只有一个家，其它位置只放指针」），但**只写下了规则，没有装任何守卫**：CI 有 fmt / clippy / test / MSRV / `node --check` 五道守卫覆盖代码，文档侧零守卫。

## Solution

把 Doc map 从一句话变成**结构**，并给它装一道常设守卫。

具体到人能看到的结果：

- 想查 CLI 用法时只有一个去处（README 的命令表，它必须与 `apim help` 一致），SKILL 只保留它独有、别处没有的知识（额度脚本契约），并在需要时把你送到 README。
- Codex 与 Pi 的契约细节（写哪些 JSON/TOML 键、怎么校验、怎么重载、真机踩过什么）各自集中成一份可整段读完的文档；AGENTS.md 里只剩一行硬约束 + 指针。
- AGENTS.md 缩成「背景一行 + 活着的决策 + 目录索引 + 硬约束一行版 + 验证命令 + Doc map」，并有一个**行数上限**让它只能变小或保持。
- 目录树与 `src/` 的一致性、命令表与 `apim help` 的一致性、禁令的唯一性，全部由 `cargo test` 守；一条不变量一个测试，失败信息点名「哪个文件缺哪一项」。
- 本 spec 一条 `src/` 代码都不改：守卫是一个新的集成测试二进制，搭在既有的三条执行路径上（本地 `cargo test`、CI 的 `cargo test --all`、MSRV job 的 `cargo check --locked --all-targets`）。

## User Stories

1. As 维护者, I want 每类信息只存在于 Doc map 指明的那个家, so that 我改一处就够，不会留下一份会说谎的第二份。
2. As 维护者, I want AGENTS.md 只装「每次运行都必须知道」的东西与指针, so that 常驻上下文变小、注意力不被稀释。
3. As 维护者, I want 目录树与 `src/` 由机器保证一致, so that 拆文件之后我不必记得手工同步索引。
4. As 维护者, I want 同一条禁令在整个仓库只出现一次, so that 不会有一份文档教我做另一份明令禁止的事。
5. As 维护者, I want 客户端契约（Codex / Pi 的 JSON、TOML 形状与真机坑）按客户端分文件, so that 需要时能整段读完，而不是在 AGENTS.md 里和别的话题交错。
6. As 维护者, I want 厂商类型（模型 / 非模型）的语义有自己的家, so that 它不会被硬塞进「客户端」文档。
7. As 维护者, I want AGENTS.md 有一个行数上限（ratchet）, so that 它随着每次新发现只能变小或保持，不会重新膨胀。
8. As 维护者, I want 被删掉的 no-op 行有据可查, so that 下一个人不会好心把它们写回来。
9. As 实现 agent, I want 一条明确的「CLI 用法看 README」指针, so that 我查用法时不必判断哪份文档更新。
10. As 实现 agent, I want `apim help` 是命令面的唯一事实来源, so that 我不读文档也知道有哪些子命令。
11. As 实现 agent, I want SKILL 只保留它独有的知识, so that 我读它时不会被一份过期的命令表误导。
12. As 实现 agent, I want 客户端契约文档按客户端分文件, so that 我改 Codex 时不必把 Pi 的段落一起读完。
13. As 实现 agent, I want 目录树只回答「哪个文件是干什么的」, so that 我不必区分树里哪句话是权威、哪句话是约定里那句话的旧版。
14. As 实现 agent, I want AGENTS.md 里每条约定短到能一次读完, so that 我能记住并按它做。
15. As 实现 agent, I want SKILL 的 description 继续准确描述它真正专长的事, so that 触发时机不漂。
16. As 评审 agent, I want 文档类不变量由机器守, so that 我的注意力花在判断上，而不是「两份表是否一致」上。
17. As 评审 agent, I want 一份明确的「文档家」清单, so that 我能判断新内容该落在哪个文件，而不是看贡献者心情。
18. As 评审 agent, I want 落点错误能被测试点名, so that 我的评审意见有可执行的落点。
19. As README 读者（用户）, I want README 就是唯一的用户手册, so that 我不必知道仓库内部还有 AGENTS.md 与 skill。
20. As README 读者, I want 命令表与 `apim help` 一致, so that 我照 README 敲的命令今天仍然有效。
21. As README 读者, I want 客户端契约细节有独立文档可查, so that README 不必为了完整而变得难读。
22. As 未来接手的人, I want 「故意不做 + 理由」在 ADR、待办在 TODO、进行中在 `.scratch/`, so that 我知道去哪找，不必翻 git 历史。
23. As 未来接手的人, I want 仓库里不再有孤儿文档, so that 我不会把一个过期文件当真。
24. As 未来接手的人, I want 迁移后旧位置留下指针而不是空荡荡, so that 我记得的东西不会变成 404。
25. As 维护者, I want 守卫失败时点名「哪个文件缺哪一项」, so that 修它比不检查更快。
26. As 维护者, I want 守卫只断言结构与集合、不断言译文措辞, so that 润色文案不会让 CI 变红。
27. As 维护者, I want 本次改动不新增 CI 步骤, so that 守卫搭在既有三条执行路径之上，不增加维护面。
28. As 维护者, I want 本 spec 不碰任何 `src/` 代码, so that 它能与 TODO-1 那种真改代码的工作并行而不打架。
29. As 维护者, I want 每条守卫各自独立失败, so that 一条坏了不会掩盖另一条。
30. As 维护者, I want 收尾时结论进 CHANGELOG、遗留项回 TODO, so that Doc map 描述的生命周期真的跑通一次。

## Implementation Decisions

### 决定 0 · 家在哪（沿用 Doc map，只补缺口）

Doc map 已经定下大部分家，本 spec 不重新分配，只补两处缺口：

| 信息 | 家 |
|---|---|
| CLI 命令面（机器可读） | `apim help` 的输出（代码） |
| 面向用户的命令表 / 按键 / recipe / 额度脚本 | `README.md`（中文同源翻译 `README.zh-CN.md`） |
| 额度脚本独有知识（env 注入表、输出契约、坑、mock 流程、自查清单） | `.agents/skills/apim/SKILL.md` |
| Codex 契约细节 | `docs/clients/codex.md`（**新建**） |
| Pi 契约细节 | `docs/clients/pi.md`（**新建**） |
| 厂商类型（模型 / 非模型）语义与「不适用面」 | `docs/provider-kinds.md`（**新建**；Doc map 补一行） |
| 索引 / 角色表 / 目录树 / 背景与决策 / 硬约束一行版 / 验证命令 | `AGENTS.md` |
| 仓库内的文档角色（含 skill） | Doc map 补一行「skill：`.agents/skills/*/SKILL.md`」 |

约定 15 不属于客户端契约，**不进** `docs/clients/`：它讲的是厂商类型对 TUI 分页、表单、状态口径的影响，按客户端归类会误导读者。

### 决定 1 · SKILL 不再拥有命令表

- 删掉「CLI 命令速查」表；SKILL 保留的 `apim` 调用只作为**流程示例**（注册厂商 + 绑定脚本、`apim status <id> --json` 验证），不定义签名。
- SKILL 顶部（跟着「apim 是什么」那句）加一条指向 `README.md` 的 CLI 章节的指针。
- 删掉 SKILL 里「recipe YAML 字段含义」表中与 README「Adding a provider」重复的字段行，只留额度脚本相关的 `balance` / `vars`（其余用指针）。
- SKILL 第 8 行「`cargo install --path .` 安装；开发时用 `target/debug/apim`」改为「跑本地代码用 `cargo run -- <args>`（见 AGENTS.md 约定 10）」。
- SKILL 的 `description`（常驻上下文）改为只描述它真正专长的事：额度查询脚本的编写与绑定。

### 决定 2 · 客户端契约下沉

- `docs/clients/codex.md` 与 `docs/clients/pi.md` 的内容来源是 AGENTS.md 约定 11 / 14 的逐条（含 TODO-3 里与这两个客户端相关的报告级小账），组织成：写到哪 → 写什么形状 → 怎么校验 → 怎么重载 → 怎么认出正在用的密钥 → 真机踩过的坑 → 何时该重开。
- AGENTS.md 的约定 11 / 14 缩成**一条硬约束 + 指针**：一句「Codex（Pi）的契约细节与真机实测坑在 `docs/clients/codex.md`（`pi.md`）；改之前先读它」+ 该条真正需要在写代码时立刻知道的一两句话。
- 约定 15 缩成一条 + 指向 `docs/provider-kinds.md`。
- 三份新文档都要能被 `docs/agents/domain.md` 那套「探索之前先读」覆盖到：Doc map 的「客户端契约细节」行已覆盖 `docs/clients/`，厂商类型需要新增一行。
- **与约定 13 的冲突要当面解决**：约定 13 说客户端说明「要写就写在客户端子模块的文档里」，Doc map 说 `docs/clients/*.md`。本 spec 取 Doc map：契约的家是 `docs/clients/*.md`，Rust 子模块的模块文档只放一行指针。约定 13 那句话同步改成指针措辞。

### 决定 3 · AGENTS.md 的目标形状与行数上限

逐段处置：

| 段 | 处置 |
|---|---|
| 项目背景 | 压成一行（README 首段与 `docs/agents/domain.md` 已覆盖其余） |
| 技术选型 | 删依赖清单（`Cargo.toml` 是环境事实），只留真正活着的决定：Rust、数据化范围只到厂商协议、不扩 DSL |
| 目录结构 | 只留「目录 / 文件 → 一行职责」。凡约定 11/14/15 已详述的行为，在树里只留名词（例：`restart.rs` 只写「按进程表找 codex daemon 并重启」，「严格匹配 argv[2]」只在 `docs/clients/codex.md` 里出现） |
| 运行时数据 | 保留（它是 apim 磁盘布局的唯一家）；实现时审一遍与 README「Where the data lives」的重叠，把纯用户向的重复留给 README |
| 核心约定 1–10、12、13、16 | 保留；13 的措辞按决定 2 改成指针 |
| 核心约定 11 / 14 / 15 | 缩成一行 + 指针（决定 2） |
| Doc map | 保留在 AGENTS.md（它正是「AGENTS.md 只装指针」的体现），补 skill 与厂商类型两行 |
| 验证命令速查 | 保留（这些命令的唯一家） |

- **行数上限作为 ratchet**：AGENTS.md 目标 ≤ 160 行。实现时按实际收敛结果定这个数（不为了凑数删活的信息），定下来后由守卫钉住，只允许变小。

### 决定 4 · 守卫：5 条规则，一个测试二进制

**唯一的新 seam**：一个顶层集成测试二进制，`cargo test` 下自然运行。零新增 CI 步骤 —— 它同时被约定 5 的本地 `cargo test`、CI 的 `cargo test --all`、MSRV job 的 `cargo check --locked --all-targets` 覆盖。每个规则一个独立的测试函数，独立失败。

- **R1 · CLI 面 ⊆ README**：从 `apim help` 的真实输出提取动词集合（`apim` 后的第一个 token，别名并入同一项），断言它 ⊆ README 命令表里出现的动词集合。以 `apim help` 为唯一事实来源。**这条今天就会红一次**：`apim help` 列出别名 `apim tui`，README 里一次都没出现 —— 实现时补一行 README 即可（顺带把 `--version` 一并写全）。
- **R2 · SKILL 不定义面**：SKILL 里出现在 `apim` 调用行上的动词与 `--flag`，必须都出现在 README 的 `apim` 行上（子集关系，只拦「发明」，见下「守卫的边界」）；且 SKILL 必须含指向 README CLI 章节的链接。
- **R3 · 目录树 ↔ `src/` 双向**：树里出现的每个 `src/` 路径都必须存在；`src/` 下每个**非测试** `.rs` 文件都必须被树点名。测试代码按目录归拢（`tests/` 目录或 `tests.rs` 文件）不要求逐个进树 —— 这条契约要写进目录结构那段的标题里，成为明示的口径。
- **R4 · 禁令唯一化**：`cargo install --path .` 只允许出现在 README 的安装节与 AGENTS.md 约定 10。实现时要修掉现存的违例（SKILL 第 8 行、`docs/quota-script-prompt.md`）。
- **R5 · 无孤儿文档**：仓库内每个 tracked 的 `*.md` 都要从 AGENTS.md 的 Doc map 出发**最多一跳**可达（`docs/adr/*` 由 `docs/adr/README.md` 索引即算可达，`.scratch/<slug>/` 按类覆盖）。当前它还会抓到一个真实缺口：SKILL 没有任何 Doc map 行 —— 由决定 0 补上。

**确定性的文件发现（重要）**：守卫必须用**显式根目录列表**（仓库根、`docs/`、`.agents/skills/`、`.scratch/`）而不是全仓递归。本机存在 `.delta/worktrees/`（整仓副本，含同名 `AGENTS.md` / `docs/**`）与 `target/`、`.pi/`，全仓扫描会把它们当成 repo 内容，产生假红。本机私有的 `DEV-NOTES.local.md` 采用「存在则必须在 Doc map 里点名」的包含式断言，这样 CI（无该文件）与开发机行为一致。

**断言的对象是结构与集合**：标题层级、token 集合、路径存在性、行数。从不断言译文措辞、行号或段落顺序 —— 否则每次润色都会变红。

**守卫的边界（明说，避免它被当成万能）**：子集检查只能抓「发明了不存在的命令 / 标志」和「两份说法互相矛盾」，**抓不到遗漏**。SKILL 缺 `apim auth` 这种漂移无法靠集合关系发现 —— 这正是本 spec 的解法是**删掉第二份**而不是「检查第二份」的原因。

### 决定 5 · 不动的东西

- **不碰 `src/`**：命令面通过运行 `apim help` 取得（集成测试天然拿得到被测二进制），不需要为可测性抽函数；本 spec 因此与 TODO-1（OAuth 按厂商存）零交集，可并行。
- **不碰 CI 配置**：新 seam 搭在既有执行路径上。
- **不碰 README 正文**（除 R1 若发现动词缺口才补行）；不改 `docs/TODO.md`、`docs/adr/*`、`CHANGELOG.md` 的内容 —— 它们只被 Doc map 索引。
- 收尾按 Doc map 走：结论进 `CHANGELOG.md`，遗留项回 `docs/TODO.md`。

## Testing Decisions

**什么算好测试**：只断言**外部可观察的结构不变量**（集合、层级、路径存在性、行数上限），从不断言译文措辞、行号、段落顺序。一个守卫读的是「仓库现在的文档长什么样」，不是「检查器内部怎么实现」。

**测哪些**：新增的顶层集成测试二进制，5 个不变量一个测试函数（对应 R1–R5）。**不新增其他 seam** —— 理想值就是一个。

**为什么是这一层**：它是「全仓文档不变量」能取到的最高点，一个文件覆盖五条规则；且免费搭在三条既有执行路径上（本地 `cargo test`、CI `cargo test --all`、MSRV job 的 `--all-targets`）。

**既有先例（照它们写）**：

- 从测试里够到仓库根：`env!("CARGO_MANIFEST_DIR")` 在本仓已有 11 个文件 / 14 处用法（`recipe/mod.rs`、`cli/update/install_sh.rs`、各 `tests/helpers.rs` 等）。
- 跑真实二进制的端到端测试：现有 fake codex / fake pi 脚本测试、`--snapshot` 的「真接口」口径 —— R1 跑 `apim help` 属同一类外部行为测试。
- 「两处必须一致」的注册表自检：`clients/mod.rs` 里守「角标不许撞车」的那条测试，形状与本 spec 的 R1–R4 完全同类。
- 逐字节 / 逐行断言：`pi/tests/import.rs` 对 `settings.json` 的逐字节断言、`ui/import.rs` 的列表一致性断言。
- 纯函数 + 表驱动：TODO-13 记的「抽 `refresh_form(cred)` 再表驱动测它」——本 spec 的 R1 不需要抽函数，直接测二进制输出。

**测试自己要满足的要求**：失败信息点名「哪个文件、缺了哪一项」（例如「AGENTS.md 目录树缺少 `src/cli/uninstall/report.rs`」）。

**明确不测**：`README.md` ↔ `README.zh-CN.md` 的段落级等值（见 Out of Scope）；`docs/` 下散文的质量；ADR 与 TODO 条目的内容正确性。

## Out of Scope

- **README 双语的守卫**（retro #4）：标题层级与 code fence 数的对齐检查。今天两边严格对齐（各 35 个标题、38 个 fence），但本 spec 不把它纳入 R1–R5。若你要，它可以是第 6 条规则 —— 请显式说一声，否则不做。
- **retro #7（两个待办中心）**：已由 `88e7522`（§2→ADR、§3→TODO）与 `9577520` 处置完毕，无需再做。
- **retro #8（`docs/TODO.md` TODO-3 指向 `target/apim-review/`）**：仍是一处悬空引用（`target/` 被 `cargo clean` 清掉即失去可追溯性）。本 spec 不修；若要修，应单开一条 TODO-N，别顺手塞进本次文档重构。
- **TODO-1 / 7 条 grilling 问题**（OAuth 凭据按厂商存）：与本文档无关，用户已搁置；本 spec 不碰、不预设其结论。
- **任何 `src/` 代码改动**：本 spec 一条都不需要。
- **ADR-0007 容忍的三处代码重复**（`ps` 行解析、光标钳位、过滤谓词）：那是**代码**重复，与本文档重复不同源，不动。
- **`AGENTS.md` 里「核心约定 1–10、12、16」的重新排版**：只在需要减行时顺手做，不作为目标。

## Further Notes

- **与 ADR-0007 的关系要当面说清**：ADR-0007 明确容忍三处**代码**重复，并给了「出现第四处同类重复，或其中一处开始漂移」的重开条件。本 spec 处理的是**文档**重复 —— 两者不冲突，但措辞上必须区分，免得有人拿 ADR-0007 来挡文档去重。
- **与 AGENTS.md 约定 13 的冲突是显式的**（不默默覆盖）：约定 13 说客户端说明写在客户端子模块的文档里，Doc map 说 `docs/clients/*.md`。本 spec 选 Doc map，并在决定 2 里把约定 13 的措辞改成指针。
- **相邻发现（本 spec 未纳入，供你决定）**：`docs/quota-script-prompt.md` 里那句 `cargo install --path .` 由 R4 抓，实现时一并改；AGENTS.md「运行时数据」与 README「Where the data lives」有重叠，本 spec 只把它列为实现时的审计项，不预设结论；`docs/TODO.md` TODO-3 的 `target/` 引用见 Out of Scope。
- **关于模板里「不要写具体文件路径」**：本特性的决策对象**就是**文档的归属地，路径即接口（且 `docs/clients/`、`.scratch/` 这些名字已由 Doc map 与 `docs/agents/issue-tracker.md` 定下），所以本 spec 保留文档路径与 Rust 模块名；不含任何代码片段与行号。
- **待确认的一件事（seam）**：本 spec 只引入**一个**新 seam —— 顶层集成测试二进制。请确认它落在你要的位置；若你更希望把守卫写成 CI 里的一步脚本（对齐 `ci.yml` 现有的 `node --check` 那一类），说一声，R1–R5 的内容不变，只换载体。
- **triage 词汇表缺位**：`docs/agents/issue-tracker.md` 自己写明 `triage-labels.md` 尚不存在（本仓未装 `triage` 技能），所以 `Status: ready-for-agent` 是直接按 `to-spec` 技能的指令写进本文件头部的，不是来自那份词汇表。
