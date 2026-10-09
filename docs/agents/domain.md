# Domain Docs

工程技能在探索本仓代码之前，该如何消费本仓的领域文档。

## 探索之前先读这些

- 仓库根的 **`GLOSSARY.md`**，或
- 仓库根的 **`GLOSSARY-MAP.md`**（如果存在）：它指向每个上下文一份 `GLOSSARY.md`，读其中与当前话题相关的那几份。
- **`docs/adr/`**：读与即将改动的区域相关的 ADR。多上下文仓库还要看 `src/<context>/docs/adr/` 下的上下文级决策。

这些文件若不存在，**安静地继续**。不要指出它们缺失，也不要主动建议先建。`/domain-modeling` 技能（经由 `/grill-with-docs` 与 `/improve-codebase-architecture` 抵达）会在术语或决策真正定下来时惰性创建它们。

## 文件结构

单上下文仓库（绝大多数仓库，本仓即此类）：

```
/
├── GLOSSARY.md
├── docs/adr/
│   ├── 0001-event-sourced-orders.md
│   └── 0002-postgres-for-write-model.md
└── src/
```

多上下文仓库（根部存在 `GLOSSARY-MAP.md` 时）：

```
/
├── GLOSSARY-MAP.md
├── docs/adr/                          ← 全系统决策
└── src/
    ├── ordering/
    │   ├── GLOSSARY.md
    │   └── docs/adr/                  ← 上下文专属决策
    └── billing/
        ├── GLOSSARY.md
        └── docs/adr/
```

## 使用 glossary 的词表

当你的产出要给领域概念命名时（票据标题、重构提案、假设、测试名），使用 `GLOSSARY.md` 里定义的术语，不要漂到 glossary 明确避免的同义词上去。

如果你需要的概念还不在 glossary 里，那是个信号：要么你在发明项目并不使用的语言（回头想想），要么这是真实的缺口（记下来交给 `/domain-modeling`）。

## ADR 冲突要显式指出

若你的产出与既有 ADR 矛盾，明确说出来，不要默默覆盖：

> _与 ADR-0007（event-sourced orders）冲突，但值得重开，因为……_
