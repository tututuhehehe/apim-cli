# 02: 写入闸 —— 登录与在途探针不再互相覆盖

Status: done
Blocked by: 01

**What to build:** 结束 `TODO-12` 的用户可见故障：登录成功后磁盘上**一定**留的是刚登进去的那份凭据，哪怕是有一个在途的额度探针（它 `load` 在登录之前、`save` 在登录之后）。做法是把凭据文件分成两类写者 —— 登录是权威写（无条件），一切从已加载快照派生的写（刷新）带上「我读到的那份」并**在写锁内重读磁盘**比对身份，不一致就跳过、不写。用户可见结果：底栏说「已连接」不再是谎话；`apim auth openai logout` 之后在途刷新也不会把凭据复活。

- [x] 新增受限写：带 baseline（写者开始时读到的那份凭据），在既有写锁内重读磁盘、比对身份，一致才写，并返回「写了 / 跳过了」
- [x] 身份 = `client_id` + `refresh_token`；另两种边界同样跳过：baseline 是「本来没有」而现在有、baseline 是「有」而现在没有（被注销删掉）
- [x] 既有 `save` 的语义与签名不变，登录继续无条件写（它是权威写）
- [x] **写完把 baseline 推进成刚写进去的那份**：`refresh` 一次调用里的第二次写盘（补 account id）拿第一次的结果当 baseline，否则 account id 永远补不上
- [x] 刷新被跳过时**不报错**：`fetch_usage` 重读磁盘后整体重试**一次**（上界 `USAGE_ATTEMPTS = 2`），二次跳过按「这一轮没有读数」处理、不弹错误
- [x] 重读磁盘发生在写锁内且**不跨 await**
- [x] **不新增全局状态**（身份由磁盘内容推导，无计数器 / epoch / 新全局标志）
- [x] 写盘仍走既有私有写（tmp + rename + **建文件即 0600**），无 `fs::write` + `chmod`
- [x] 测试（沙盒临时目录）：登录先落盘后派生写者被跳过、注销后不复活、身份判定四情形、坏文件不被覆盖、受限写的 0600
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `283058e`（`283058e49fb5`，代码与测试；票面回填在其后一次提交）
- 验证（本机 `aarch64-apple-darwin`）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0（另跑非测试的 `cargo build`：0 warning）
  - `cargo test --all` → `332 passed; 0 failed; 4 ignored`（本票 +5）
  - 新增测试：`a_stale_derived_writer_cannot_overwrite_a_fresh_login`（用户可见故障本身）、`a_stale_writer_cannot_resurrect_a_removed_credential`（注销）、`the_write_gate_only_writes_for_a_matching_identity`（①–⑤ 判定表）、`the_write_gate_refuses_when_the_current_file_cannot_be_read`、`the_write_gate_keeps_the_credential_private`
  - **守卫确实会红**（两轮临时反向变更，验完已恢复）：①把身份比较改成 `if false`（无条件写）→ 4 条 FAILED；②单独删掉 `persist_refreshed` 里那句「基线推进」→ `refreshed_tokens_hit_the_disk_before_the_account_id_is_derived` 与 `refresh_response_merges_…` FAILED（第一次跑时两个变更同时上了，①把②的效果掩盖了，所以②单独重跑了一遍）
- **偏离 spec：两处，均为加严 / 结构化，无放松。**
  1. 多了一条 AC 没列的边界：**当前文件读不出来（坏 JSON / 被删）一律不写**。理由：CAS 的前提是能**证明**身份，证不出来就不写（与回写判定的 fail-closed 同旨）——AC 里的「被注销删掉」只是这条的一个特例，单列出来更清楚。
  2. `fetch_usage_locked` 里把「拿到可用凭据之后真正打用量接口」那段抽成了 `query_usage(http, c)`（重试循环里只留 load → refresh → read 三行）。行为不变，只为让上界一眼可见；原函数里的权限 / account id 检查与错误文案一字未改。
  3. 另：`WriteBaseline` / `WriteOutcome` / `baseline_of` / `save_guarded` 做成 `pub(crate)` 而不是私有 —— 票 04 的回写要用同一条闸，提前把接口摆出来（它们在票 02 内部已被生产代码使用，不会撞 dead-code）。
  4. 票 01 的两条测试（`persist_refreshed` / `refresh_from_response`）随签名调整：多传一个 `&mut baseline`，断言一字未改。
- **已知覆盖缺口（spec 已声明，不是本次偷工）**：「跳过 → 重读重试一次」这条编排路径没有自动化测试（本仓不引 HTTP mock，见 `TODO-13`），靠代码审查 + 上界写死为 2 次保证。这样本票的红线（登录不被覆盖、注销不复活、0600、无新全局状态）全部有测试，仅「重试」这一跳没有。
- 另：AC 末条写的是 `cargo fmt && cargo clippy -q …`，本次按轮次规矩跑的是 `cargo fmt --all -- --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test --all`（更严）。
