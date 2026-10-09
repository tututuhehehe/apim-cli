# ADR-0007：容忍三处小重复（附动手触发条件）

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §2.5

## Context

第三轮 review 记下三处低价值重复：

- `ps` 行解析：`is_codex_server` 与 `looks_like_wrapped_codex_server` 各自 split / basename
- 光标钳位：`app/import/mod.rs::clamp_index` vs `app/modal.rs` 里的内联 `clamp`
- 过滤谓词：`ImportFlow::visible` vs `modal::filter_models`（语义相同、类型不同）

## Decision

**保留**，不抽公共零件。「两边列表一致」已经有测试守住，此刻抽取只会增加间接层。

## Consequences

- `ui/import.rs` 307 行，属软上限临界：**再涨就拆**（拆文件归 `docs/TODO.md` TODO-14 那一类活）。
- **动手触发条件**：出现第四处同类重复，或其中一处开始漂移（相关测试变红）时，就该抽公共零件了。
