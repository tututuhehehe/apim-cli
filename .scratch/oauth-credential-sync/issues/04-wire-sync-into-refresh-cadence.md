# 04: 回写接线 —— 挂上刷新节奏 + 端到端 + README 双语改口

Status: done
Blocked by: 02, 03

**What to build:** 让 03 的判定真的跑起来：在既有的刷新节奏上（启动一次、每 `AUTO_REFRESH_INTERVAL` 一次、`r` 一次）**先回写、再探活**，于是 Codex 自己刷新过的 token 被 apim 采纳，额度查询不必再自己刷一遍。写盘走 02 的受限写，所以「回写 vs 登录」也是安全的（谁先落盘谁赢，登录永远赢）。同时把 README 双语里那句已经变成假话的说法改掉。

- [x] 挂上节奏：在既有刷新入口里先做回写、再做 OAuth 探活；回写用 02 的受限写（baseline = 开始时的 apim 凭据）
- [x] 无凭据 / `auth.json` 缺失 / `auth_mode` 不是 chatgpt / ownership 不吻合 / token 不更新 → **静默跳过**，不弹 toast、不改任何文件、不报错
- [x] 回写**绝不写** `~/.codex/auth.json`（单向：codex → apim；只经 `read_local_login` 的 `fs::read_to_string`）
- [x] 端到端沙盒（临时 `CODEX_HOME` + 临时配置目录，**合成凭据、不读真实 token**）：更新过的同账号 `auth.json` → apim 凭据被更新、逐字段正确、权限 **600**；另一个账号的 `auth.json` → apim 文件**逐字节未变**
- [x] 幂等：连续跑两次节奏点，第二次不写盘（同一份 token 不会被反复写）
- [x] README **双语**同步改口（只改那一句，两版各 1 行，见 `git show f91ba1d -- README.md`）
- [x] 不碰 `codex_route` / 额度面板 / AUTH 行的显示与交互（`ADR-0008`）
- [x] **删掉票 03 引入的 7 处 `#[allow(dead_code)]`**，且**无新增**（现状 `grep -rn allow(dead_code) src/` = 0 处；为删干净做了一处形状调整，见下方偏离 ①）
- [x] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿（按轮次规矩跑的三条见下方，更严）

## Done

- commit: `f91ba1d`（`f91ba1d5923c`，代码 + README + 测试；票面回填在其后一次提交）
- 验证（本机 `aarch64-apple-darwin`）：
  - `cargo fmt --all -- --check` → 无 diff
  - `cargo clippy --all-targets -- -D warnings` → 0 warning，exit 0（另跑非测试的 `cargo build`：0 warning）；`#[allow(dead_code)]` 全仓 0 处
  - `cargo test --all` → `335 passed; 0 failed; 4 ignored`（本票 +3）
  - 新增测试（均在 `app::tests`，走真实节奏点 `refresh_active_keys`）：`codex_refreshed_tokens_are_adopted_into_apims_credential`、`a_foreign_codex_login_is_never_copied_into_apim`、`adopting_from_codex_does_nothing_without_a_stored_credential`
  - **守卫确实会红**（三个反向变更**各自单独跑**）：①接线断掉（`sync_oauth_from_codex` 直接 return）→ `codex_refreshed_tokens…` FAILED；②更新判断改成 `if exp == 0`（相同/更旧也采纳）→ `codex_refreshed_tokens…`（幂等那条 mtime 断言）+ 票 03 的判定表 FAILED；③ownership 硬门改 `if false` → `a_foreign_codex_login_is_never_copied_into_apim` FAILED。三次恢复后均回绿
  - **验证方法上的坑（记下来）**：第一轮我用 `cargo test --all oauth` 筛选，而本票三个测试名里根本没有 `oauth` 字样 —— 那个筛选只跑到了 3 条无关测试，于是给出「绿」的假象。发现后改成跑全量 + 按完整测试名 grep。**筛选器写错 = 假绿**，与上一轮「两个反向变更互相掩盖」是同一类坑
- **偏离 spec：三处，均为实现后果，无放松。**
  1. `AdoptDecision` 的形状从票 03 的 `Reject(&'static str)` / `Skip(&'static str)` 改成四个**具名变体**（`Incomplete` / `NotOurs` / `UnknownExpiry` / `NotNewer`）+ `Adopt(Box<Credential>)`。原因：删掉 allow 后 lint 直接指出「这两个载荷没有任何生产读者」——坦白说：生产路径静默、不读原因，确实没人读。想过把原因写进 `openai-oauth.log`，**否掉了**：`AttemptLog::save` 是**覆盖写**，后台每 tick 写一次会把「上一次登录/刷新的诊断」冲掉，而那正是那个文件存在的理由。改成具名变体后测试按类型断言（比原来的子串匹配更强：理由指错了也能拓出来）。
  2. `clients/codex/mod.rs` 的再导出里**只留 `read_local_login`，不留 `CodexLogin`** —— app 只用函数、类型靠推导，带上类型名会被 `unused_imports` 拓（同样没加 allow）。
  3. 多了一条票面没要求的 E2E：`adopting_from_codex_does_nothing_without_a_stored_credential`（没登录过 / API Key 路 → 不凭空造凭据）。成本极低，守的是「别自己造一份凭据出来」这种没法回退的事故。
- **已知覆盖缺口（spec 已声明）**：「刷新跳过 → 重读重试一次」那条编排跳仍无自动化测试（票 02 的 `## Done` 已记）。
- **留给票 05 的两处陈旧表述（本票按你的规矩没碰）**：①`docs/TODO.md` 的 `TODO-2` 标题（票 05 会删掉）；②`docs/adr/0008` 末尾那句「`TODO-2`（codex 刷新后不回写 apim）」现在与实现相反 —— 但 `docs/adr/README.md` 定了「已 Accepted 的 ADR 不要改内容，要改就新写一条」，所以留给你定：新写一条 ADR 取代它，或是只删 TODO 条目、容一句陈旧的相关性备注。
- 另：`~/.codex/auth.json` 每 tick 会被读两次（`codex_route` 一次、回写一次）—— 这是 spec 显式接受的小重复（与 `ADR-0007` 同旨），记在这里以免将来被当成新发现。
