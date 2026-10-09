# ADR — 架构与取舍决策记录

这里记录**已经决定、不要再重新翻案**的取舍，包含「故意不做 + 理由」。

- 文件名：`NNNN-<slug>.md`，四位编号，顺序递增，**不复用**
- 状态：`Accepted`（生效中）／`Superseded by ADR-NNNN`（被取代）／`Deprecated`（不再适用）
- 一条 ADR 记一个决策：Context（背景）→ Decision（决定）→ Consequences（后果，含代价与**何时该重开**）
- 已 Accepted 的 ADR 不要改内容；要改就新写一条并标注取代关系
- **谁该读**：探索代码之前，先读与手上区域相关的 ADR（见 `docs/agents/domain.md`）；产出与既有 ADR 矛盾时要显式指出，不要默默覆盖

## 索引

| 编号 | 决策 | 状态 |
|---|---|---|
| [ADR-0001](0001-permission-tightening-only-for-secret-files.md) | 权限收紧只针对含密钥的文件（`apim-models.json` 保持 644） | Accepted |
| [ADR-0002](0002-client-adapters-are-rust-modules.md) | 客户端适配走 Rust 子模块；数据化范围只到厂商协议；不引 trait | Accepted |
| [ADR-0003](0003-codex-daemon-matching-is-argv2.md) | Codex daemon 匹配严格用 argv[2]，不做父 shell 加固 | Accepted |
| [ADR-0004](0004-windows-branches-not-verified-locally.md) | Windows 分支不在本机验证，靠 release 构建覆盖编译 | Accepted |
| [ADR-0005](0005-purge-refuses-symlinked-config-dir.md) | `--purge` 拒绝软链配置目录，且不提供 `--force` 绕过 | Accepted |
| [ADR-0006](0006-oauth-deliberate-tradeoffs.md) | OAuth 的四个有意取舍（不是缺陷，别再改回去） | Accepted |
| [ADR-0007](0007-tolerated-small-duplications.md) | 容忍三处小重复（附动手触发条件） | Accepted |
| [ADR-0008](0008-oauth-stays-builtin-openai-only.md) | OAuth 只服务内置 `openai`，不做「凭据按厂商存」 | Accepted |

> 来源：ADR-0001~0005、0007 由本机 `DEV-NOTES.local.md` §2 迁入（2026-10-09）；ADR-0006 由 §3.12 迁入；ADR-0008 由 `TODO-1` 的取向决议产生（2026-10-09）。
