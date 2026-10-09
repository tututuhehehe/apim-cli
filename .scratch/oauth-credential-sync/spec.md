# OAuth 凭据同步：关掉登录与在途探针的写回竞争 + 从 Codex 现场回写新 token

Status: ready-for-agent
Feature: oauth-credential-sync
来源: `docs/TODO.md` 的 `TODO-12`（登录任务 vs 在途探针的 last-writer-wins）+ `TODO-2`（官方路导入不回写 codex 刷新的 token）；范围由用户 2026-10-09 拍板（「两件都做，同一轮」，见 `.scratch/open-questions.md` A07）。
约束来源: `docs/adr/0008`（OAuth 只服务内置 `openai`、凭据保持单份）、`docs/adr/0006`（OAuth 的四个有意取舍）。

## Problem Statement

apim 的 OpenAI Codex OAuth 凭据存在一份文件里，而**有两个写者**：浏览器登录任务（最长 10 分钟）与每 5 分钟一档的在途探针（`fetch_usage`）。写者之间没有「我读到的那份还是不是现在这份」的检查，于是有两个用户可见的故障：

**问题一 · 登录说成功，凭据却是旧的（`TODO-12`）。**
探针的流程是「`load` → 可能 POST 刷新 → `save`」的读改写。如果它在登录落盘**之前** load、在登录落盘**之后** save，它就会把凭据写回旧的那份。用户看到的后果是：底栏 toast 说「OpenAI Codex OAuth 已连接」，但磁盘上躺着旧凭据 —— 之后的额度查询失败、或者要求重新登录，而用户刚刚才登录过。UI 侧的探针代际（`oauth_seq`）只丢弃了**过期的读数**，磁盘那一半没人管。

**问题二 · Codex 刷新过的 token，apim 用不上（`TODO-2`）。**
按 `x` 把凭据导入 Codex 官方路之后，Codex 自己会刷新 access token 并**只**写回 `~/.codex/auth.json`。apim 那份 `~/.config/apim/openai-oauth.json` 就此落后：额度查询要用过期的 access token 自己再刷一次（多一次网络往返），服务端一旦轮换 refresh token，apim 那份就彻底失效、只能按 `o` 重新授权。README 双语里那句「apim 不回写同步」今天就是这条现状的说明。

**顺带暴露的测试缺口（`TODO-13`）**：`refresh` 的档位门（只有非 Codex 档位带 `resource`）与「先把新 token 落盘、再推导 account id」的落盘顺序，都只有读代码验证 —— 改回去不会有测试变红。而本轮要动的正是这段代码，所以先把它钉住（prefactor：让改动变容易，再改）。

## Solution

**一、写入闸（写者互不覆盖）。** 把凭据文件分成两类写者：**登录**是用户发起的权威写（无条件写，它就是「现在这份」的定义）；**一切从已加载快照派生出来的写**（刷新、以及本次新增的回写）必须带「我读到的那份」的凭据身份，写盘时在既有写锁内**重读磁盘**，只有身份没变才写 —— 变了就跳过，并把「跳过」如实告诉调用方。凭据被删除（`apim auth openai logout`）时同理：在途刷新不会把它写回来。

**二、回写（Codex → apim，单向）。** 在既有的刷新节奏上读一次 `~/.codex/auth.json`，**只在能证明那份凭据就是我们写进去的同一份**时，把更新的 token 采纳进 apim 自己的凭据。**apim 永远不写 `auth.json`。**

两件事的用户可见结果：登录说成功就真的成功（磁盘上留的是新凭据）；Codex 刷新过之后 apim 自动跟上，不必重新授权。

## User Stories

