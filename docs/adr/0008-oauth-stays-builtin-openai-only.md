# ADR-0008：OAuth 只服务内置 `openai`，不做「凭据按厂商存」

- 状态：Accepted（2026-10-09）
- 来源：`TODO-1` 的取向决议（原调研见 `docs/TODO.md` 的墓碑条目）

## Context

`AUTH` 行与 `x` 键（把凭据导入 Codex 官方路）走的是 OpenAI 官方动态注册流程，凭据存在 `~/.config/apim/openai-oauth.json`（**单份**）+ 伴生 host-id。apim 侧 `openai_auth` 的路径、`App` 的 OAuth 状态、以及 6 处「id 正好是 `openai`」的门，都是围绕「只有一份」建立的。

`TODO-1` 曾计划把凭据改成按厂商 id 分表，从而让**复制出来的 `openai`** 也能登录。背景约束：Codex 官方路在一个 `CODEX_HOME` 下**只有一个登录位**（`~/.codex/auth.json`），`codex --profile` 换的是 config、不是 auth。

## Decision

**暂时不变，维持现状：**

1. `AUTH` 行与 OAuth 登录能力**只给内置 `openai`** —— 它是特殊通道，不发给其他厂商（包括复制出来的厂商）。
2. 凭据保持**单份**（`openai-oauth.json` + 伴生 host-id），**不**按厂商 id 分表。
3. `recipe` **不加** OAuth 能力字段；那 6 处 `current_provider_id() == Some("openai")` 的门保持原样。
4. **继续禁止复制内置 `openai`**（`recipe/dup.rs::ensure_copyable`；TUI `y` 与 CLI `provider copy` 都拒绝），README 双语里的说明与单测 `provider_copy_refuses_the_builtin_openai` 保持不动。
5. `x` 的语义不变：在内置 `openai` 的 `AUTH` 行上按 `x` = 通过 OpenAI 官方方式给 Codex 用。

## Consequences

- 「复制的 OpenAI 也能登录」这个目标作废 → `TODO-1` 关闭（见台账里的墓碑条目）。
- 6 处重复的 id 门保留：不做能力字段，就没有替掉它们的理由。
- 复制牵扯面大（官方登录位单槽 + 导入路径 + 备份/回滚 + 文档双语 + `★` 对账），当前收益不足以承担。
- **何时该重开**：真的需要「同一个 OpenAI 厂商 + 多个 ChatGPT 账号」时。届时先解决「Codex 官方登录位只有一个」的**呈现**问题（现场在用哪一份、怎么切），再谈凭据分表与放开复制。
- 相关但独立的问题：`TODO-12`（登录落盘 vs 在途探针的竞争）与 `TODO-2`（codex 刷新后不回写 apim）—— 两件都已在 `.scratch/oauth-credential-sync/` 落地（2026-10-09）。采纳方向与红线见 **`ADR-0009`**；本条 ADR 的决策不变。
