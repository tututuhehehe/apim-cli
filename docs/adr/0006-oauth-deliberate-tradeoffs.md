# ADR-0006：OAuth 的四个有意取舍（不是缺陷，别再改回去）

- 状态：Accepted（2026-10-01）
- 来源：原 `DEV-NOTES.local.md` §3.12

## Context

OAuth 落地过程中有四处看起来「可疑」、但都是权衡后保留的行为。评审与后来的人容易把它们当 bug 修掉。

## Decision

以下四点**有意如此**：

1. **`--snapshot` 会做真实 OAuth 读取，并可能旋转 refresh token** → OpenAI 面板依赖真实凭据 + 网络（离线时显示 `● 失败`）。符合本仓「`--snapshot` = 真接口」的口径。
2. **不用「按 `oauth_checking` 跳过探针」来省掉这次请求**：那会让快照永远停在「正在查询」（`App::start` 派发的探针结果没人消费）。现在用 `USAGE_LOCK` 串行化，代价是快照多一次只读 usage GET（纯延迟）。
3. **密钥表 AUTH 行在「刷新在途但有旧读数」时显示 `● 已连接`/`● 失败`**，而不是「… 查询中」：与额度面板保持一致，优先于在途信号。
4. **非缺省 `APIM_OAUTH_CLIENT=apim` 档位会把 `id_token_hint` / `login_hint` 放进浏览器 URL**（`ps` 与浏览器历史可见）。它是身份提示、不是 bearer token，故保留。

## Consequences

- 要恢复「在途」信号，得加一个独立的 `Refreshing` 状态 —— 那是新工作，不是修 bug。
- **何时该重开**：OpenAI 改变未公开的 ChatGPT usage 端点行为，或 Codex CLI 换掉 `Logged in using ChatGPT` 措辞（见 `docs/TODO.md` TODO-3 最后一条）。
