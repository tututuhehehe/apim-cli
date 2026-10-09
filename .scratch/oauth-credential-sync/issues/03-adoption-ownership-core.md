# 03: 回写核心 —— 认 ownership 的采纳判定

Status: done
Blocked by: 无（可立即开工）

**What to build:** 交付「从 Codex 现场取回的 token 到底该不该采纳、采纳哪些字段」这个判定的**全部**正确性，并且它是纯函数、可表驱动证明。硬门是用户给的红线：只有 `auth.json` 里的 refresh token 与 apim 自己那份**逐字符相同**才可能采纳 —— 用户自己 `codex login` 的另一个账号一个字段都不能被抄进 apim。同时交付 `auth.json` 的只读解析（客户端契约留在客户端适配里）。

- [x] `auth.json` 只读入口（`clients::codex`）：给一个 codex 目录 → 解析出「三个 token」的小结构，或 `None`；只认 `auth_mode == "chatgpt"` 且三个 token 齐全（**与 AC 原话的差别见下方 Done**）
- [x] 纯判定函数（凭据模块）：输入 apim 自己那份 + 收到的 token 快照 + 收到的 access token 有效期 → 输出**拒绝 / 跳过 / 采纳（新凭据）**三态，不做任何 IO
- [x] **ownership 硬门**：refresh token 不逐字符相等 → **拒绝**（另一个账号的情形，一个字都不写）
- [x] **更新判断**：只在收到的 access token `exp` **晚于** apim 当前 `expires_at` 时才采纳；`exp` 解析不出 → 拒绝（绝不替它编有效期）；token 更旧或相同 → 跳过（保证幂等）
- [x] **字段处置**：只采纳 `access_token` / `refresh_token` / `id_token` / 派生出的 `expires_at`；`client_id` / `host_id` / `subject` / `email` / `scopes` / `codex_family` 一概保持 apim 原值；`account_id` 按既有刷新口径从新 access token 推导
- [x] 表驱动测试覆盖：同账号且更新 → 采纳；refresh token 不吻合 → 拒绝；更旧/相同 → 跳过；缺字段 → 拒绝；`exp` 缺失 → 拒绝；采纳结果**逐字段**断言（哪些变了、哪些没变）
- [x] `auth.json` 解析的测试用合成 JSON 字符串（含 API Key 登录、缺 token、空文件三种非 ChatGPT 情形）
- [x] 本票不落盘、不接线（写入与节奏见 04）；**不写** `~/.codex/auth.json`（只用 `fs::read_to_string`，有用例断言文件逐字节未变）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `b52569c`（`b52569cbea43`，代码与测试；票面回填在其后一次提交）
- 验证（本机 `aarch64-apple-darwin`）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0（另跑了非测试的 `cargo build`：0 warning）
  - `cargo test --all` → `327 passed; 0 failed; 4 ignored`（本票 +3）
  - 新增测试：`clients::codex::official::tests::local_login_snapshot_needs_chatgpt_mode_and_all_three_tokens`、`openai_auth::tests::adoption_requires_ownership_and_a_newer_token`（判定表 ①–⑥）、`openai_auth::tests::adoption_entry_point_reads_the_expiry_from_the_token_itself`
  - **守卫确实会红**（两轮临时反向变更，验完已恢复）：①ownership 硬门改成 `if false`、②更新判断改成 `if exp <= 0`、③「读不出有效期→拒绝」改成 `unwrap_or(u64::MAX)` → 两条判定测试 FAILED；④解析器改成「空/空白 token 也算数」→ `local_login_snapshot…` FAILED
- **偏离 spec：两处。**
  1. AC 原话写的是「解析出三个 token **+ account id** 的小结构」，实做的 `CodexLogin` 只带三个 token。理由：依据 spec 决定 3，采纳后的 `account_id` 要从**新 access token** 推导（与既有 `refresh` 同一口径），而 `auth.json` 的 `account_id` 是 codex 自己的元数据、在采纳路径上用不到；带一个没人读的字段会撞本仓的 dead-code 告警（`clients/codex/mod.rs` 里已记“二进制 crate 里没人用的项会被告警”这个坑）。该字段与路由判定用的宽松谓词 `auth_has_chatgpt_tokens` 保持原样不动。
  2. 新增的 7 处 `#[allow(dead_code)]`（5 处在凭据模块、2 处在 codex 适配层）：本票按票面要求**不接线**，而本仓对未使用的项会告警，所以这批入口在票 04 接线前只有测试在用。接线时删掉这 7 行 —— 票 04 的验收里要指名这一项。
- 另：AC 末条写的是 `cargo fmt && cargo clippy -q …`，本次按轮次规矩跑的是 `cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test --all`（更严）。
