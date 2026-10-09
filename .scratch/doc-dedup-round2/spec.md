# 文档去重第二轮：三处「细节还在，只是没搬完」

Status: ready-for-agent
Feature: doc-dedup-round2
来源：`docs/TODO.md` 的 `TODO-18`（客户端子模块的模块头注释仍与 `docs/clients/*.md` 重复）、`TODO-19`（SKILL 的「一键导入」整节重复）、`TODO-20`（`AGENTS.md` 约定 12 该收成指针）。三件都是上一轮（`.scratch/docs-single-source/`）自己暴露、当时**故意没做**的剩余重复。
口径来源：`docs/adr/0010`（文档事实单一来源：Doc map 是唯一索引、重复的那一份**删掉**而不是再检查一遍、守卫钉住）。

## Problem Statement

`ADR-0010` 立了规矩：每类信息只有一个家，其它位置**只放指针**。上一轮搬完了一批，但还有三处「细节仍在原地」：

1. **客户端子模块的模块头注释**：`src/clients/mod.rs`（16 行）与 `src/clients/{codex,pi}/*.rs` 的 `//!` 里，有 9 个模块把契约细节抄了一遍（`apim-models.json` 为什么存在、别克隆 GPT 模板、daemon 什么时候读模型目录、pi 的 `auth.json` 为什么只读、`★` 怎么对账……）。这些细节的家是 `docs/clients/codex.md` / `pi.md`（上一轮刚建）。现在两边都在讲，谁先漂移没人知道。
2. **SKILL 的「一键导入到 Codex / Pi」整节**（`SKILL.md` 第 174–193 行，约 21 行）：整节讲的是客户端契约，而那一类信息的家是 `docs/clients/*.md`。SKILL 的家（Doc map 明写）是**额度脚本知识**。
3. **`AGENTS.md` 约定 12**（第 127 行，全文最大的一行，约 1000 字符）：讲的是 `apim update` 的渠道识别规则与三条硬约束。它已经在两处提到 `docs/RELEASING.md` 的渠道表，但机制细节（怎么认渠道、钉 tag + 摘要校验、`APIM_INSTALL_DIR` 先 canonicalize、npm 主包与平台子包版本交叉核对、`target/` 与 `~/.cargo/bin` 不更新、加渠道要同步改哪几处）**只在 `AGENTS.md` 里** —— 每次运行都在付这 1000 字符，而它只在动 `src/cli/update/` 时才需要。

三处的共同后果：内容会各自漂移（第二份先过期），而且其中两处还占着常驻上下文 / 最先被读到的文档。

## Solution

按 `ADR-0010` 的老套路，逐处「搬细节 → 原地留一行指针」：

- **模块头注释**（票 01）：每个客户端子模块的 `//!` 只留「这个模块负责什么 + 契约在哪（+ 子模块分工）」，细节交给 `docs/clients/{codex,pi}.md`；`clients/mod.rs` 的「加客户端三处改动」交给 `AGENTS.md` 约定 13（家已经在那儿）。
- **SKILL 的导入节**（票 02）：先把两处**只有那一节有**的实测知识并进 `docs/clients/codex.md`（否则删节即丢信息），再把整节压成一行指针；SKILL 从此只讲额度脚本。
- **约定 12**（票 03）：机制搬进 `docs/RELEASING.md`（渠道表与发布期坑本来就在那儿，两件事必须相邻），`AGENTS.md` 只留「三条渠道 + 指针」。

**验收的共同标准**（每票都要满足）：

- 每一处重复都点名到**行号或小节名**；
- 搬走的内容在**新家逐条都在**（实施时对着列出的条目核对，不是「感觉搬完了」）；
- 原地留的指针**一行**、且措辞能触发读取（点名家与「动之前先读它」）；
- 每票都有守卫钉住：没有能直接用的就加一条，**失败信息写清文件 + 规则 + 缺什么 + 怎么修**。

## User Stories

