# AGENTS.md — apim 项目说明

## 项目背景

终端里的模型厂商 API Key 管理器（TUI + 未来的 CLI）。用户维护多家厂商 / 中转站的 API Key（DeepSeek 官方、各类 NewAPI 中转站等），痛点是 Key 复制来复制去、查额度要开网页。目标：本地一个二进制管全部 Key——增删改查、复制、探活、查额度；后续以 CLI 子命令接入 Alfred Workflow 作为统一入口。

## 技术选型（已定，勿改）

- **Rust**（用户本机有 rustc，无 Go）。产物是单二进制，Alfred 调用秒开。
- **可扩展性靠数据不靠代码（范围：厂商协议）**：厂商协议 = YAML recipe（怎么鉴权、怎么探活、怎么查额度、怎么展示）。加一个中转站 = 丢一个 YAML，不重新编译。**这条只管「同一套协议内的厂商差异」**；跨协议的差异（声明式 http 已退役）与**客户端差异**各写 Rust 子模块，不做 DSL。
- TUI 用 ratatui；HTTP 用 reqwest(rustls)；密钥存 TOML，不进仓库。

## 目录结构（按功能单元拆分，单文件 ≤ ~300 行）

```
src/
├── main.rs            入口 + 参数分发（无参数=TUI；--snapshot* 渲染快照；其余走 CLI）
├── tui.rs             事件循环、按键路由（厂商栏 Enter 开主页）、快照渲染
├── clipboard.rs       复制到剪贴板（arboard → pbcopy 兜底）
├── browser.rs         用默认浏览器打开厂商主页（open/xdg-open，只放行 http(s)）
├── clients/           一键导入到外部客户端（agent）
│   ├── mod.rs         Agent 注册表 + 分派（加客户端：加变体、加子模块、补这个 match）
│   ├── lock.rs        导入期间对客户端配置目录的排他锁（codex/pi 共用）
│   ├── file_io.rs     客户端配置文件公共落盘件：跟随符号链接 + 写前备份 + 原子 600
│   ├── codex/         Codex 适配（mod.rs 只是门面：子模块声明 + 再导出 + codex_home/config_hint）
│   │   ├── import.rs  写入编排 + 回滚（保留名加前缀、base_url 补 /v1）
│   │   ├── official.rs 官方路（ChatGPT 登录）：写 ~/.codex/auth.json + 摘掉 config.toml 的第三方路由（AUTH 行按 x）
│   │   ├── restart.rs RestartReport、按进程表找 codex daemon 并重启（严格匹配 argv[2]）
│   │   ├── config_file.rs  ~/.codex/config.toml 读改写（toml_edit 保注释保顺序）+ 备份 + 原子写
│   │   ├── catalog.rs 模型目录：手写官方迷你条目（~1KB/模型）+ `codex debug models` 端到端校验
│   │   ├── active.rs  回读 ~/.codex/config.toml 现场：现在在用哪把密钥（★ 角标）
│   │   └── tests/     沙盒测试（按源码文件分）+ 真机 opt-in（`--ignored codex_real_end_to_end`）
│   └── pi/            Pi 适配
│       ├── import.rs  ~/.pi/agent 两份 JSON 的写入编排 + 回滚（provider 键一律 `apim-` 前缀）
│       ├── config.rs  models.json 读写（保留未知字段 + 备份 + 原子 600）；auth.json 只读
│       ├── verify.rs  跑 `pi --list-models` 让 pi 自己确认勾选的模型都在
│       ├── active.rs  回读 auth.json + models.json 的凭据：现在在用哪把密钥（★ 角标）
│       └── tests/     沙盒测试（假 pi 脚本）+ 真机 opt-in（`--ignored pi_real_end_to_end`）
├── cli/               CLI 子命令（AI/脚本的机器接口，与 TUI 共用底层）
│   ├── mod.rs         Args 解析（--flag 值/布尔）、Ctx（config+recipes 目录，可注入测试）、分发与帮助
│   ├── provider.rs    provider ls/add/set/rm/copy（--script 绑定/解绑、--kind 只在 add；copy 整份复制含脚本文件）
│   ├── auth.rs        OpenAI Codex OAuth 登录/状态/退出命令
│   ├── keys.rs        key ls/add/set/rm（token 只走 stdin，不进 argv）
│   ├── query.rs       status（并发探活+额度，--json）/ copy / use
│   ├── uninstall/     apim uninstall：mod.rs 流程 / cleanup.rs 删程序（含软链）与 --purge 配置目录 / report.rs 人机两套输出
│   ├── update/        apim update：channel.rs 认渠道 / install_sh.rs 下载+校验+执行 / http.rs 取 tag
│   └── tests.rs       CLI 沙盒测试（临时目录全流程）
├── form/              通用表单引擎（密钥表单、厂商表单共用）
│   ├── mod.rs         Field（文本/选择/勾选框/只读）、Form、按键分发、表单构造器（PF_* 字段下标）
│   ├── edit.rs        LineEdit：单行编辑（值 + 光标）
│   └── tests.rs       表单引擎测试
├── app/               应用状态机
│   ├── mod.rs         App 结构、分页（Kind::ALL 顺序的 tabs）、start、导航、探活调度（探针代际：配置变更后旧结果丢弃）
│   ├── import/        一键导入面板：mod.rs 流程控制 + flow.rs 状态 + apply.rs 写盘回执 + keys.rs 按键
│   │                  （tests/ 按 flow / apply 分）
│   ├── modal.rs       Modal 枚举 + 打开/保存分发/删除确认分发
│   ├── keys_store.rs  密钥保存/删除（写 config.toml + secrets.toml）
│   ├── undo.rs        Ctrl+Z 撤销栈（本次会话的写操作）+ 回退内存与磁盘
│   └── providers_store.rs  厂商保存/删除/整份复制（recipe YAML + 额度脚本文件）
├── ui/                一个面板一个文件
│   ├── mod.rs         draw 分发 + theme + pane_block/centered + 滚动偏移
│   ├── header.rs      顶栏/底栏（底栏按焦点显示 c 复制什么）
│   ├── providers.rs   左栏分页条（模型/非模型）+ 当前分页的厂商列表
│   ├── keys.rs        右侧密钥表 + 状态标签（含 ★ 角标）
│   ├── balance.rs     右下额度面板
│   ├── inspector.rs   详情弹窗（密钥 / 厂商）
│   ├── models.rs      `m` 键的模型浏览弹窗
│   ├── search.rs      搜索框零件（密钥/厂商/模型共用）
│   ├── form_modal.rs  表单弹窗（光标截断渲染）
│   ├── import.rs      一键导入面板渲染
│   └── confirm.rs     删除确认弹窗
├── openai_auth.rs     OpenAI Codex OAuth：浏览器登录（缺省 Codex CLI 档位，`APIM_OAUTH_CLIENT=apim` 走动态注册）、PKCE、凭据刷新与用量查询
├── util.rs            跨模块小工具（truncate、expand_tilde；只放「多处各写了一遍」的东西）
├── recipe/            厂商协议
│   ├── mod.rs         Recipe/ProviderKind/Auth/HttpCall 模型、YAML 加载（builtin→manifest→user 逐级覆盖，加载期校验 id 字符集）、is_valid_id、{token}/{base_url} 模板替换
│   ├── script.rs      ScriptSpec（balance.kind=script，command/run 二选一，自定义 serde 校验）
│   ├── dup.rs         厂商整份复制（新 id 自动顺延 + 额度脚本文件副本）；内置 openai 不可复制（OAuth 凭据是全局一份，见 docs/TODO.md）
│   └── store.rs       用户 recipe 读写（~/.config/apim/recipes/*.yaml）
├── config/            密钥清单
│   ├── mod.rs         KeyEntry、读取 config.toml + secrets.toml（严格版给 TUI，宽松版 load_keys_lenient 给 CLI 自救）
│   └── store.rs       原子写入（tmp+rename，tmp 名带 pid，600 权限）
└── probe/             并发探活（health + balance 并发，tokio::join!）
    ├── mod.rs         Health/ProbeResult、client、http 一路（探活 + 模型列表拉取的鉴权请求）
    └── script.rs      脚本执行器（env 注入/超时 kill/stderr 截断 200/stdout 50 行上限）+ expand_tilde
docs/
├── quota-script-prompt.md  额度脚本代写提示词（整体复制给 AI Agent 用）
├── TODO.md             维护者待办（已知缺口 + 真要做得动哪些地方）
└── RELEASING.md        维护者发布手册（发版、npm、Homebrew、回滚）
README.md             英文说明（默认，GitHub 首页）
README.zh-CN.md       中文说明（与英文版内容同步）
install.sh            一行安装脚本（macOS/Linux：下载 Release 二进制 + 校验 sha256）
npm/                  npm 分发（平台子包模型）：bin/apim.js 主包 shim
scripts/              publish-npm.mjs（从 Release 资产组装并发布 6 个 npm 包）
.github/workflows/    ci.yml（fmt/clippy/test/JS 检查 + msrv job）、release.yml（tag 发 5 平台二进制）、publish-npm.yml（Release 转正后自动发 npm；也可手动重跑）
recipes/              内置 recipe ×4（deepseek/openai/moonshot/openrouter，include_str! 编译进二进制）
├── deepseek.yaml
├── openai.yaml
├── moonshot.yaml
└── openrouter.yaml
```

