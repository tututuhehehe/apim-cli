# 01: 客户端子模块的模块头注释压成指针（`TODO-18`）

Status: done
Blocked by: 无（可立即开工）

**What to build:** 打开 `src/clients/` 下的任何一个适配模块，模块头只回答两件事 —— **这个模块负责什么、契约在哪**（`mod.rs` 再加一条子模块分工）—— 细节去 `docs/clients/{codex,pi}.md` 读。删掉那 9 份手抄的契约说明，并加一条守卫钉住（防它重新长回来）。

**① 重复位置（`//!` 行号，逐条核对用）**

| 文件 | 重复的内容 | 新家（确认已在） |
|---|---|---|
| `src/clients/mod.rs` 1–16 | 加客户端的三处改动、面板不用改、不做 YAML 配方 | `AGENTS.md` 约定 13（已是家）+ `docs/clients/*.md` |
| `src/clients/codex/catalog.rs` 1–18 | 目录存在的唯一理由、别克隆 GPT 模板（`code_mode_only` / `872000` / 62KB harness）、`base_instructions` 不能缺/不能空、`shell_type` + `apply_patch_tool_type` | `docs/clients/codex.md`「`apim-models.json`（模型目录）」 |
| `src/clients/codex/import.rs` 1–13 | 编排顺序、写进 `config.toml` 的四个键（含 `wire_api = "responses"`、`experimental_bearer_token`）、「先纯内存改 + 校验不过还原两处」 | `docs/clients/codex.md`「写到哪 / 写什么形状 / 怎么写」 |
| `src/clients/codex/restart.rs` 1–7 | daemon 只在启动时读一次目录、实测现象、cc-switch v3.16.1、`APIM_NO_RESTART_CODEX` | `docs/clients/codex.md`「怎么重载」 |
| `src/clients/codex/active.rs` 1–6 | ★ 只认现场：`model_provider` → `[model_providers.<key>]` 的 `base_url` / `experimental_bearer_token` | `docs/clients/codex.md`「怎么认出正在用的密钥」 |
| `src/clients/pi/active.rs` 1–17 | pi 没有唯一激活 provider、两张表、`auth.json` 只读 + `proper-lockfile` + 逐条校验、只比 token、`$NAME` / `!command` 退化成比 `baseUrl`、`type: oauth` 不算、前缀只约束写 | `docs/clients/pi.md`「怎么认出正在用的密钥」 |
| `src/clients/pi/import.rs` 1–10 | 只写一处（`providers.<apim-厂商id>` 五个字段、`baseUrl` 补 `/v1`）、不动 `settings.json`、「先内存后写盘 + 校验不过还原」 | `docs/clients/pi.md`「写到哪 / 写什么形状 / 怎么写」 |
| `src/clients/pi/config.rs` 1–9 | `auth.json` 只读的理由、只按认识的键改、备份成 `<原名>.apim.bak`、走 `file_io`（跟随符号链接 + 原子 + 建文件即 600） | `docs/clients/pi.md`「怎么写 / 只动我们认识的键」 |
| `src/clients/pi/verify.rs` 1–4 | 让 pi 自己列一遍模型、没装就直接失败 | `docs/clients/pi.md`「怎么校验」 |

**② 新家**：上表第三列（`docs/clients/*.md`；`mod.rs` 那条的家是 `AGENTS.md` 约定 13）。**实施时逐条核对**，发现文档里真缺的那条就补进文档 —— 不是把模块头留着。

**③ 原地留什么指针**：每个模块一行或两行，形状固定：「<这个模块负责什么>；细节见 `<家>` —— 改这个文件之前先读它。」`mod.rs` 例外：指向 `AGENTS.md` 约定 13（加客户端的三处改动）+ `docs/clients/*.md`，子模块分工可以留成一行（那是导航，不是契约）。

**④ 守卫**：**新增第 9 条** `the_client_modules_point_at_their_contract`：
- 扫描 `src/clients/mod.rs` 与 `src/clients/*/*.rs`（一层子模块）的 `//!` 块；
- 断言 (a) `//!` 行数 ≤ **10**、(b) 含 `docs/clients/`（`mod.rs` 也接受 `AGENTS.md` 约定 13）；
- 失败信息：`problem(file, RULE, "<实测行数 / 缺哪句话>", "把细节搬进 `docs/clients/<id>.md`，原地只留一行指针")`。

- [x] 9 个模块头按上表压成指针（细节逐条核对已在新家，缺的补进文档）—— 另补了 `codex/config_file.rs`（守卫会扫它，但 `TODO-18` 清单没列）
- [x] 新增第 9 条守卫（10 行上限 + 指针要求），失败信息三件套齐全
- [x] 反向变更各自单独验：①给 `catalog.rs` 塞回 13 行细节 → FAILED（报「模块头 13 行，超过上限 10 行」）；②删掉 `pi/verify.rs` 的指针 → FAILED（报「模块头里没有指向契约的家」）；验完恢复 9 passed
- [x] `docs/clients/*.md` 若有补写，只补**缺的那几条**（本轮无需补：搬走的实测事实逐条 grep 全部命中）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿

## Done

- commit: `03cb17d`（`03cb17da6c53`，9 个 `src/clients/**` 模块头 + `tests/docs.rs` 新增守卫；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `9 passed`
  - 模块头行数（上限 10）：`clients/mod.rs` 7、`codex/{mod 8, catalog 4, import 5, official 6, restart 4, active 4, config_file 4}`、`pi/{mod 7, import 5, config 4, active 5, verify 3}`
  - **先核对后删**（你要求的「不许丢信息」机器核对）：搬走的实测事实逐条 grep `docs/clients/*.md`，全部命中 —— `code_mode_only` / `872000` / `62KB` / `base_instructions` / `shell_command` / `apply_patch_tool_type` / `Model metadata for` / `v3.16.1` / `experimental_bearer_token` / `wire_api`（codex.md）、`proper-lockfile` / `applyModelsJson` / `PI_CODING_AGENT_DIR` / `baseUrl`（pi.md）
  - **反向验证**（各自单独跑，实测报错如下，验完恢复）：
    - A：给 `codex/catalog.rs` 塞回 13 行细节 → `the_client_modules_point_at_their_contract` **FAILED**，原文：`src/clients/codex/catalog.rs：违反「客户端模块头只留「负责什么 + 契约在哪」」 / 模块头 13 行，超过上限 10 行 / 怎么修：把细节搬进 docs/clients/<id>.md，模块头只留「负责什么 + 契约在哪」`
    - B：删掉 `pi/verify.rs` 的指针 → **FAILED**，原文：`src/clients/pi/verify.rs：违反「…」 / 模块头里没有指向契约的家（docs/clients/；clients/mod.rs 也可以是 AGENTS.md 约定 13） / 怎么修：补一行：细节见 docs/clients/<id>.md —— 改这个文件之前先读它`
- **偏离 spec：一处（额外改动）。** 票面清单列的是 9 个文件，新守卫扫的是 `src/clients/mod.rs` + `src/clients/*/*.rs`（**13 个**）—— 涵盖了上一轮已压过的 `codex/mod.rs` / `official.rs` / `pi/mod.rs`（它们已合规），也扫出了 `codex/config_file.rs`（只有一句职责、没指家）→ 顺势给它补了 4 行指针。不补它本票就会红。
- 注：`clients/lock.rs` 与 `clients/file_io.rs` **不在**守卫范围（它们讲的是公共机制、不是客户端契约）——与票面 `Out of Scope` 一致。
