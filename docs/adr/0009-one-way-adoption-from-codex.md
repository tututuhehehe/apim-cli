# ADR-0009：单向采纳 —— apim 只读 Codex 现场，永不写 `auth.json`

- 状态：Accepted（2026-10-09）
- 来源：`.scratch/oauth-credential-sync/`（对应 `TODO-2`）的实现决议；与 `ADR-0008`（凭据只服务内置 `openai`、保持单份）配套

## Context

按 `x` 把凭据导入 Codex 官方路之后，**现场归 Codex 所有**：它会自己刷新 access token，并只写回 `~/.codex/auth.json`。apim 那份 `~/.config/apim/openai-oauth.json` 就此落后 —— 额度查询要用旧 access token 自己再刷一次；服务端一旦轮换 refresh token，apim 那份还会彻底失效。

同时 apim 侧有**两个写者**：浏览器登录任务（最长 10 分钟）与每 5 分钟一档的在途探针（`fetch_usage` 的「load → 可能刷新 → save」读改写）。没有身份检查时，在途写者会把刚登录的凭据写回旧的。

## Decision

1. **方向单向**：apim 把 `auth.json` 当**输入**读，采纳进自己的凭据文件；**永不写 `auth.json`**。
2. **ownership 是硬门**：只有 `auth.json` 里的 refresh token 与 apim 存的那份**逐字符相等**，才认作「我们写进去的那份」。不相等（例如用户自己 `codex login` 的另一个账号）→ **一个字都不动**。
3. **更新判断保守**：只在新 access token 的 `exp` **晚于** apim 现有 `expires_at` 时采纳；解析不出有效期就**拒绝**（不替它编一个）。
4. **幂等**：采纳后同步推进 baseline，重复采纳同一份不会来回抖。
5. **fail-closed 且静默**：判定不了 / 读不出来 / 条件不满足 → 保持现状，不弹 toast、不做多余动作。最坏情况是「没同步」，绝不「抄错一份」。
6. **写者分两类**：登录是**权威写**（无条件，它就是「现在这份」的定义）；一切从已加载快照派生的写（刷新、采纳）**必须带 baseline**，在既有写锁内重读磁盘比对身份（身份 = `client_id` + `refresh_token`），不符则跳过。凭据被注销后，在途写者不得把它写回来。
7. 挂载在既有的刷新节奏（`refresh_active_keys`）上，**不新增全局状态**；写盘一律经 `config::write_private`（600，建文件时即 600）。

## Consequences

- **已知限制**：服务端真轮换 refresh token 时，ownership 门会拒绝采纳、退回「apim 落后于 Codex」的旧行为（spec 已声明）。要跟上得设计 ownership 的迁移策略 —— 那是新工作，不是修 bug。
- `~/.codex/auth.json` 每个 tick 会被读两次（`codex_route` 一次、采纳一次）：**显式接受**的小重复（与 `ADR-0007` 同旨）。
- README 双语不再声称「apim 不回写同步」。
- **何时该重开**：需要「apim 主动把凭据推给别的客户端」时 —— 那时要重新回答「谁拥有现场」。
- 审查/替换关系：本 ADR **不取代** `ADR-0008`（后者管的是「只有内置 `openai` 有 OAuth、凭据单份、不可复制」），两者并行成立。
