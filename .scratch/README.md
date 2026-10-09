# `.scratch/` — 进行中特性的工作区

这个目录装**正在进行**的特性：一个特性一个子目录 `<feature-slug>/`。

```
.scratch/<feature-slug>/
├── spec.md                       # /skill:to-spec 产出：为什么做、目标、明确不做什么
└── issues/
    ├── 01-<slug>.md              # Status: … / Blocked by: … / 正文
    └── 02-<slug>.md
```

规矩见 `docs/agents/issue-tracker.md`；角色边界见 `AGENTS.md` 的 Doc map。
长期缺口台账是 `docs/TODO.md`，两边不互相复制。

目录要提交进 git（别的机器、别的评审 agent 也要能看到）。