1. As apim 用户, I want 登录成功后磁盘上留的是我刚登进去的那份凭据, so that 底栏说「已连接」不是谎话。
2. As apim 用户, I want 在途的额度探针不会覆盖我刚登录的凭据, so that 我不必在登录后立刻又登一次。
3. As apim 用户, I want 额度查询在我刚登录后就能用, so that 「已连接」之后马上能看到用量，而不是等下个 5 分钟。
4. As apim 用户, I want 我在 `apim auth openai logout` 之后，在途的刷新不会把凭据写回来, so that 退出登录是干净的。
5. As apim 用户, I want Codex 自己刷新过的 token 被 apim 采纳, so that 额度查询不必再自己刷一遍、也不会因为 refresh token 轮换而失效。
6. As apim 用户, I want 回写只发生在我自己的那份凭据上, so that 我另一个账号的状态**绝不会**被抄进 apim。
7. As apim 用户, I want 当 `auth.json` 里的东西不是 apim 写的那份时，apim 一个字都不动, so that 我自己 `codex login` 的结果不受影响。
8. As apim 用户, I want 回写不会让 apim 的凭据变旧（不采纳更早的 token）, so that 同步是幂等的、不会来回抖。
9. As apim 用户, I want 回写失败/判断不了时保持现状, so that 最坏情况是「没同步」，而不是「抄错了一份」。
10. As apim 用户, I want 凭据文件仍然是 600、且建文件时就是 600, so that 新增的写入路径不会让密钥副本落到全局可读。
11. As apim 用户, I want 不回写时不做任何多余动作（不弹 toast、不刷屏）, so that 同步是透明的。
12. As apim 用户, I want 回写与登录可以同时发生而结果仍然正确, so that 两个写者谁先谁后都不需要我操心。
13. As 维护者, I want 登录与派生写者的区别写在代码的一个地方, so that 以后加第三个写者时不用重新推导一遍规则。
14. As 维护者, I want 「我读到的那份」这个身份是可比较的数据, so that 「能不能写」是纯函数、可表驱动测试。
15. As 维护者, I want 判定规则（ownership / 更新与否 / 该采纳哪些字段）是纯函数, so that 正确性由测试守，而不是靠读 HTTP 代码。
16. As 维护者, I want `refresh` 的档位门与落盘顺序有测试守着（`TODO-13`）, so that 本轮改动不会悄悄改坏刷新。
17. As 维护者, I want 不新增全局状态, so that 「谁赢」这件事由磁盘内容推导，而不是靠一个隐藏的计数器。
18. As 维护者, I want 回写的判定与 IO 分层（Codex 侧只解析 `auth.json`；apim 侧只认自己的凭据）, so that 客户端契约留在客户端适配里，凭据存储留在凭据模块里（`ADR-0002`）。
19. As 维护者, I want README 双语里「apim 不回写」那句话在本轮改掉, so that 文档不再描述一个已经不成立的行为。
20. As 维护者, I want `TODO-2` / `TODO-12`（以及 `TODO-13`）做完就从台账里删掉, so that 台账不积累已完成的条目。
21. As 评审 agent, I want 每个写路径都有对应的沙盒测试, so that 我能按 diff 核对该写的地方写了、不该写的地方没写。
22. As 评审 agent, I want 明确的「不测什么」清单, so that 我不会把「没有 HTTP mock」当成漏测。

## Implementation Decisions

### 决定 1 · 写入闸：`save` 保持权威，派生写者走新的受限写

- **`save(dir, cred)` 语义不变**（无条件写）。登录的成功落盘继续用它 —— 登录是权威写，它定义「现在这份」。**不改 `save` 的签名**，避免为了一个检查去动所有调用点。
- 新增受限写：带一个 **baseline**（写者开始时从磁盘读到的那份凭据），在既有 `WRITE_LOCK` 内**重读磁盘**并与 baseline 比对身份，只有一致才写，返回一个明确的结果（写了 / 跳过了）。
- **身份 = `client_id` + `refresh_token`**（用户给的口径，也是最小可比较对）。补充两种边界：baseline 是「本来没有凭据」而现在有 → 跳过；baseline 是「有」而现在没有（被 `logout` 删了）→ 跳过（这条顺带把「退出登录 vs 在途刷新」也关掉了）。
- **写完要把 baseline 推进成「我刚写进去的那份」**：`refresh` 在一次调用里写两次盘（先落 token、再补 account id），第二次必须拿第一次的结果当 baseline，否则第二次会被自己跳过、account id 永远补不上。
- **重读磁盘在写锁内、且不跨 await**（沿用既是注释也是约束的「锁只包住写盘本身」）。
- **不新增全局状态**：身份从磁盘内容推导，不引计数器 / epoch / 新的全局标志。

