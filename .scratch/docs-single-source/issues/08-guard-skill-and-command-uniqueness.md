# 08: 守卫 —— SKILL 不定义命令面 + 禁令唯一化

Status: done
Blocked by: 06, 07

**What to build:** 两条「同一事实只有一份」的不变量由机器守：（一）SKILL 里出现的每一条命令形式都能在 README 找到；（二）`cargo install --path .` 这条禁令在全仓只有两个允许落点。两条规则在 06 / 07 落地后**从红变绿**，此后任何一次单边编辑都会立刻变红。

- [x] **R2a**：SKILL 里出现在 `apim` 调用行上的动词与 `--flag`，必须都出现在 README 的 `apim` 行上（子集关系；逻辑行先接反斜杠续行）
- [x] **R2b**：SKILL 必须含指向 README CLI 章节的指针
- [x] **R4**：那条禁令只允许出现在白名单（README 两版不限次、`AGENTS.md` 恰好 1 处、`docs/RELEASING.md` 恰好 1 处）；其余长期文档（仓库根 + `docs/` + `.agents/`，**刻意不含 `.scratch/`**）出现即失败
- [x] 两条规则都**观察到红**（06/07 已先把真实缺口修掉了，所以用四处反向变更各自单独验；见下方 Done）
- [x] **把边界写进测试附近**：子集检查只能抓「发明了不存在的命令 / 两份说法互相矛盾」，**抓不到遗漏** —— 写进了 `the_skill_does_not_redefine_the_cli_surface` 的文档注释
- [x] 断言只碰 token 集合与链接存在性，不碰译文措辞
- [x] 失败信息点名文件 + 规则 + 出错 token 的前后窗口 + 怎么修
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `a0ce5a8`（`a0ce5a8dfbcf`，`tests/docs.rs`；票面回填在其后一次提交）
- 验证（本机）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored` + 守卫 `7 passed`
  - **四处反向变更各自单独验过**（每次都恢复）：①SKILL 的调用行塞 `--frobnicate` → `the_skill_does_not_redefine_the_cli_surface` FAILED（报出那条逻辑行的前后窗口 + 怎么修）；②删掉指向 README CLI 一节的指针 → 同一条 FAILED；③`docs/RELEASING.md` 加第二处禁令 → `the_dev_install_command_stays_in_its_own_home` FAILED（报「出现 2 次，白名单只允许 1 次（渠道表要用它说明源码渠道不自动更新）」）；④往 `docs/provider-kinds.md` 写禁令 → 同一条 FAILED（报「这里不是它的家」+ 两条正确落点）
- **偏离 spec：两处，均为把口径写准。**
  1. **R4 的扫描范围与白名单比票面多了一层**：票面说「README 的安装节 + `AGENTS.md` 恰好一处；其余任何 tracked `.md`」。实做判的是**长期文档**（仓库根非递归 + `docs/` + `.agents/` 递归），**不含 `.scratch/`** —— 那里是进行中的工作区，spec/ticket 本来就要讨论这条禁令（本特性的 spec 里就写着它），拿它当「文档」去卡只会自伤。另：`docs/RELEASING.md` 的**渠道速查表**多了一个白名单（恰好 1 次）—— 它要点名「源码」这个渠道，正是为了说明它**不在** `apim update` 覆盖范围内，那是另一个事实、不是禁令的第二份。
  2. **R2 只扫「调了 `apim` 的逻辑行」，并把反斜杠续行接起来**：票面写的「`apim` 调用行」如果不做续行拼接，SKILL 里 `apim provider add … \` 那串示例只有第一行会被检查，`--homepage` / `--health` / `--script` 会漏检。另：**绝不能全文件扫开关** —— SKILL 里还有 curl（`--connect-timeout` / `--max-time`）、cargo（`--all-targets` / `--ignored` / `--nocapture`）与 codex（`--profile`）的开关，它们不是 apim 的命令面（票 06 的 `## Done` 里也预先提醒过这一点）。
- 另：本票落下的**新守卫有两个刻意弱点**，写在这里以免下个人误以为它们比实际强：①R2 是子集关系，**抓不到遗漏**（SKILL 少写一整块不会红）；②R4 只能抓字面串，换成 `cargo  install`（双空格）或 `cargo install ./` 这类变体它就看不见。两个弱点都是「删掉第二份」而不是「检查第二份」的直接后果 —— 本特性的取向就是前者。
