# 08: 守卫 —— SKILL 不定义命令面 + 禁令唯一化

Status: ready-for-agent
Blocked by: 06, 07

**What to build:** 两条「同一事实只有一份」的不变量由机器守：（一）SKILL 里出现的每一条命令形式都能在 README 找到；（二）`cargo install --path .` 这条禁令在全仓只有两个允许落点。两条规则在 06 / 07 落地后**从红变绿**，此后任何一次单边编辑都会立刻变红。

- [ ] **R2a**：SKILL 里出现在 `apim` 调用行上的动词与 `--flag`，必须都出现在 README 的 `apim` 行上（子集关系）
- [ ] **R2b**：SKILL 必须含指向 README CLI 章节的链接
- [ ] **R4**：`cargo install --path .` 只允许出现在 README 的安装节与 AGENTS.md 恰好一处（约定 10）；其余任何 tracked `.md` 出现即失败
- [ ] 两条规则今天都必须先观察到红、再由 06 / 07 的修复转绿
- [ ] **把边界写进测试附近**：子集检查只能抓「发明了不存在的命令 / 两份说法互相矛盾」，**抓不到遗漏**（SKILL 缺 `apim auth` 这类漂移无法靠集合关系发现）—— 这正是本特性的解法是**删掉第二份**而不是「检查第二份」的原因
- [ ] 断言只碰 token 集合与链接存在性，不碰译文措辞
- [ ] 失败信息点名文件与缺失项
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