### 决定 2 · 被跳过的派生写者怎么收尾

- 刷新在跳过时**不报错**：它拿到的是一个「本次结果作废」的信号。`fetch_usage` 据此**重读磁盘并整体重试一次**（新的一份通常就是登录刚写进去的那份，重试会成功）。上界写死为 1 次尝试 + 1 次重试。
- 二次跳过就按「这一轮没有读数」处理（不弹错误），下一个节奏点自然会读到新凭据。
- **不选**「登录在途时不派发 OAuth 探针」这个方案，理由：它拦不住**已经在途**的那一个（它已经过了 `load`，POST 也在飞），所以它单独用仍然会复现原故障；而且登录窗口最长 10 分钟，期间完全不派探针会让用量面板整段时间停在旧读数。写入闸是在正确的层（写盘那一步）解决问题。

### 决定 3 · 回写：ownership 是硬门，且必须 fail closed

- **读哪份**：`~/.codex/auth.json` 的**现场内容**（不是 `.apim.bak` 备份 —— 那是导入前的状态）。
- **只看 ChatGPT 登录**：`auth_mode == "chatgpt"` 且 `tokens` 里 id/access/refresh 齐全（既有 `auth_has_chatgpt_tokens` 同口径）。API Key 登录、`cli_auth_credentials_store = keyring|ephemeral` 导致文件缺失或不合形 → 静默跳过。
- **ownership（硬门，用户的红线）**：`auth.json` 里的 **refresh token 必须逐个字符等于** apim 自己那份的 refresh token。不吻合就**一个字都不写**（这是「用户自己 `codex login` 了另一个账号」的情形）。
- **更新判断**：只在收到的 access token 的 `exp` 声明**晚于** apim 当前的 `expires_at` 时才采纳。解析不出 `exp` → 跳过（fail closed，绝不替它编一个有效期）。这条同时让回写幂等：每个节奏点重跑都不会写盘、不会来回抖。
- **采纳哪些字段**：只采纳 `access_token` / `refresh_token` / `id_token` / 派生出的 `expires_at`；`client_id` / `host_id` / `subject` / `email` / `scopes` / `codex_family` 一律保留 apim 自己那份（它们是 apim 自己的注册身份，`auth.json` 也不带这些）。`account_id` 按既有 `refresh` 的同一套推导口径从新 access token 里取。
- **写盘用决定 1 的受限写**（baseline = 回写开始时读到的 apim 凭据），这样「回写 vs 登录」也是安全的：谁先落盘谁赢，登录永远赢（它是权威写）。
- **已知限制（有意接受）**：如果 OpenAI 真的轮换了 refresh token（服务端通常不轮换），ownership 会因为「对不上」而**拒绝回写** —— 结果是退回今天的行为（apim 那份落后 → 重新授权）。这是 fail closed 的代价，写进 Further Notes，不在这里悄悄放宽判定。

### 决定 4 · 分层与落点（`ADR-0002`）