## 运行时数据（都在仓库外）

- `~/.config/apim/config.toml` — 密钥清单（provider/alias/group，无 token）
- `~/.config/apim/secrets.toml` — API token，键名 `"厂商.别名"`，600 权限
- `~/.config/apim/openai-oauth.json` — apim 自己的 OpenAI Codex OAuth 凭据（access/refresh/id token），600 权限；`openai-oauth-host-id` 是稳定 host ID；`openai-oauth.log` 是最近一次登录/刷新的逐步诊断（状态码与遮罩后的响应体，不含 token），登录失败时也会把完整错误链写进去；与 Pi 的 `auth.json` 无关
- 浏览器登录缺省复刻 Codex CLI 客户端（Pi / cc-switch 同款：固定公开 client id `app_EMoamEEZ73f0CkXaXp7hrann`、`localhost` 回调、不发 nonce、不申请 `resource`）。**不要改回缺省走动态注册**：实测动态注册签发的 token 里 `https://api.openai.com/auth` 只有 `per_user_salt` + `encrypted_auth_metadata`（无 `chatgpt_account_id`），请求 `backend-api/wham/usage` 一律 401；`APIM_OAUTH_CLIENT=apim` 是显式切回的口子（写错的值会被点名写进诊断日志，不静默）。token 端点按 `/api/accounts/oauth/token` → `/oauth/token` 依次尝试；刷新 token 与登录同口径（Codex CLI 档位不带 `resource`），且服务端不轮换 refresh token 时沿用旧的
- `~/.config/apim/recipes/*.yaml` — 用户厂商 recipe，同 id 覆盖内置；非模型厂商靠 `kind: non_model` 标识（缺省即模型）
- `~/.codex/config.toml`、`~/.codex/apim-models.json` — 一键导入到 Codex（`x` 键）写的，前者每次改写前备份成 `config.toml.apim.bak`
- `~/.pi/agent/models.json` — 一键导入到 Pi（`x` 键）写的，备份成 `models.json.apim.bak`；`PI_CODING_AGENT_DIR` 可改整个目录。`settings.json` / `auth.json` **都不写**（前者完全不碰，后者只读来判断哪把 key 在用）
- `APIM_CONFIG_DIR` 环境变量可重定向整个配置目录（测试用）

