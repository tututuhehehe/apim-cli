# 03: 回写核心 —— 认 ownership 的采纳判定

Status: ready-for-agent
Blocked by: 无（可立即开工）

**What to build:** 交付「从 Codex 现场取回的 token 到底该不该采纳、采纳哪些字段」这个判定的**全部**正确性，并且它是纯函数、可表驱动证明。硬门是用户给的红线：只有 `auth.json` 里的 refresh token 与 apim 自己那份**逐字符相同**才可能采纳 —— 用户自己 `codex login` 的另一个账号一个字段都不能被抄进 apim。同时交付 `auth.json` 的只读解析（客户端契约留在客户端适配里）。

- [ ] `auth.json` 只读入口（`clients::codex`）：给一个 codex 目录 → 解析出「三个 token + account id」的小结构，或 `None`；只认 `auth_mode == "chatgpt"` 且三个 token 齐全
- [ ] 纯判定函数（凭据模块）：输入 apim 自己那份 + 收到的 token 快照 + 收到的 access token 有效期 → 输出**拒绝 / 跳过 / 采纳（新凭据）**三态，不做任何 IO
- [ ] **ownership 硬门**：refresh token 不逐字符相等 → **拒绝**（另一个账号的情形，一个字都不写）
- [ ] **更新判断**：只在收到的 access token `exp` **晚于** apim 当前 `expires_at` 时才采纳；`exp` 解析不出 → 拒绝（绝不替它编有效期）；token 更旧或相同 → 跳过（保证幂等）
- [ ] **字段处置**：只采纳 `access_token` / `refresh_token` / `id_token` / 派生出的 `expires_at`；`client_id` / `host_id` / `subject` / `email` / `scopes` / `codex_family` 一概保持 apim 原值；`account_id` 按既有刷新口径从新 access token 推导
- [ ] 表驱动测试覆盖：同账号且更新 → 采纳；refresh token 不吻合 → 拒绝；更旧/相同 → 跳过；缺字段 → 拒绝；`exp` 缺失 → 拒绝；采纳结果**逐字段**断言（哪些变了、哪些没变）
- [ ] `auth.json` 解析的测试用合成 JSON 字符串（含 API Key 登录、缺 token、空文件三种非 ChatGPT 情形）
- [ ] 本票不落盘、不接线（写入与节奏见 04）；**不写** `~/.codex/auth.json`
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