- **`clients/codex`（客户端契约）**：新增一个「读回现场登录」的只读入口，输出一个自己的小结构（三个 token + account id）。解析 `auth.json` 形状的知识留在客户端适配里。
- **凭据模块**：新增「采纳判定」的纯函数（输入是 apim 自己那份 + 收到的 token 快照 + 收到的 access token 有效期，输出是「拒绝 / 跳过 / 采纳这份新凭据」）与受限写。**不认 Codex 的 JSON 形状**（由上面的小结构隔开，避免跨模块类型依赖）。
- **`app`（节奏）**：挂在既有的 `refresh_active_keys` 上（启动一次 + 每 `AUTO_REFRESH_INTERVAL` 一次 + `r` 手动一次）—— 那里本来就已经在读 `~/.codex`（`codex_route`）与各客户端现场。顺序是**先回写、再探活**，这样紧跟着的那次额度查询用的就是刚采纳的 access token。
- 同一次节奏里 `auth.json` 会被读两次（`codex_route` 一次、回写一次）。**接受**这个小重复：文件很小、本来就是这个同步函数里在读，为省它去重构 `codex_route` 不划算（与 `ADR-0007` 容忍同类小重复同旨）。
- **不碰**那 6 处 `current_provider_id() == Some("openai")` 的门、不碰 `recipe`、不碰 `AUTH` 行的位置与交互、不改 `codex_route` 的显示口径（`ADR-0008`：只有一份凭据，不存在「这是谁的凭据」的对账问题）。
- `--snapshot` 会走到同一个节奏点，因此快照也会（幂等地）回写 —— 与 `ADR-0006` 第 1 条「`--snapshot` 做真实读取」的口径一致。
- **prefactor（可选，默认纳入）**：先给 `refresh` 的档位门与落盘顺序加测试守卫（`TODO-13` 的「第一步」：把请求表单抽成纯函数再表驱动测；把「先落盘再推导」的编排做成可注入的形态）。**不要也可以**：本轮不依赖它，只是没有兜底测试。

### 决定 5 · 红线（原样继承）

- 含密钥的文件一律 **600，且建文件时就是 600**：新增/改动的写路径**只能**经既有 `config::write_private`（tmp + rename + 建文件即 0600），不得出现 `fs::write` + `chmod`。
- **不新增全局状态**。
- 凭据仍是单份（`openai-oauth.json` + 伴生 host-id）、仍只服务内置 `openai`、仍禁止复制内置 `openai`（`ADR-0008`）。
- **`~/.codex/auth.json` 只读**：本特性是单向的 codex → apim。

## Testing Decisions

**什么算好测试**：只断言**外部可观察的东西** —— 磁盘上的字节、返回的判定、文件的权限；从不断言内部调用顺序或私有函数结构。判定逻辑一律走「纯函数 + 表驱动断言」，IO 只做薄薄一层壳。

**新增的纯函数（都按既有先例写：`next_refresh_token` / `window_label` / `reusable_registration` 那种小函数 + 表驱动测试）**：

- `refresh` 的请求表单：给定凭据（Codex 档位 / 非 Codex 档位）→ 期望的表单键值（含「只有非 Codex 档位带 `resource`」）。
- 受限写的判定：`(baseline, 当前磁盘内容) → 写 / 跳过`，覆盖「一致」「换了凭据」「本来没有现在有」「本来有现在没有」四行。
- 回写判定：`(apim 那份, 收到的 token 快照, 收到的 exp) → 拒绝 / 跳过 / 采纳`，表里至少要有：同账号且更新 → 采纳；refresh token 不吻合（另一个账号）→ 拒绝；token 更旧或相同 → 跳过；token 不全 → 拒绝；`exp` 缺失 → 拒绝；采纳后哪些字段保持原样（逐字段断言）。
- `auth.json` 解析：给合成 JSON 字符串 → 期望的结构或 `None`（含 API Key 登录、缺 token、空文件）。

**沙盒测试**（都用临时目录，绝不碰真实 `~/.config/apim` 与真实 `~/.codex`）：

- 既有先例：凭据模块自己的 `dir(name)`（`target/oauth-<name>-<pid>`）与 `credential_file_mode_is_private`；客户端侧的临时 home + 合成 `auth.json`（口径同 `codex_real_official_end_to_end`：**合成凭据，不读真实 token**）。
- 端到端一条：在临时 `CODEX_HOME` 放一份合成的「更新过的」`auth.json`，在临时配置目录放一份对应的 apim 凭据 → 调用那个节奏点 → 断言 apim 的文件被更新、内容逐字段正确、权限仍是 600；再放一份「另一个账号」的 `auth.json` → 断言 apim 的文件**逐字节未变**。
- 写闸一条：模拟「登录先落盘」再让一个拿旧 baseline 的派生写者去写 → 断言文件仍是登录写的那份；以及「凭据被删」后派生写者不复活它。