★ 不落在 apim 自己的文件里：密钥行的 ★ 是**回读各客户端配置现场**算出来的（codex 见约定 11、pi 见约定 14）。

## 核心约定

1. **密钥永不进仓库**。`.gitignore` 已排除 secrets.toml/.env；写文档、注释、提交信息时一律用 `sk-...` 占位。动手前 `grep -r "sk-"` 扫一遍。
2. **加厂商不改 Rust，额度一律走脚本**。厂商表单只配：ID/名称/Base URL/主页/**非模型勾选框**/探活路径/**脚本路径**（指向 ~/.config/apim/scripts/ 下可执行脚本）；编辑时路径没改就保留手写配置，清空即取消。每个厂商一个额度脚本（`balance.kind: script`，env 注入 APIM_TOKEN/APIM_VAR_*，stdout 逐行直显），声明式 http 解析已退役；不要再扩 DSL，也不要往表单加预设类型（厂商类型就两种，见约定 15）。AI 代写脚本的标准提示词在 docs/quota-script-prompt.md。
3. **Recipe 覆盖顺序**：builtin(include_str) → `<repo>/recipes/`（开发时）→ `~/.config/apim/recipes/`，后读的同 id 覆盖先读的。`origin: None` = 内置，不可删除只可编辑覆盖。
4. **模块路径稳定**：子模块类型经 mod.rs re-export（如 `crate::app::Modal`），拆文件不破坏外部 import。
5. **改完必跑**：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`。UI 改动跑 `cargo run -- --snapshot`（主界面，`APIM_SNAPSHOT_TAB=non-model` 出非模型分页）/ `--snapshot-form` / `--snapshot-provider-form`（`APIM_SNAPSHOT_KIND=non-model` 出勾上非模型的添加表单；`APIM_SNAPSHOT_PROVIDER=<id>` 出编辑表单）/ `--snapshot-inspector` 出纯文本渲染核对。
6. **添加功能前先确认 git 状态，全程用 git 管理便于回退**。动手前 `git status` 看工作区：有未提交的旧改动就先提交或 `git stash`，别和新功能混在一起；`git log --oneline -3` 确认当前在哪个提交上，心里有可回退的锚点。功能完成（fmt+clippy+test 通过）后一次性提交：先 `git status` + `git diff --stat` 核对只包含本次功能相关文件（不混入 secrets/临时文件），再提交。要回退用 `git checkout <提交号> -- <路径>`（局部）或 `git revert`（整体）。
7. **README 默认英文**（`README.md`），中文版在 `README.zh-CN.md`，两版内容保持同步：改一版必须同步另一版，顶部语言切换链接别删。
8. 提交信息中文，一行主题 + 要点列表；功能一次一提交。
9. **发版走 `docs/RELEASING.md`**：版本号单一来源是 `Cargo.toml`；打 `v*` tag 触发 5 平台构建，Release 转正后由 `release.yml` **显式 dispatch** `publish-npm.yml`，npm 自动跟随发布（**不能靠 `on: release: [published]`**：那次 Release 是 GITHUB_TOKEN 转正的，而 GITHUB_TOKEN 产生的事件不会再起新 workflow —— 实测 v0.1.4 踩过；`workflow_dispatch` 是例外，所以那个 job 需要 `actions: write`）；Homebrew 仍按手册手动更新；不要手改 Release 资产。
10. **开发跑本地代码一律 `cargo run -- <args>`，别 `cargo install --path .`**：本机 `apim` 是 npm 装的正试版（`/opt/homebrew/bin/apim`），PATH 里 `~/.cargo/bin` 排在它之后 —— `cargo install` 装出的二进制**不会被 `apim` 命中**，只会变成一个过期副本让人误判“改了没生效”。

11. **一键导入到 Codex（`x` 键）的硬约束**（都来自 codex 源码 + 真机实测，别凭感觉改）：
    - Codex 允许 `config.toml` 里同时定义多个 `[model_providers.*]`，但同一时刻只有顶层 `model_provider` 指向的那个激活 → apim **只切换激活项，旧 provider 块一律保留**（用户手写的注释 / `[projects]` / `[tui]` 也不能丢），不做整体重写。
    - **模型不在 `config.toml` 里**：写在 `model_catalog_json` 指向的独立 JSON（`~/.codex/apim-models.json`），config.toml 只有指针（GLM / DeepSeek 官方 Codex 文档也是这个写法）。**思考强度是顶层 `model_reasoning_effort`**，不写就等于没有；apim 固定写 `high`，四档（medium/high/xhigh/max）声明在每个目录条目上让 codex 的 `/model` 去选，**面板不做逐个选择**（用户明确要求）。
    - **写盘顺序与回滚**：先 `config_file::read/apply`（纯内存，失败时磁盘没动）→ 写目录 → 写 config → 校验；校验不过要用备份把两处改动都还原（否则用户看到「导入失败」，codex 配置其实已切到新厂商）。
    - **密钥落盘权限一律 600，且必须「建文件时就 600」**（`OpenOptions::mode`，不要 `fs::write` + `chmod`：中间有 0644 窗口）。`~/.codex/config.toml` 与它的 `.apim.bak` 备份里都有 `experimental_bearer_token`，用 `fs::write` 会按 umask 摊成 0644 = 密钥副本全机可读。
    - **杀 codex daemon 要能只杀 daemon**：`is_codex_server` 只认「可执行文件名正好是 codex + 第一个参数是 `app-server`」；扫「任意 token」会把 `codex --profile app-server` 这类用户会话也杀掉。只扫描一次、只杀扫描到的 pid（重扫会杀掉刚起、已加载新配置的 daemon）。
    - **`config.toml` 是符号链接时要跟随写入**（dotfiles 常这么管）：直接在链接路径上 tmp + rename 会把链接替换成普通文件、与仓库版本分叉；但**备份不跟随**，始终放 `~/.codex/` 下，免得含 token 的备份落进用户的 dotfiles 仓库。
    - **导入后必须让 codex 重启才能生效**：codex 的模型目录（`model_catalog_json`）只在 app-server daemon 启动时读一次，之后一直缓存（TUI 和桌面端都挂同一个 daemon）。实测现象：不重启时 `codex exec` 已能用新模型、但 `/model` 里还是旧的内置 GPT 表。cc-switch 的 v3.16.1 release notes 也只是提示用户重启；apim 做得更直接：导入成功后杀在跑的 `codex app-server`（`APIM_NO_RESTART_CODEX=1` 可关）。找进程必须**严格匹配**「可执行文件名正好是 codex + **子命令位（argv[2]）正好是 `app-server`**」；按「参数里有 `app-server`」或命令行 contains 会把 `codex --profile app-server`、`ps | grep codex app-server` 这类无关进程/用户会话也杀掉（已加单测守）。
    - 改写 `~/.codex/config.toml` **必须用 `toml_edit`**（`toml` 序列化会丢注释），写前备份成 `config.toml.apim.bak`，再 tmp + rename。注意加注释要挂在 **key 的 decor**（`leaf_decor_mut`）上，挂到 value 的 decor 会把值挤到下一行、产出非法 TOML。
    - `wire_api` 只接受 `"responses"`（0.134+ 删了 `"chat"`，写了会硬报错）→ 中转站得提供 `/v1/responses`；**导入面板的模型列表必须与 `m` 键完全一致**（同一个 `probe::fetch_models` / `parse_models`，只取模型名）；**不要做端点能力判断** —— `supported_endpoint_types` 是 new-api 后端的端点映射配置而非能力探测，实测会漏报（ikun 把 `gpt-6-sol` 标成只有 `openai`，实际 /responses 完全能用），照它筛会藏掉能用的模型。
    - `model_catalog_json` 相对路径按 `CODEX_HOME` 解析；它是**整表替换**（不是合并）；条目必须带 `base_instructions`（不能缺也不能给空串）；`supports_reasoning_summaries`（codex ≥0.144.5）与 `supports_parallel_tool_calls`（0.144.5~0.148.0-alpha.15）也被部分版本当必填 —— 两个字段名都给最安全（cc-switch v3.18.0 / v3.20.2 release notes 记了这两个坑）。
    - 模型条目照官方字段**手写迷你条目**（GLM / DeepSeek 官方 Codex 文档 + cc-switch 实测模板）：`shell_type: shell_command`、`apply_patch_tool_type: freeform`、中性 `base_instructions`、`input_modalities` fail-open 给 `[text, image]`、上下文窗口用 codex 给未知模型的默认值 272000。**不要克隆 `codex debug models --bundled` 里的 GPT 条目** —— 那会带进 `code_mode_only`、`use_responses_lite`、`max_context_window: 872000` 和 62KB harness 提示词（v1 就是这么错的一版，目录 64KB/模型）。导入完用 `codex debug models` 反向校验勾选的模型都在（校验是硬前提，没装 codex 直接报错）。
    - **★ 不存 apim 侧台账**：密钥行的 ★ 是**回读客户端现场**算出来的 —— codex 读 `config.toml` 的顶层 `model_provider` → `[model_providers.<key>]`，拿 `experimental_bearer_token`（或只有 `env_key` 时用 `base_url`）与 apim 密钥对账（表名 + token 都一致才算），所以用户手改了 codex 配置 ★ 会跟着变、不会留在旧密钥上。多个客户端都用同一把时并排成角标 `★C`（字母取自 `Agent::badge()`）。重算时点：启动、切厂商（`j`/`k`，`App::select_provider`）、`r` 刷新、5 分钟自动刷新、导入成功后。
    - **官方路（ChatGPT 登录）也要能一键导入**：`src/clients/codex/official.rs`，入口是**密钥表里 AUTH 行按 `x`**（或 CLI `apim auth openai import-codex`），不走 `x` 的密钥导入面板（官方路没有模型列表、也不要 API key）。写 `~/.codex/auth.json`（`auth_mode:"chatgpt"` + `OPENAI_API_KEY:null` + `tokens{id_token,access_token,refresh_token,account_id}` + `last_refresh`；**refresh_token 必须带** —— codex 自己拿它刷新，client id 与我们同一个 `app_EMoamEEZ73f0CkXaXp7hrann`、同样不发 `resource`）；config.toml **只摘** `model_provider` / `model` / **apim 自己写的** `model_catalog_json`（手写目录不动），其余一律保留 —— cc-switch 是整份清空，因为它有 provider 数据库兜底，apim 没有，而 `.apim.bak` 会被下一次导入覆盖，清空就是真丢用户手写的 `[projects]`/`[plugins]`/`notify`。`cli_auth_credentials_store = keyring|ephemeral` 时**直接拒绝**（codex 不读 auth.json；apim 不写钥匙串）。校验用 `codex login status`：**真机 0.161 把 `Logged in using ChatGPT` 写在 stderr、exit 0**（未登录/配置非法同样 stderr 但 exit 1），所以两个流和退出码都要看；失败用备份还原 auth.json + config.toml 两处。成功后照旧重启 daemon。额度面板 AUTH 区那行「Codex：官方 OAuth / 官方 API Key / provider x / 未登录」也是**回读 `~/.codex` 现场**（`codex_route`，与 ★ 同节奏），不存台账。
    - **AUTH 行是一个可选中的行**（只在内置 `openai` 分页，下标 = 过滤后密钥数，排在密钥行之后；`App::auth_row_index` / `auth_row_selected`）：`x` 走官方路导入，`c/i/d/m/e` 给「只支持 x」的提示而不是假装没有密钥。`clamp_selections` / `move_down` 的密钥上限要跟着它 +1。
    - **内置 `openai` 不可复制**：OAuth 凭据是全局一份（只认 id 正好是 `openai` 的厂商），副本会是一个「看着像 OpenAI、却永远登录不上、也没有 AUTH 行」的厂商。闸在 `recipe/dup.rs::ensure_copyable`（`duplicate_recipe` 开头调用），TUI `y` 与 CLI `provider copy` 都绕不过去；消息里指向 `provider add`。凭据改成按厂商 id 存是后续的事，见 `docs/TODO.md`。
    - 厂商 id 撞上保留名（`openai`/`ollama`/`lmstudio`/`amazon-bedrock*`）时加 `apim-` 前缀。

12. **`apim update` 只认三条渠道**（install.sh / npm / Homebrew，见 `docs/RELEASING.md` 的速查表）：`src/cli/update.rs` 按可执行文件路径认渠道（npm 看 `node_modules/apim-cli`、brew 看 `Cellar/apim`，其余当 install.sh 装的裸二进制），裸二进制那条复用官方 install.sh，但**不是 `curl | sh`**：URL 钉到本次要更新到的 tag、下载到临时文件、先做形状校验（是 shell 脚本 / 是本仓库安装器 / 含 sha256 校验）、再按 Release 发布的 `install.sh.sha256` 校验摘要（**拿不到摘要就拒绝执行**），最后用 `sh <file>` 跑；`APIM_INSTALL_DIR` 钉在当前二进制的**真实位置**（先 canonicalize，否则符号链接会被替换掉）保证原地更新。**`target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本一律不更新**（前者会被 Release 覆盖掉开发二进制，后者是 `cargo install` 留下的、被 PATH 遮挡的多余副本）。**npm 渠道更新前要同时核对主包与**当前平台子包**的版本**（`npm view <pkg> version`）：npm 的发布是异步的、主包会先可见，而 npm 对 optional 依赖失败是静默跳过 —— 只看主包就会装出一个跑不起来的 shim（v0.1.4 实测）。落后于 GitHub tag 时报出两个版本号并拒绝安装（`--force` 可越过）。加渠道要同时改 `Channel` 与它的识别规则、测试和 RELEASING 的表；`apim uninstall` 复用同一套 `detect_channel`，新渠道的卸载动作会被 `uninstall_program` 的穷尽 `match` 拦下（编译器逼你补），但提示语与单测仍要手工过一遍。

13. **加一个客户端（Claude Code / pi …）就是三处改动**，别在面板里写客户端专属分支：
    - `src/clients/mod.rs`：加 `Agent` 变体（所有 `match` 会被编译器强制补全）+ 一条分派（`label/badge/config_hint/reload_hint/default_model_step/active_key_ids/import/needs_reload/reload`）；`ImportRequest` / `ImportReport` 是共用的（面板只读这两个形状，不认客户端细节）；
    - `src/clients/<id>/`：新子模块，实现「写哪里 / 怎么写 / 写完后怎么校验 / 怎么重新加载 / 怎么从自家配置里认出正在用的密钥」；公共件在 `clients/file_io.rs`（跟随符号链接 + 写前备份 + 原子 600）与 `clients/lock.rs`（导入排他锁），别重写一遍；
    - `app/import` 与 `ui/import` **不用改**：它们只经 `Agent` 的这套方法调客户端；选择面板只列客户端名，不展开各家说明（要写就写在客户端子模块的文档里）。
    - 密钥行的 ★ 已经按客户端通用：`Agent::active_key_ids` 各自回读现场，UI 用 `Agent::badge()` 拼角标（`★C`、`★C,P`），加客户端只需补 `active_key_ids` 与 `badge` 两条分派（`clients/mod.rs` 里有一条注册表自检的测试守着「角标不许撞车」）。
    - **不要**给客户端造 YAML 配方（约定 2 的数据化范围是厂商协议）；客户端之间不是同一套协议，各写 Rust 更直白。
    - 面板的「选默认模型」第三步已经按客户端可关：`Agent::default_model_step()` 返回 `None` 就跳过（pi 就是这样），`ImportRequest.default_model` / `ImportReport.model` 都是 `Option`。只勾一个模型时也自动跳过。

14. **一键导入到 Pi（`x` 键）的硬约束**（都来自 pi 源码 + 真机实测，别凭感觉改）：
    - **只写一处**：`<agent-dir>/models.json` 的 `providers.<键>`（`name`/`baseUrl`/`api="openai-completions"`/`apiKey`/`models`）。`<agent-dir>` 默认 `~/.pi/agent`，`PI_CODING_AGENT_DIR` 可改（面板提示也读它）。
    - **`settings.json` 一个字都不动**：一键导入只干「往模型列表里加 provider + 模型」；默认 provider / 默认模型 / `enabledModels` 都是用户自己的设定（pi 里有 `/model` + `Ctrl+S`），apim 既不读也不写它（有逐字节断言的测试）。代价：用户设了非空 `enabledModels` 时，新模型不在 `/model` 的 scoped 视图里（也不进 `Ctrl+P` 循环），要他自己 `Ctrl+S` 存一次 —— 那一步 pi 自己会追加（`AgentSession._addPersistedDefaultToNonEmptyScope`）。因此 **Pi 没有「选默认模型」这一步**：`Agent::default_model_step()` 返回 `None`，面板勾完模型直接开写。
    - **provider 键一律加 `apim-` 前缀（只约束写、不约束认）**：pi 自带一大批同名 provider（`deepseek`/`openai`/`openrouter` …），`models.json` 里同名的条目会被 `applyModelsJson` 用来**覆盖那个内置 provider 的 baseUrl**（等于把用户的 OpenAI 指到我们的中转站）。加前缀永远不会撞名，`/model` 里也一眼看出是 apim 写的；但认 ★ 时不看名字（见下一条）。
    - **只动我们认识的键**：`providers.<键>` 里的 `headers` / `compat` / `modelOverrides` / `authHeader` 与其它 provider 都原样保留（`pi/tests/import.rs` 有断言守）；`settings.json` 完全不碰。
    - **模型条目用 pi 的默认值兜底**：只写 `id`/`name`/`reasoning: true`/`input: [text, image]`，**不写** `contextWindow`/`maxTokens`/`cost` —— pi 对缺省用自己的保守默认（128000 / 16384 / 零价），apim 不替它编数字（与 codex 那边写 272000 不同：那是 codex 给未知模型的默认值）。
    - **校验靠 `pi --list-models`**（同 codex 的 `codex debug models`）：输出是定宽表，要匹配 `provider` 与 `model` **两列都对**（同名模型挂在别的 provider 下不算）；不通过就用备份还原 `models.json`（原来没有的文件删掉），没装 pi 直接报错。
    - **pi 没有常驻进程可杀**：`needs_reload()` 返回 false，提示语是「在 Pi 里打开 `/model`（或重开）即可看到新模型」（`Agent::reload_hint`，面板不写客户端分支）。
    - **★ 扫 pi 配置里的每一份凭据**（pi **没有**「唯一激活的 provider」：`defaultProvider` 只是启动默认值，`/model` / `Ctrl+P` / 会话记录都可能用别的）：`<agent-dir>/auth.json`（`/login` 存的 `type:"api_key"` 的 `key`；`type:"oauth"` 是订阅凭据，不算）与 `models.json` 里**每个带 `apiKey`** 的 provider（没有 `apiKey` 的不算 —— 它可能靠环境变量/登录用，那个值看不见，只看地址会把地址相同的别的密钥误标）；能看见明文就**只比 token**（同一个 token 就是同一把），`$ENV` / `!cmd` 看不见才退化成比 `base_url`。所以内置 provider 用着 apim 的 key 也认。
    - **`auth.json` 只读不写**：那里面还有你的订阅凭据（`type:"oauth"`，含 refresh token），而且 pi 用 `proper-lockfile` 自己管、读取时**逐条校验**（任一条不合法整份加载失败）—— apim 写它既帮不上忙又可能把你登出订阅。apim 的 key 一律写在 `models.json` 的 `apiKey` 里。
    - **与 codex 侧的语义差别（有意为之）**：codex 同一时刻只有一个激活 provider → ★ = 当前激活的那个在用它；pi 是「配置里有的凭据都算在用」→ 导入过几把就有几个 `★P`。这是两个客户端的真实差别，不是实现偷懒。
    - **只支持 API key 这一路**：pi 的订阅渠道是 `/login` 的 OAuth（凭据在 `auth.json`），apim 拿不到也不该碰。
    - **`apim uninstall` 要报 pi 残留**（同 codex：只报不删）：`models.json` 里的 `providers.apim-*` 条目与含明文 apiKey 的 `models.json.apim.bak`（`cli/uninstall/cleanup.rs::pi_leftovers_in`）。

15. **厂商类型 = 两个分页，类型创建时定死**（模型 / 非模型）：
    - 数据源只有一个字段：recipe 的 `kind: model|non_model`（`ProviderKind`，缺省 model，所以老 YAML / 内置 recipe 不用动；序列化时 model 不写 kind）。非模型厂商 **没有模型列表（`m`）、不能导入客户端（`x`）、不探活**（`health` 恒空，`m`/`x` 给提示而不是静默）；密钥、别名/分组、主页 `⏎`、`c`、`y` 复制、`^Z`、额度脚本、`provider ls`、`apim status` 全都一样。
    - **类型的唯一入口是创建**：TUI 添加表单的「非模型」勾选框（默认跟随当前分页）、CLI `provider add --kind`。编辑表单里它只读，且 `save_provider_form` 一律取原值（规则落在保存这一处，不靠 UI 灰掉）；`provider set --kind` 与给非模型厂商传 `--health` 都报错。要换类型只能删了重建 / `provider copy`。
    - **不适用当前类型的表单行直接不出现**：`Field::Toggle` 用 `.hides(下标)` 声明「勾上就把这一行收起来」，勾选状态一变由 Form 自己开关（构造时先按初值对齐）；隐藏的行不渲染、不占弹窗高度、Tab 跳过、也写不进去（`Field::Text{hidden}`）。添加/编辑表单的「探活路径」都靠它养活：勾上非模型就没了这一行（不是灰掉）。
    - **详情弹窗只列真适用的**：鉴权/探活只服务 HTTP 请求（探活、模型列表），非模型厂商两行都不列；也不列「类型」那种背景信息（打开它的分页已经说明了一切）。
    - **分页状态在 App 里**：`tab: ProviderKind` + `tabs: [TabView; 2]`（每个分页自己的 `provider_ids` / `selected` / `filter`，下标 = `ProviderKind::ALL` 顺序）；取值一律走 `provider_ids()` / `provider_ids_filtered()` / `selected_provider()` / `provider_filter()`，**不要**再引入第三个「全局选中项」。`rebuild_provider_list` 两个分页各建一份（有密钥的在前，其余按 id 排序）；保存 / 复制厂商后必须 `focus_provider(id)`（分页跟着厂商类型跳，否则新建的厂商落在看不见的那一页）。
    - **快捷键**：`Tab` = 切分页（两栏焦点都生效），`h`/`l` 与 ←/→ = 切左右栏焦点。改按键提示时四个分支都要过一遍（厂商焦点 / 模型页密钥焦点 / 模型页密钥焦点且光标在 AUTH 行 / 非模型页密钥焦点，非模型页不出现 `m`/`x`；AUTH 行那支只留 `x`/`o`/`c`/`a`/`r`，见约定 11）。
    - **状态口径**：非模型厂商没有探活，密钥表状态列与左栏厂商摘要都用**额度脚本的成败**（`ui::script_status`，一处写、两个地方用；CLI `status` 同一口径）。
    - `health` / `models_url` 对非模型照旧保留在 YAML 里（加载期不拒收），但**不生效**：探活/模型列表的消费点一律走 `Recipe::health_call()`（非模型恒为 None）与 `is_model()` 门，手改 `kind:` 也不会拿旧 `health` 去发请求；`auth` 可省略（`#[serde(default)]`）——非模型厂商没有 HTTP 请求。

16. **待办写在 `docs/TODO.md`**：已知缺口（例：「OpenAI OAuth 凭据按厂商存」）连同「现状 / 为什么不现在做 / 真要动哪些文件」一起记在那里，别只留在脑子里；做了就把它从 TODO 删掉。

## 验证命令速查

```bash
cargo run                              # 进 TUI（跑当前代码）
cargo run -- --snapshot                # 真实接口拉数据渲染成文本（不进 TUI）
APIM_SNAPSHOT_TAB=non-model cargo run -- --snapshot          # 非模型分页的主界面快照
APIM_SNAPSHOT_KIND=non-model cargo run -- --snapshot-provider-form   # 添加表单：勾上「非模型」的样子
APIM_SNAPSHOT_PROVIDER=<id> cargo run -- --snapshot-provider-form   # 编辑表单（ID 与类型只读；非模型没有探活那一行）
cargo run -- --snapshot-inspector      # 详情弹窗快照：假状态不拉接口；=provider 出厂商详情
cargo run -- --snapshot-import         # 一键导入面板快照：第一步选客户端
cargo run -- --snapshot-import-models  # 一键导入面板快照：第二步勾选模型
cargo run -- --snapshot-import-default # 一键导入面板快照：第三步选默认模型
cargo test                             # 单测（recipe/表单/CLI 沙盒/codex+pi 适配等）
cargo +1.88 check --locked --all-targets   # 声明的 MSRV（Cargo.toml 的 rust-version）要真能编；CI 的 msrv job 跑同一条
cargo test -- codex_real_end_to_end --ignored --nocapture   # 需本机装 codex：真机端到端（生成目录 + 让 codex 校验）
cargo test -- codex_real_official_end_to_end --ignored --nocapture  # 需本机装 codex：官方路（合成凭据，不读真实 token；不碰真实 ~/.codex）
cargo test -- pi_real_end_to_end --ignored --nocapture      # 需本机装 pi：真机端到端（写两份 JSON + 让 pi 列模型）
APIM_SNAPSHOT_AGENT=pi cargo run -- --snapshot-import-models   # 换客户端出面板快照（默认第一个；「选默认模型」那屏只对 codex 存在）
cargo run -- update --check                # 认安装渠道 + 报当前/最新（不动手）
apim provider ls --json                # CLI 冒烟（跑已发布版；本地代码用 cargo run -- provider ls）
```

> 开发期不要 `cargo install --path .`（见约定 10）。
