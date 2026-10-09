# 04: Pi 契约成家（`docs/clients/pi.md`）+ 约定 14 一行化

Status: done
Blocked by: 无（可立即开工）

**What to build:** 与 03 同形，对象换成 Pi：谁要改「一键导入到 Pi」，能整段读完它需要知道的一切。AGENTS.md 约定 14 从 14 行缩成一行硬约束 + 指针。

- [x] 新建 `docs/clients/pi.md`，骨架与 `docs/clients/codex.md` 同序（写到哪 → 写什么形状 → 怎么校验 → 怎么重载 → 怎么认出正在用的密钥 → 真机踩过的坑 → 何时该重开），并同样多立「怎么写」与「范围」两节
- [x] 内容来源是 AGENTS.md 约定 14；迁移时**未丢失任何一条实测约束**（核对过的关键词见下方 Done）
- [x] AGENTS.md 约定 14 只剩一行 + 指向本文档的指针（只留两条红线：`settings.json` 不动、`auth.json` 只读）
- [x] **不产生第三份逐字副本**：README 的用户向摘要未动（`APIM_PI_BIN` 等用户可见项保留在 README），本文档只放契约与开发视角
- [x] 文档里的客户端适配取向指向 `docs/adr/0002`，不复制其内容；公共落盘件指向 AGENTS.md 约定 13
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `06c6a18`（`06c6a1890f7b`，`docs/clients/pi.md` + `AGENTS.md`；票面回填在其后一次提交）
- 验证（本机，本票只动 markdown）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored`（文档票，测试数不变）
  - **搬干净的机器核对**：`enabledModels` / `openai-completions` / `applyModelsJson` / `proper-lockfile` / `modelOverrides` / `pi_leftovers_in` / `contextWindow` / `reasoning: true` 在 AGENTS.md 里均为 **0 处**
  - AGENTS.md 行数：**202 → 189**
- **偏离 spec：无实质偏离。** 三处需要记一笔：
  1. **骨架加了两节**（同票 03）：「怎么写」与「范围：只支持 API key 这一路」。七节骨架全在、顺序未变。
  2. **补了两条约定 14 未写、代码里已有的事实**（不是新发现）：`models.json` 与它的 `.apim.bak` 都经 `file_io` → `config::write_private`（**600，建文件时即 600**）；找 pi 可执行文件的顺序是 `APIM_PI_BIN` → `PI_BIN` → `PATH` → 三个常见路径（`/opt/homebrew/bin`、`/usr/local/bin`、`/usr/bin`）。
  3. **Doc map 本票未动**：票 03 已把「客户端契约细节」行落成 `docs/clients/*.md`（去掉「尚未建」注记），所以 04 不需要再改同一行。
- **本批范围外、仍与两份客户端文档重叠的 AGENTS.md 位置**（我按「不要提前动 01/07」没动）：
  - 第 34 / 36 行：目录结构的 `pi/import.rs`（`apim-` 前缀）与 `pi/verify.rs`（`pi --list-models`）注解 → 票 07 的「目录结构去重」；
  - 第 109 / 110 行：运行时数据里的 Codex / Pi 文件清单（含「`settings.json` / `auth.json` 都不写」） → 票 07 的「运行时数据审计」；
  - 第 137 行：约定 13 里「选默认模型按客户端可关…（pi 就是这样）」—— 约定 13 不在本批三票的范围内。