**不测什么（明说，免得被当成漏测）**：

- **不引 HTTP mock server**：刷新的 POST 与用量 GET 都不进自动化测试（`TODO-13` 已经记过这个代价）。因此「跳过 → 重读重试一次」这条编排路径由**代码审查 + 上界写死为 1 次**保证，不由测试覆盖。
- 不测真实网络、不使用真实凭据；不测并发时序本身（把它化成上面那条纯判定规则）。
- 不测 `codex_route` / 额度面板的显示（本特性不改它们）。

**每条测试的验收**：`cargo test` 绿；`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all` 零警告；改动过 UI 无关的东西，所以除 `cargo run -- --snapshot` 的既有口径外不需要新快照。

## Out of Scope

- **`TODO-14`（超线文件拆分）**：`openai_auth.rs` 已经超过「单文件 ≤ ~300 行」。本特性会往里加代码，但**不拆**（另开一轮）；只要求新增代码按决定 4 的缝放（纯判定与 IO 分开）。
- **`TODO-1` 的全部内容**：不改凭据存储布局、不加 `recipe` 能力字段、不动那 6 处 id 门、不放开内置 `openai` 的复制（`ADR-0008` 已是终局）。
- **`ADR-0006` 的四个有意取舍**：不「顺手修」它们（快照的真实读取、`USAGE_LOCK` 串行化、AUTH 行的显示优先级、非缺省档位的 URL 提示）。
- **UI/交互**：不加 toast、不加确认、不改 AUTH 行与额度面板的显示、不改 `x` 的语义。
- **`auth.json` 的写入**：一字不改（含 `last_refresh`、`OPENAI_API_KEY`、`cli_auth_credentials_store` 的处理）。
- **`TODO-13` 的其余部分**（真守死要引 HTTP mock）不在此轮。
- **回写的反向路径**（apim 刷新后写回 `auth.json`）：不做，也不该有。

## Further Notes

- **已知限制（决定 3 的代价，别当 bug 修）**：服务端轮换 refresh token 时，ownership 会对不上，于是**拒绝回写**、退回今天的行为（apim 那份落后 → 按 `o` 重新授权）。重开条件：真的观察到轮换造成「每次都要重新登录」时，再考虑把 ownership 放宽成「账号身份相同（`sub` / `account_id`）+ 收到的 token 确实更新」这套更宽松（但仍然是同账号）的判定 —— 那需要新的 ADR，因为它是「凭据来源」这条口径的变更。
- **与 `ADR-0007` 的关系**：本次新增的「同一个节奏里读两次 `auth.json`」属于 `ADR-0007` 容忍的那类**代码**小重复（它给的动手触发条件是「出现第四处同类重复，或其中一处开始漂移」）。本 spec 明确把它记在这里，避免将来被误当成新发现。
- **与 `ADR-0006` 的关系**：`USAGE_LOCK` 保持不动 —— 它解决的是「两个用量查询同时刷新」，本特性解决的是「登录与派生写者互相覆盖」，两者是互补的，不是重复。
- **prefactor 是可裁的**：决定 4 最后那条（`TODO-13` 的测试守卫）默认纳入；若不要，删掉对应票并去掉「竞态票」对它的依赖即可 —— 竞态票不依赖它也能做，只是少了兜底测试，且 `TODO-13` 保持原样。
- **为什么两件事同一轮**：它们改的是同一个函数链（`load → refresh → save`）与同一个文件。分开做会让第二件再动一遍刚改好的落盘路径，且 ownership 判定本身要用到第一件引入的受限写。
- **收尾**：做完后 `docs/TODO.md` 删掉 `TODO-2` / `TODO-12`（与 `TODO-13`，若纳入 prefactor），`CHANGELOG.md` 记一笔。
