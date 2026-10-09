# 03: Codex 契约成家（`docs/clients/codex.md`）+ 约定 11 一行化

Status: done
Blocked by: 无（可立即开工）

**What to build:** 谁要改「一键导入到 Codex」这条路，能**整段读完**它需要知道的一切，而不必在 AGENTS.md 里和别的话题交错。AGENTS.md 约定 11 从 18 行缩成一行硬约束 + 指针，细节落到新文档里。

- [x] 新建 `docs/clients/codex.md`，按固定骨架组织：写到哪 → 写什么形状 → 怎么校验 → 怎么重载 → 怎么认出正在用的密钥 → 真机踩过的坑 → 何时该重开（并把「怎么写」与「模型列表从哪来」也立成节，因为约定 11 里这两类约束量最大）
- [x] 内容来源是 AGENTS.md 约定 11 与 `docs/TODO.md` TODO-3 里与 Codex 相关的条目；**未丢失任何一条实测约束**（核对过的关键词见下方 Done）
- [x] AGENTS.md 约定 11 只剩一行 + 指向本文档的指针（只留「密钥落盘 600、建文件时即 600」这条红线）
- [x] **不产生第三份逐字副本**：README 的用户向摘要未动，本文档只放契约与开发视角（文档开头一句指向 README 与 `docs/adr/`）
- [x] Doc map 的「客户端契约细节」行已改成实际路径（`docs/clients/*.md`，去掉「尚未建」注记）
- [x] 文档里的「有意取舍」指向 `docs/adr/0006`（以及 0002 / 0008 / 0009），不复制其内容
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `1e2287`（`1e2287062775`，`docs/clients/codex.md` + `AGENTS.md`；票面回填在其后一次提交）
- 验证（本机，本票只动 markdown）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored`（文档票，测试数不变）
  - **搬干净的机器核对**：`leaf_decor_mut` / `experimental_bearer_token` / `supported_endpoint_types` / `ensure_copyable` / `auth_row_index` / `APIM_NO_RESTART_CODEX` / `Logged in using ChatGPT` 在 AGENTS.md 里均为 **0 处**
  - AGENTS.md 行数：**219 → 202**
- **偏离 spec：无实质偏离。** 四处需要记一笔：
  1. **骨架加了两节**（「怎么写」与「模型列表从哪来」）。票面骨架是七节，而约定 11 里体量最大的是「文件 IO 与写盘顺序」与「模型列表来源」两类约束，硬塞进「写什么形状」会不可读；所以拆成独立节，七节全部保留、顺序未变。
  2. **纠正了一处过期指向（不是仅仅搬运）**：约定 11 原写「凭据改成按厂商 id 存是后续的事，见 `docs/TODO.md`」，而 `TODO-1` 已关闭、决议是「**不做**」（`docs/adr/0008`）——照搬会把一个已决的事说成待办，所以改成指向 `docs/adr/0008`（含「何时该重开」）。
  3. **补了一条约定 11 未写、但代码里已有的行为**：激活的 `[model_providers.<key>]` 子表是**整块替换**（同一块里 `env_key`/`auth` 与 `experimental_bearer_token` 不能共存）。来源是 `config_file.rs` 的注释，不是新发现。
  4. **AGENTS.md 里还剩三处细节在「目录结构」树与「运行时数据」里**：第 28 行 `restart.rs …（严格匹配 argv[2]）`、第 29 行 `config_file.rs …（toml_edit 保注释保顺序）`、第 109 行运行时数据里的 `~/.codex/apim-models.json`。它们现在与 `docs/clients/codex.md` 重叠，但属于**票 07（目录结构去重 / 运行时数据审计）**的范围，按你「不要提前动 07」的要求我没动。
