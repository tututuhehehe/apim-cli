# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与
[语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.9] - 2026-10-09

### 修复

- **登录成功后的凭据不会再被在途的额度查询覆盖**：以前可能出现「底栏说已连接、实际却查不到用量」——
  一次在途的额度查询会把刚登进去的那份凭据写回旧的，于是要么查不到、要么又要重新授权。现在两者并发时
  以登录为准。

### 新增

- **Codex 自己刷新过的 token 会被 apim 采纳**：按 `x` 把凭据导入 Codex 官方路之后，Codex 续期时只把新
  token 写回 `~/.codex/auth.json`，apim 现在会在自己的刷新节奏里读回来，于是额度查询一直能用、不必再按
  `o` 重新授权。只有能证明那份是 apim 自己写进去的（refresh token 吻合）才采纳 —— 你自己 `codex login`
  的另一个账号绝不会被抄进来。

### 安全

- 升级 `rustls` 0.23.43 → 0.23.45（修 Dependabot 报的 medium 告警；`rustls` 是 `reqwest` 的间接依赖，只动 `Cargo.lock`）。

## [0.1.8] - 2026-10-09

### 新增

- **AUTH 行按 `x` 一键把 OpenAI Codex OAuth 导入 Codex 的官方路**：让 Codex 自己用你的 ChatGPT 订阅，
  而不是走中转站 —— 与 cc-switch 的「OpenAI Official」卡同一件事。写 `~/.codex/auth.json`（Codex 原生
  ChatGPT 登录形状：`auth_mode` + `tokens{id_token,access_token,refresh_token,account_id}` + `last_refresh`；
  `refresh_token` 必带，codex 自己拿它续期），config.toml 只摘顶层 `model_provider` / `model` 与 apim 自己
  写的 `model_catalog_json`，其余（`notify`、`[projects.*]`、`[tui]`、`[plugins.*]`、其它 `[model_providers.*]`
  块、注释与顺序）一个字不动 —— cc-switch 是整份清空，它有 provider 数据库兜底，apim 没有。
  CLI：`apim auth openai import-codex`。
- 密钥表里的 `AUTH` 行变成**可选中行**（只在内置 `openai` 分页，排在密钥行之后）：在它上面按 `x` 走上面
  那条官方路导入，`c`/`i`/`d`/`m`/`e` 会明说「只支持 x」而不是假装没有密钥。
- 导入前先拒掉 codex 读不到的凭据：`cli_auth_credentials_store = keyring|ephemeral` 时直接报错并说明原因
  （apim 不写系统钥匙串）；导入后让 **codex 自己**校验（`codex login status` 必须认到
  `Logged in using ChatGPT`），失败就把 `auth.json` 与 `config.toml` 两处都还原，成功则照旧重启 codex 的
  app-server 守护进程（`APIM_NO_RESTART_CODEX=1` 可关）。
- 额度面板的 AUTH 区多一行「Codex：官方 OAuth / 官方 API Key / provider x / 未登录」：与 ★ 角标同一个
  思路，**回读 `~/.codex` 现场**算出来，不存台账。
- **内置 `openai` 不可复制**（TUI `y` 与 CLI `provider copy` 都拒绝，消息里指向 `provider add`）：它的
  OAuth 凭据是全局一份，复制出来的 `openai-copy` 会是一个「看着像 OpenAI、却永远登录不上、也没有 AUTH 行」
  的厂商。想加第二个 OpenAI 兼容厂商就用 `provider add` 建新 id（本来就不带 OAuth）。

### 变更

- `apim uninstall` 多报一项 apim 自己的备份 `~/.codex/auth.json.apim.bak`（里面是上一份 ChatGPT 登录
  凭据），手工清理清单同步；`auth.json` 本身仍不报 —— 那是 codex 自己的登录文件，归 `codex login/logout` 管。
- AUTH 行选中时的底栏提示只列真能用的键（去掉了 `c 复制`）。

### 修复

- 官方路导入失败时，`auth.json` 还原失败不再短路掉 `config.toml` 的还原（原来可能留下「apim 说导入失败、
  codex 配置其实已经被摘掉第三方路由」的状态）。