1. As 维护者, I want 客户端的契约细节只写在 `docs/clients/*.md` 一处, so that 改契约时不必记得同步模块头注释。
2. As 维护者, I want 打开 `src/clients/codex/*.rs` 时一眼看到「契约在哪」, so that 我不必先猜细节散落在哪里。
3. As 维护者, I want 读 SKILL 的 agent 不会被一份重复的客户端契约误导, so that 它拿到的是额度脚本知识（SKILL 真正专长的事）。
4. As 维护者, I want `AGENTS.md` 里不再有 `apim update` 的机制细节, so that 常驻上下文少付约 1000 字符。
5. As 维护者, I want `docs/RELEASING.md` 的渠道表与渠道契约挨着, so that 加渠道时两件事一起改。
6. As 实现 agent, I want 每处搬走后都有守卫, so that 细节不会悄悄长回原地。
7. As 实现 agent, I want 守卫失败时说清「哪个文件、哪条规则、缺什么、怎么修」, so that 修它比不检查更快。
8. As 评审 agent, I want 每票点名到行号/小节 + 列出搬走的条目, so that 我能按 diff 核对「信息没丢」。
9. As 维护者, I want `AGENTS.md` 的 ratchet 在本轮**同步下调**, so that 省下的预算不会被后来的沉积吃掉。
10. As 维护者, I want 这一轮不新增「家」（除 RELEASING 里新加的一节外不新建文档）, so that 收敛的是重复、不是文件数。

## Implementation Decisions

### 决定 0 · 三处重复的位置与新家（逐条）

**票 01 · 模块头注释 → `docs/clients/*.md`**（内容已在文档里，实施时逐条核对）

| 位置（`//!` 行号） | 重复的内容 | 新家（已覆盖，核对用） |
|---|---|---|
| `src/clients/mod.rs` 1–16 | 加客户端的三处改动、面板不用改、不做 YAML 配方 | `AGENTS.md` 约定 13（已是家）+ `docs/clients/*.md` |
| `src/clients/codex/catalog.rs` 1–18 | 目录文件存在的唯一理由、别克隆 GPT 模板（`code_mode_only` / 872000 / 62KB harness）、`base_instructions` 不能缺/不能空、`shell_type` + `apply_patch_tool_type` | `docs/clients/codex.md`「`apim-models.json`（模型目录）」 |
| `src/clients/codex/import.rs` 1–13 | 编排顺序、写进 `config.toml` 的四个键（`wire_api = "responses"`、`experimental_bearer_token`）、「先纯内存改 + 校验不过还原两处」 | `docs/clients/codex.md`「写到哪」「写什么形状」「怎么写」 |
| `src/clients/codex/restart.rs` 1–7 | daemon 只在启动时读一次目录、实测现象、cc-switch v3.16.1、`APIM_NO_RESTART_CODEX` | `docs/clients/codex.md`「怎么重载」 |
| `src/clients/codex/active.rs` 1–6 | ★ 只认现场：`model_provider` → `[model_providers.<key>]` 的 `base_url` / `experimental_bearer_token` | `docs/clients/codex.md`「怎么认出正在用的密钥」 |
| `src/clients/pi/active.rs` 1–17 | pi 没有唯一激活的 provider、两张表、`auth.json` 只读 + `proper-lockfile` + 逐条校验、只比 token、`$NAME` / `!command` 退化成比 `baseUrl`、`type: oauth` 不算、前缀只约束写 | `docs/clients/pi.md`「怎么认出正在用的密钥」 |
| `src/clients/pi/import.rs` 1–10 | 只写一处（`providers.<apim-厂商id>` 的五个字段、`baseUrl` 补 `/v1`）、不动 `settings.json`、「先内存后写盘 + 校验不过还原」 | `docs/clients/pi.md`「写到哪」「写什么形状」「怎么写」 |
| `src/clients/pi/config.rs` 1–9 | `auth.json` 只读的理由、`models.json` 只按认识的键改、备份成 `<原名>.apim.bak`、走 `file_io`（跟随符号链接 + 原子 + 建文件即 600） | `docs/clients/pi.md`「怎么写」「只动我们认识的键」 |
| `src/clients/pi/verify.rs` 1–4 | 让 pi 自己列一遍模型、没装就直接失败 | `docs/clients/pi.md`「怎么校验」 |

