# 05: 收尾 —— 台账与 CHANGELOG 归位

Status: done
Blocked by: 01, 02, 04

**What to build:** 两件都做完之后，按 Doc map 的生命周期收口：台账里的条目删掉、结论进 `CHANGELOG.md`、`docs/TODO.md` 不再留已完成的条目。

- [x] `docs/TODO.md` 删掉 `TODO-2`（官方路不回写同步）与 `TODO-12`（last-writer-wins 竞争）
- [x] 同时删掉 `TODO-13`（票 01 已纳入并完成）
- [x] `CHANGELOG.md` 记一笔（新增 `[Unreleased]` 段，按体例写「已修复 / 新增」两类；**没写内部实现细节**）
- [x] 核对该做的事都做了：`README` 双语不再声称「不回写」（票 04）；`ADR-0006` 未被改动；`ADR-0008` 的决策未被改动（只由你换了末尾备注，见下）；`~/.codex/auth.json` 依旧只读
- [x] `.scratch/oauth-credential-sync/` 保留（工作过程留在原地，按 Doc map）
- [x] 新开 `TODO-16` 登记已知覆盖缺口（票面原清单没有这条，是你本轮加的要求）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `215293a`（`215293a17c7f`，`CHANGELOG.md` + `docs/TODO.md`；票面回填在其后一次提交）
- 验证（本机 `aarch64-apple-darwin`）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored`（本票只改文档，测试数不变）
- **删掉的台账条目：`TODO-2`、`TODO-12`、`TODO-13`**（分别 -17 / -10 / -8 行，含票 02 与票 04 开工时加的「已开工 → 收尾时删掉本条」提示行）。
- **新开的台账条目：`TODO-16`**（现状 / 为什么现在不做 / 第一步 —— 第一步是把「一次尝试」抽成吃注入闭包的薄函数再表驱动测，先例是 `cli/uninstall` 的 `run_with` 与 `app/import` 的 runner）。
- `TODO-3` **无需改动**：它五条报告级小账（AUTH 行提示与厂商类型、密钥过滤为空时的空面板文案、`toml_edit` 摘键连注释、`verify_login` 绑死措辞、CLI `import-codex` 不区分 ambiguous daemon）都没被本轮碰到，描述仍然成立。
- **弦空引用自检（你要求的第 5 项）**：写了个脚本比对 `docs/TODO.md` 里定义的 `TODO-N` 与正文引用的 `TODO-N` —— 第一遍就抓到一处：`TODO-16` 的「为什么现在不做」里本来引了一句 `TODO-13`（已删）。已改成不带编号的描述，重跑后**弦空引用 = 0**。
- **偏离/说明：三处**
  1. `CHANGELOG` 的类别：票面写的是「修复 / 内部」，你本轮改为「已修复 / 新增」——按你的口径走，没有 `### 内部` 段（内部实现进了 `ADR-0009`）。
  2. 顺手改了「已归位」里那句 `ADR-0001~0007`：**ADR-0009 已经加进来了，这个区间不写就陈旧**。改成指向 `docs/adr/README.md` 索引，以后新增 ADR 不会再留一句要同步的区间。
  3. 你自己的三处 ADR 改动（`ADR-0008` 末尾备注、`ADR-0009` 新建、`docs/adr/README.md` 索引）在写本票时**尚未提交**，我按你的规矩没动、也没 stage，本票只提交了 `CHANGELOG.md` 与 `docs/TODO.md`。
- **给你的一处提醒（在 ADR 里，我没动）**：`docs/adr/0008` 末尾与 `docs/adr/README.md` 的来源行、`docs/adr/0009` 的来源行都提到 `TODO-2` / `TODO-12`，而这两个条目已从台账删除。它们写的是「已落地」的历史来源，不算误导，但编号现在指不到东西 —— 要不要收一下笔由你定（我不确定你那边是否希望 ADR 里保留原台账编号作痕迹）。