- 校验口径收紧：codex 的 API Key 登录同样是 exit 0 的 `Logged in using an API key - sk-***`，只匹配
  `Logged in` 会把「官方 OAuth 已导入」说成假话，现在必须认到 `Logged in using ChatGPT`。

### 内部

- 两轮子代理评审（5 个 fresh 上下文：正确性/回归、测试与凭据安全、命令行反证）后按 P2 回修 7 处并补强测试
  （空 openai 面板的 AUTH 高亮、`clamp_selections` 保留 AUTH 选中位、auth.json 四个 token 字段一个都不能少、
  真机 opt-in 测试的目录指针断言按输入判）；`docs/TODO.md` 记下 5 条报告级小账与残留风险。

## [0.1.7] - 2026-10-05

### 新增

- **OpenAI Codex OAuth 用量**：内置 OpenAI 厂商的密钥栏按 `o`（或 `apim auth openai login`）在浏览器里
  授权，额度面板就会在 API Key 额度之外**并列**显示 ChatGPT/Codex 的订阅用量：接口返回的每个限额窗口
  各占一行、标签按窗口真实长度算（5h / 1w / 1mo）、带重置倒计时，另有套餐与 credits。密钥表里多一条固定的
  `AUTH` 行显示状态（未配置 / 查询中 / 已连接 / 失败）；它**不是**普通密钥，不能复制、编辑、删除，`x` 也不导入。
  CLI：`apim auth openai login|status|logout`。
- 凭据由 apim 自己保管：`~/.config/apim/openai-oauth.json`（600 权限），与 Pi 的 `auth.json` 再无关系；
  登录走 Codex CLI 客户端配置（与 Pi / cc-switch 同款，`localhost` 回调），
  `APIM_OAUTH_CLIENT=apim` 可切到 OpenAI 的开源动态注册档位。
- 登录失败可诊断：每一步（含 token 端点状态码与已遮罩的响应体，绝不含 token）写进
  `~/.config/apim/openai-oauth.log`，底栏 toast 先说原因、后跟日志路径。

### 变更

- 额度窗口标签不再假设「一定有 5h + 7d」：按接口返回的窗口长度算（`go` 套餐只有一个按月窗口就只显示一条）。

### 修复

- `jsonwebtoken` 升到 10.4.0（9.x 的 CVE-2026-25537：`nbf`/`exp` 类型混淆可能绕过时间校验）。
  apim 的用法本不受影响（没开 `validate_nbf`、`exp` 仍在默认必检集合里，且自己再查一遍），
  仍一并升级；`default-features = false` 保持不变，不引入 `pem` / `simple_asn1` / `time`。
- 刷新 token 时不再给 Codex CLI 档位多带 `resource`（与登录表单同口径）；服务端已轮换的 refresh token
  一定先落盘，不会再被后续校验失败丢掉。
- 刷新响应没有新 refresh token 时沿用旧的（RFC 6749 §6 允许不轮换），不再让凭据到期即死。
- `apim auth openai <未知子命令>` 改为非零退出；`o` 在非内置 OpenAI 处给出提示而不是静默无效。

### 内部

- AUTH 状态收敛到 `ui::oauth_state` 单一来源，密钥表与额度面板不会再各说各话；凭据与诊断日志写入串行化。

## [0.1.6] - 2026-10-03

### 新增

- **非模型厂商（翻译 / 搜索这类 API 也能进 apim 了）**：添加厂商时勾上「非模型」，或者在 CLI 用
  `apim provider add --kind non-model`。非模型厂商没有模型列表（`m`）、不能一键导入客户端（`x`）、
  也不探活；密钥、别名 / 分组、主页 `⏎`、`c` 复制、`y` 整份复制、`^Z`、额度脚本、`provider ls`、
  `apim status` 全都一样。类型写进 recipe（`kind: model|non_model`，缺省模型），
  **只能在创建时定**：编辑表单里它只读、`provider set --kind` 被拒绝，要换类型就 `provider copy` 或删了重建。
  这类厂商的接入点是**额度脚本**（env 注入 `APIM_TOKEN` / `APIM_BASE_URL` / `APIM_ALIAS` / `APIM_VAR_*`，
  stdout 逐行上面板）——调哪个端点、key 怎么传，全在脚本里