**票 02 · SKILL 第 174–193 行（`## 一键导入到 Codex / Pi（TUI \`x\` 键，暂无 CLI）` 整节）→ `docs/clients/{codex,pi}.md`**

- 该节约 95% 的内容已经在两份客户端文档里（步骤、`★` 语义、Pi 只写一处 / `settings.json` 不动 / 前缀 / 模型条目 / 不重启 / 只支持 API key、Codex 的目录机制 / 四档 / 重启 daemon、模型列表与 `m` 键一致 / 不做端点能力筛选、迷你条目 / 保留名）。
- **两处只有它有，必须先并进 `docs/clients/codex.md` 再删节**：
  1. 查 daemon 启动时间的诊断命令 `ps -o pid,lstart,command -p $(pgrep -f "app-server" | tr '\n' ',')` → 「怎么重载」；
  2. 「想要多套并存就用官方的 `codex --profile <name>` + `~/.codex/<name>.config.toml`」→ 「写到哪」。
- 原地指针（一行）要保住两条信息：**入口是 TUI 的 `x`（暂无 CLI，`apim import` 见 `TODO-9`）**；契约见 `docs/clients/codex.md` / `pi.md`（用户向步骤见 README）。

**票 03 · `AGENTS.md` 第 127 行（约定 12）→ `docs/RELEASING.md` 新增一节「更新渠道的契约（`apim update` / `apim uninstall`）」**

- **家定在 `RELEASING.md`**（不新开 `docs/update-channels.md`）：渠道速查表、`install.sh.sha256` 资产、「npm 平台子包可见性要等」这些发布期知识已经在那儿，而「加渠道要同步改什么」本来就必须连表一起改 —— 拆到两个文件只会让人漏一个。代价：Doc map 那一行的描述要从「发版手册 / 额度脚本提示词」扩成「发版手册（含更新渠道的契约）/ 额度脚本提示词」。
- 搬过去的条目（`RELEASING.md` 目前**没有**的，实施时逐条落）：渠道识别规则（npm 看 `node_modules/apim-cli`、brew 看 `Cellar/apim`、其余当 install.sh 装裸二进制）；`target/` 与 `~/.cargo/bin` 一律不更新及其理由；install.sh 渠道不是 `curl | sh`（钉 tag → 临时文件 → 形状校验 → 按 `install.sh.sha256` 校验摘要，拿不到摘要就拒绝执行）；`APIM_INSTALL_DIR` 钉真实位置（先 canonicalize）；npm 渠道更新前核对主包与当前平台子包版本（落后 tag 时报两个版本号并拒绝，`--force` 可越过）；加渠道的清单（`Channel` + 识别规则 + 单测 + RELEASING 的表；`uninstall_program` 的穷尽 `match` 编译器会逼你补，但提示语与单测要手工过）。
- `AGENTS.md` 留一行：三条渠道 + 指针 + 「动 `src/cli/update/` 或 `src/cli/uninstall/` 之前先读它」。

### 决定 1 · 指针的写法（每票一条，措辞要能触发读取）

统一形状：**「<这是谁的模块/哪一节>；<细节的家>；改 X 之前先读它」**。例（票 01 的 `catalog.rs`）：

```rust
//! 生成并校验 `~/.codex/apim-models.json`（模型目录）。
//!
//! 条目形状、为什么存在、别克隆哪些字段、版本敏感项见 `docs/clients/codex.md` 的
//! 「`apim-models.json`（模型目录）」—— **改这个文件之前先读它**。
```

### 决定 2 · 守卫（每票一条，能复用就复用）

