# Issue tracker：本地 markdown

本仓的 issue 与 spec 以 markdown 文件放在 `.scratch/` 下。**角色边界看 `AGENTS.md` 的 Doc map** —— 那里规定「每类信息只有一个家，其它位置只放指针」。

## Conventions

- 一个特性一个目录：`.scratch/<feature-slug>/`
- 规格是 `.scratch/<feature-slug>/spec.md`
- ticket 一票一文件：`.scratch/<feature-slug>/issues/<NN>-<slug>.md`，从 `01` 起编号，**绝不**用一个合并的 ticket 文件
- ticket 文件顶部用一行 `Status:` 记录状态。取值：规范五值 `needs-triage` / `needs-info` / `ready-for-agent` / `ready-for-human` / `wontfix`（见 `triage-labels.md`；本仓尚未安装 `triage` 技能，该文件暂不存在），另加**本仓本地约定**的第六个取值 **`done`** = 本票已落地且验证通过（细节写在票文件末尾的 `## Done`：commit sha + 跑过的验证命令与结果 + 任何偏离 spec 之处）。
- 评论与对话历史追加在文件底部的 `## Comments` 标题之下

## 与 `docs/TODO.md` 的边界

- `docs/TODO.md` = **长期台账**：「已知缺口 / 为什么现在不做 / 真要做得动哪些地方」，条目编号 `TODO-N`。
- `.scratch/<feature-slug>/` = **进行中特性**的 spec 与 ticket 工作区；一个 `TODO-N` 开工时，从这里长出来。
- 收尾：结论进 `CHANGELOG.md`，遗留项回 `docs/TODO.md`，工作过程留在 `.scratch/`。
- 两处**不互相复制**：同一件事只写一处，另一处放指针。
- `.scratch/` 要提交进 git —— 别的机器、别的评审 agent 也要能看到；不要加进 `.gitignore`。

## 技能说「publish to the issue tracker」时

在 `.scratch/<feature-slug>/` 下新建文件（目录不存在就创建）。

## 技能说「fetch the relevant ticket」时

读对应路径的文件。用户通常会直接给路径或 ticket 编号。

## Wayfinding operations

供 `/wayfinder` 使用。**map** 是一个文件，每个 ticket 是一个 **child** 文件。

- **Map**：`.scratch/<effort>/map.md`（Notes / Decisions-so-far / Fog 正文）。
- **Child ticket**：`.scratch/<effort>/issues/NN-<slug>.md`，从 `01` 编号，正文是那个问题。`Type:` 行记录 ticket 类型（`research`/`prototype`/`grilling`/`task`）；`Status:` 行记录 `claimed`/`resolved`。
- **Blocking**：顶部一行 `Blocked by: NN, NN`。当一个 ticket 列出的文件全部 `resolved` 时，它才解锁。
- **Frontier**：扫描 `.scratch/<effort>/issues/`，挑出 open、unblocked、unclaimed 的文件；编号小的优先。
- **Claim**：先把 `Status: claimed` 写盘，再开工。
- **Resolve**：在 `## Answer` 标题下追加答案，置 `Status: resolved`，再把一条 context pointer（要点 + 链接）追加到 `map.md` 的 Decisions-so-far。