- **左栏分页：`Tab` 切「模型 / 非模型」两页**：两页各自记住选中厂商与过滤词，切页不串味；
  `Tab` 不再是「切左右栏焦点」（那是 `h`/`l` 与 `←`/`→` 的活）。非模型厂商的
  「状态」口径是额度脚本的成败（密钥表、左栏摘要、`apim status` 共用一处）

### 变更

- **非模型下不出现只对模型 API 有意义的行**：添加表单勾上「非模型」时「探活路径」整行消失
  （不是灰掉：表单引擎新增「勾选框隐藏某一行」——不渲染、不占弹窗高度、Tab 跳过、也写不进去；
  取消勾选那一行回来、填过的值不丢），编辑表单与 `i` 详情弹窗里也没有这一行。
  详情弹窗同时去掉「类型」那种背景行，非模型也不再列「鉴权」（与探活一样只服务 HTTP 请求）
- `auth` 在 recipe 里可以省略（它只服务 HTTP 请求，非模型一个请求也不发）；**手改 `kind: non_model` 后残留的 `health:` 也不会再发探活请求**
  （UI/CLI 都不让改类型，手改文件就是现实中的「换类型」，所以这条必须真的成立）：
  探活与模型列表的消费点一律走 `Recipe::health_call()`，`provider ls --json` / `apim status --json`
  对非模型报 `null`；`provider ls` 的人类输出与 `--json` 带类型，`apim status --json` 也带 `kind`
- AI 代写额度脚本的提示词（`docs/quota-script-prompt.md`）新增 §1.5：非模型厂商怎么写 recipe、
  怎么注册（不写 `health`、不传 `--health`），以及「脚本的成败就是这类厂商的状态」

## [0.1.5] - 2026-10-02

### 修复

- **`apim update`（npm 渠道）不再谎报「更新完成」**：更新前同时核对 `apim-cli` 与**当前平台子包**的版本，
  npm 还没跟上 GitHub Release 时报出两个版本号并拒绝安装（`--force` 可越过）；`--check` 多打一行
  「npm 上可装：…」。之前只跑 `npm install -g apim-cli@latest`：npm 还没发出来时等于把旧版本重装一遍，
  主包先可见而平台子包还没可见时更糟 —— npm 对 optional 依赖失败是**静默跳过**，
  装出来的 shim 直接报 `no prebuilt binary available for <platform>`
- **npm 发布链路堵住上面那个窗口**：`scripts/publish-npm.mjs` 每个平台子包发布后轮询到它真的可见，
  最后才发主包；全部发完再整体核对 6 个包，缺一个就非零退出。`release.yml` 转正 Release 后显式
  dispatch `publish-npm.yml`（用 GITHUB_TOKEN 转正产生的 release 事件不会再触发 workflow 运行），
  workflow 的幂等检查也改成「6 个包都在才跳过」（少一个继续发，可修复半发状态）

## [0.1.4] - 2026-10-02

### 新增

- **一键导入到 Pi**（`x` 键选客户端 Pi）：只把密钥 / 厂商 / 勾选的模型写进 `~/.pi/agent/models.json`
  （`providers.apim-<厂商id>`：OpenAI 兼容端点 + apiKey + 模型）—— **`settings.json` 一个字都不动**
  （默认 provider / 默认模型 / `enabledModels` 由用户自己在 `/model` 里选），所以 Pi 没有
  「选默认模型」这一步；写完跑 `pi --list-models` 让 pi 自己确认模型都在，失败用备份还原 models.json。provider 键一律带 `apim-` 前缀（否则会蹭到 pi 内置 provider 的 baseUrl）；
  只改 apim 负责的键，用户手写的 `headers` / `compat` / `modelOverrides` 与别的设置都保留；
  不需要重启（打开 `/model` 即可）；`PI_CODING_AGENT_DIR` / `APIM_PI_BIN` 可重定向；`apim uninstall` 会一并
  列出 Pi 那边的残留（`providers.apim-*` 条目与含明文密钥的 `models.json.apim.bak`）

### 变更