| 票 | 守卫 | 失败信息 |
|---|---|---|
| 01 | **新增第 9 条**：`src/clients/mod.rs` 与 `src/clients/*/*.rs` 的 `//!` 块必须 (a) ≤ 10 行、(b) 含指向 `docs/clients/`（`mod.rs` 也接受 `AGENTS.md` 约定 13）的指针 | 文件 + 规则 + 实测行数/缺哪句话 + 「把细节搬进 `docs/clients/<id>.md` 后留一行指针」 |
| 02 | **新增第 10 条**：SKILL (a) 必须含指向 `docs/clients/` 的指针、(b) 行数 ratchet（≤ 实施后的实际值，只允许变小或保持） | 文件 + 规则 + 缺指针/超行数 + 「把那一节压成指针；细节在 `docs/clients/*.md`」 |
| 03 | **复用现有第 4 条（`AGENTS.md` ratchet）+ 本票负责下调上限**（行数 + 字符数都要按实施后的实测值收紧）；**新增第 11 条**：`AGENTS.md` 不得出现 `detect_channel` / `install.sh.sha256` / `APIM_INSTALL_DIR` / `npm view` / `Cellar/apim` / `node_modules/apim-cli`，且 `docs/RELEASING.md` 必须含 `detect_channel` 与 `install.sh.sha256`（证明是**搬**过去了，不只是删了） | 文件 + 规则 + 少了/多了哪个标识符 + 「搬进 RELEASING 的渠道契约一节；AGENTS 只留指针」 |

- 三条新守卫都进 `tests/docs.rs`（同一个集成测试二进制，跑 `cargo test` 即执行、同时搭上 CI 与 MSRV 路径），沿用已有的 `problem(file, rule, detail, fix)` 风格。
- 都可以用**反向变更**验证会红：01 把某个模块头塞回细节 / 删掉指针；02 删掉指针 / 加几行；03 把机制写回 `AGENTS.md` / 删掉 RELEASING 那节。

## Testing Decisions

- **每个守卫一个 `#[test]`**，独立失败；断言只碰**行数、存在性、token 集合**，不断言措辞（改文案不该红）。
- **「信息没丢」怎么测**：主要靠**人对着上面的表格逐条核**（新家是不是每条都在）—— 机器只能证明「指针在」和「AGENTS/SKILL 没超体量」，证明不了语义。所以每票的 `## Done` 要写「核过哪几条、结论」，而不是只写「跑绿了」。
- 反向变更（合成红）：每票至少两次（`01`：塞回细节 / 删指针；`02`：删指针 / 超行数；`03`：写回机制 / 删 RELEASING 那节），验完恢复。
- 实施完成时 `cargo test --all` + 守卫二进制全绿；`AGENTS.md` 体量必须在**下调后**的 ratchet 内。

## Out of Scope

- **`TODO-16`（写入闸重试编排没有测试）与 `TODO-17`（评审结论落在 `target/`）**：不是「重复」，不在这一轮。
- **`clients/mod.rs`、`lock.rs`、`file_io.rs` 的公共件说明**：`lock.rs` / `file_io.rs` 讲的是公共机制而不是客户端契约，不在 `TODO-18` 的清单里（只有 `mod.rs` 在）。
- **SKILL 的额度脚本部分**（env 注入表、输出契约、已知坑、mock 流程、自查清单）：那是 SKILL 唯一的家，一个字都不动。
- **`README.md` / `CHANGELOG.md`**：本轮不碰（没有用户可见变化，按上一轮的口径不记 CHANGELOG）。
- **`docs/adr/*` 里对已删台账编号（`TODO-2` / `TODO-12`）的历史提及**：属留档，维护者可自行决定，不在本轮。
- **新增「家」**：除 `RELEASING.md` 里新加的一节外不新建文档（不做 `docs/update-channels.md`）。

## Further Notes

- 这三票**互不依赖**（票 01 动 `src/clients/*`、票 02 动 SKILL、票 03 动 `AGENTS.md` + `RELEASING.md`），可以并行；只有票 03 会动 `AGENTS.md`（含 ratchet 上限），所以它落盘后要重新跑守卫二进制。
- 上一轮的经验直接用得上：**先写守卫还是先搬**都行，但反向变更必须**各自单独跑**（上一轮踩过「两个反变换一起上会互相掩盖」与「筛选器写错 = 假绿」两个坑）。
- 票 02 的顺序不能反：**先并两条事实进 `codex.md`，再删 SKILL 那一节**（反过来就是先丢信息）。
- 这一轮做完，`TODO-18` / `TODO-19` / `TODO-20` 应当从台账里删掉（收尾动作留给实施方，不额外开票）。
