# 04: 回写接线 —— 挂上刷新节奏 + 端到端 + README 双语改口

Status: ready-for-agent
Blocked by: 02, 03

**What to build:** 让 03 的判定真的跑起来：在既有的刷新节奏上（启动一次、每 `AUTO_REFRESH_INTERVAL` 一次、`r` 一次）**先回写、再探活**，于是 Codex 自己刷新过的 token 被 apim 采纳，额度查询不必再自己刷一遍。写盘走 02 的受限写，所以「回写 vs 登录」也是安全的（谁先落盘谁赢，登录永远赢）。同时把 README 双语里那句已经变成假话的说法改掉。

- [ ] 挂上节奏：在既有刷新入口里先做回写、再做 OAuth 探活；回写用 02 的受限写（baseline = 开始时的 apim 凭据）
- [ ] 无凭据 / `auth.json` 缺失 / `auth_mode` 不是 chatgpt / ownership 不吻合 / token 不更新 → **静默跳过**，不弹 toast、不改任何文件、不报错
- [ ] 回写**绝不写** `~/.codex/auth.json`（单向：codex → apim）
- [ ] 端到端沙盒（临时 `CODEX_HOME` + 临时配置目录，**合成凭据、不读真实 token**）：放一份「更新过的、同账号」的 `auth.json` → 断言 apim 凭据被更新、逐字段正确、权限仍是 **600**；换成「另一个账号」的 `auth.json` → 断言 apim 文件**逐字节未变**
- [ ] 幂等：连续跑两次节奏点，第二次不写盘（同一份 token 不会被反复写）
- [ ] README **双语**同步改口：`README.md` 与 `README.zh-CN.md` 里「codex 刷新后新 token 只在 `auth.json` 里 / apim 不回写同步」的说法，改成「apim 会在能证明是自己写进去的那份时采纳新 token；服务端轮换 refresh token 时仍可能需要按 `o` 重新授权」（两版一起改，约定 7）
- [ ] 不碰 `codex_route` / 额度面板 / AUTH 行的显示与交互（`ADR-0008`）
- [ ] **删掉票 03 引入的 7 处 `#[allow(dead_code)]`**（5 处在凭据模块、2 处在 codex 适配层）—— 接线之后那批入口真的有人用了，这 7 行就该消失
- [ ] `cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告 + `cargo test` 全绿