- **npm 分发改为跟随 Release 自动发布**：`publish-npm.yml` 增加 `release: [published]` 触发
  （版本号取自 tag），Release 一转正就把 6 个 npm 包发出去；`workflow_dispatch` 保留给重跑/补发。
  以前必须手动点一次 Run workflow ——npm 侧的 Trusted Publisher 只是免 token，不会自己跑
- `Cargo.toml` 的 `rust-version` 从 1.85 修正为 **1.88**（代码里用了 let-chain，1.88 才稳定；原来的声明是错的），
  CI 新增一个 **msrv job** 用声明的版本真跑一遍 `cargo check --locked --all-targets`，避免以后又漂回去
- **一键导入面板瘦身**：选客户端那屏只留一行一个客户端名（不再展开配置路径与现状说明），
  并删掉「将导入：xxx（apim 上次导入的是 yyy）」那句
- **Pi 侧的 ★ 判定改成「只认 key」+ 多读一份 `auth.json`**：不再要求 provider 键带 `apim-` 前缀
  （前缀只约束写），改成扫 **pi 配置里的每一份凭据** —— `auth.json`（`/login` 存的，只读）
  与 `models.json` 里每个带 `apiKey` 的 provider，与 apim 的密钥比 token（读不到明文才退化比
  `base_url`）。pi 没有「唯一激活的 provider」（`defaultProvider` 只是启动默认），所以
  `auth.json` 里登过的 `opencode-go` 这种也会被认出来；`type:"oauth"` 的订阅凭据不算。
  **apim 不写 `auth.json`**（那里面还有订阅凭据，且 pi 自己加锁管、逐条校验）
- **密钥行 ★ 改成回读客户端现场**：不再写 `~/.config/apim/codex.toml` 台账，而是重算时点
  （启动 / 切厂商 `j`/`k` / `r` 刷新 / 5 分钟自动刷新 / 导入成功后）读一遍 codex 的 `config.toml`
  （顶层 `model_provider` → `[model_providers.<id>]` 的 `experimental_bearer_token`，或只有
  `env_key` 时用 `base_url`）与 apim 的密钥对账 —— 手改了客户端配置，★ 会跟着消失，不再留在旧密钥上。
  多个客户端都用同一把时叠成角标（`★C`、`★C,P`）

### 移除

- `~/.config/apim/codex.toml`（apim 侧的「上次导入了谁」记录）—— 升级后可以手删；
  `ImportRequest` 不再需要 `alias`

## [0.1.3] - 2026-10-01

### 新增

