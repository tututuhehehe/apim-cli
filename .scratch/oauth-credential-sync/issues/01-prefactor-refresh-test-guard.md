# 01: prefactor —— 给 `refresh` 的档位门与落盘顺序加测试守卫

Status: done
Blocked by: 无（可立即开工）

**What to build:** 本轮要改的那段代码（`refresh` 的请求表单 + 「先落盘、再推导 account id」的两次写盘）先有兜底测试：把请求表单抽成纯函数并按档位表驱动断言；把两次写盘的编排做成可注入的形态，从而能用一条测试钉住「新 token 一定先落盘」。做完之后，把这两条规则改回去会立刻变红 —— 这正是 `TODO-13` 记的缺口。

- [x] 请求表单抽成纯函数：给定凭据 → 期望的键值对；表驱动覆盖 Codex 档位与动态注册档位两行，断言只有非 Codex 档位带 `resource`
- [x] 落盘顺序可验证：抽一个可注入的编排（**注入点见下方偏离说明**），断言「POST 成功后先落盘、再推导 account id」这一顺序
- [x] 两条测试都**不使用真实网络与真实凭据**（合成凭据即可）
- [x] 不改变任何外部行为（本票只加测试与为可测性做的最小抽取）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `fb5f39f`（`fb5f39fc3286`，代码与测试；票面回填在其后一次提交）
- 验证（本机 `aarch64-apple-darwin`）：
  - `cargo fmt --all -- --check` → 无 diff（先跑 `cargo fmt --all` 修了两处换行）
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0
  - `cargo test --all` → `324 passed; 0 failed; 4 ignored`
  - `cargo test --all openai_auth` → `19 passed`，新增 4 条：`refresh_form_follows_the_profile`、`refreshed_tokens_hit_the_disk_before_the_account_id_is_derived`、`refreshed_tokens_stay_on_disk_when_the_account_id_cannot_be_derived`、`refresh_response_merges_tokens_and_keeps_the_old_refresh_token_when_not_rotated`
  - **守卫确实会红**：临时把档位门改成「Codex 档位也带 `resource`」、并删掉 `persist_refreshed` 的第一步 `save` → 4 条测试 FAILED；恢复后 19 passed
- **偏离 spec：一处。** AC 写的是「假 http 闭包注入」，实现改成**注入响应**（`refresh_from_response(…, TokenResponse, …)`）+ 在 `persist_refreshed` 的 `derive` 处注入。理由：假 http 闭包要返回一个借用 `&mut AttemptLog` 与表单的 future，闭包签名需要 HRTB / boxed future 才能表达，而收益与「注入响应」等价 —— 「先落盘再推导」这个不变量在响应返回之后才成立，注入响应就能完整钉住它；合并规则另有纯测试，覆盖面没有减少。
- 另：AC 末条写的是 `cargo fmt && cargo clippy -q …`，本次按轮次规矩跑的是 `cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test --all`（更严）。