- **`apim uninstall`**：认出安装渠道（npm / Homebrew / install.sh 裸二进制）后从**原渠道**卸掉自己 ——
  npm 走 `npm uninstall -g apim-cli`、Homebrew 走 `brew uninstall apim`、裸二进制直接删
  （软链安装会把链一路删干净，链上不叫 `apim` 的真身不碰）。默认**只删程序、保留配置与密钥**；
  `--purge` 才删 `~/.config/apim`（目录名不是 `apim`、或目录本身是软链时一律拒绝），
  `--dry-run` 只报告要动的每一个路径，`--yes` 跳过确认，`--json` 给脚本用。
  `target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本一律拒绝卸载（与 `apim update` 保持一致）

### 变更

- `apim update` 与 `apim uninstall` 共用同一套渠道识别与「跑外部命令」实现
  （`Channel` / `detect_channel` / `run_tool`）—— 以后加渠道，卸载动作会被穷尽 `match` 拦下

## [0.1.2] - 2026-10-01

### 新增

- **一键导入到 Codex**（密钥表选中一把密钥按 `x`）：三步面板（选客户端 → 勾选模型 → 选默认模型）
  → 把密钥 / 厂商 / 勾选的模型写进 `~/.codex/config.toml`（`model_provider`/`model`/`model_reasoning_effort`/
  `model_catalog_json`）+ 生成 `~/.codex/apim-models.json`（每个模型带 medium/high/xhigh/max 四档思考等级），
  写完让 **codex 自己解析校验**，成功后在密钥行打 ★
- **`apim update`**：识别安装渠道（npm / Homebrew / install.sh）后从原渠道更新自己；
  `--check` 只看当前/最新版本，`--force` 版本相同时也重装，`--json` 给脚本用
- 客户端适配层 `src/clients/`：`Agent` 抽象 + `clients/codex/`。加一个新客户端 = 加变体 + 子模块 + 一条分派，
  面板不用改

### 变更

- 一键导入成功后**自动重启 codex 的 app-server 守护进程**（codex 只在进程启动时读一次模型目录；
  `APIM_NO_RESTART_CODEX=1` 可关掉自动重启）
- install.sh 更新渠道**不再 `curl | sh`**：URL 钉到目标 tag → 进程内下载到临时文件 → 形状校验 →
  按 Release 发布的 `install.sh.sha256` 校验摘要（拿不到就拒绝执行）→ 才交给 `sh` 执行
- `~/.codex/config.toml`、它的备份、以及 `secrets.toml` 一律 **0600，且建文件时就是 600**
- 导入面板与 `m` 键列出**完全一致**的模型列表（不再按厂商声明的端点能力筛掉实际可用的模型）

### 修复

- 备份文件曾按 umask 落成 0644，而备份里有 `experimental_bearer_token` → 现在强制 600
- 导入校验失败时用备份**回滚**两处改动，不再出现「提示导入失败、配置其实已切换」
- 重启 codex 的进程匹配收紧到「子命令位正好是 `app-server`」，避免误杀 `codex --profile app-server` 这类用户会话；
  包装脚本形态只如实报告，不再静默跳过
- 并发导入加排他锁（`.apim-import.lock`，崩溃残留可被认领），不再互相抹掉 provider 块
- 导入结果按**请求代际**丢弃过期回执 / 旧模型列表
- 补写顶层 `model_reasoning_effort`；模型目录条目改成厂商官方文档那种迷你条目（64KB/模型 → ~1.4KB/模型）
- `config.toml` 是符号链接（dotfiles 管理）时跟随写入而不是替换链接；备份始终留在 `~/.codex/`


## [0.1.1] - 2026-09-28

### 新增

- `install.sh` 一行安装脚本（macOS/Linux）：识别平台 → 下载预编译二进制 → 校验 sha256；
  支持 `APIM_VERSION` 锁版本、`APIM_INSTALL_DIR` 指定目录
- npm 分发：`npm install -g apim-cli` / `npx apim-cli`（主包 + 5 个平台子包）
- Homebrew：`brew install tututuhehehe/tap/apim`
- `cargo binstall apim` 的元数据（crate 发布到 crates.io 后可用）
- `docs/RELEASING.md`：维护者发布手册

### 修复

- 测试不再依赖真实剪贴板（无显示环境如 Linux CI 会失败）
- release workflow 的发布步骤补 `--repo`（该 job 未 checkout 时会报 not a git repository）

### 变更

- npm Windows 子包名 `win32` → `windows`（`win32` 会触发 npm 名称风控 spam detection）

## [0.1.0] - 2026-09-28

首个公开版本。

### 新增

- 双栏 TUI：左侧厂商列表（带实时额度与延迟）、右侧密钥表、右下额度面板
- 厂商与密钥的增删改查、焦点感知复制、详情检查器（token 按需显隐）
- 并发探活 + 脚本化额度查询（`balance.kind: script`，密钥经 env 注入，超时 kill）
- 厂商整份复制（recipe + 额度脚本文件独立复制）
- 会话内 `Ctrl+Z` 撤销（内存与磁盘一起回退）
- CLI 子命令：`provider` / `key` / `status` / `copy` / `use`，token 只走 stdin
- `m` 键用选中的 key 拉取模型列表
- 内置 4 家 recipe：DeepSeek / OpenAI / Moonshot AI / OpenRouter
- `apim --version` 版本输出（供安装脚本与更新检测使用）
- MIT 开源协议

[Unreleased]: https://github.com/tututuhehehe/apim-cli/compare/v0.1.9...HEAD
[0.1.9]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.9
[0.1.8]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.8
[0.1.7]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.7
[0.1.6]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.6
[0.1.5]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.5
[0.1.4]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.4
[0.1.3]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.3
[0.1.2]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.2
[0.1.1]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.1
[0.1.0]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.0
