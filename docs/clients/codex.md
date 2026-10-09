# Codex 契约：一键导入（第三方路）与官方路

**这是什么**：`x` 键把「一把密钥 + 它的厂商 + 勾选的模型」写进 Codex，以及把 apim 的 ChatGPT 凭据导入
Codex **官方路**时，Codex 侧的契约与实测约束。

**别处已经写了什么**：面向用户的步骤与「写到哪张表」在 `README.md` 的 *One-click import into Codex / Pi*；
取舍与「故意不做」在 `docs/adr/`（0002 客户端适配、0006 OAuth 的四个有意取舍、0008 只服务内置 `openai`、
0009 单向采纳）；`AUTH` 行与厂商类型的交叉部分见 `docs/provider-kinds.md`。

**两条路**：

- **第三方路**（密钥行按 `x`）：写 `~/.codex/config.toml` 的激活 provider + `~/.codex/apim-models.json`
  模型目录。
- **官方路**（内置 `openai` 的 `AUTH` 行按 `x`，或 `apim auth openai import-codex`）：写
  `~/.codex/auth.json` 的 ChatGPT 登录 + 摘掉 config.toml 的第三方路由。

## 写到哪

### 第三方路

- **`config.toml` 允许同时定义多个 `[model_providers.*]`，但同一时刻只有顶层 `model_provider` 指向的
  那个生效** → apim **只切换激活项**：旧 provider 块、用户手写的注释、`[projects]`、`[tui]`、`notify`
  一律保留，**不做整体重写**。
- **想自己多套并存**（不经过 apim 切来切去）：用官方的 `codex --profile <name>` +
  `~/.codex/<name>.config.toml`（apim 不碰这条机制）。
- 激活的那个 provider 子表（`[model_providers.<key>]`）是 apim **整块替换**的：同一块里 `env_key` /
  `auth` 与 `experimental_bearer_token` 不能共存，替换掉旧块就没有它们。
- **模型不写在 `config.toml` 里**：写在 `model_catalog_json` 指向的独立 JSON
  （`~/.codex/apim-models.json`），config.toml 只有指针（GLM / DeepSeek 官方 Codex 文档也是这个写法）。
- **厂商 id 撞上 Codex 保留名时加 `apim-` 前缀**，保留名是：`openai` / `ollama` / `lmstudio` /
  `amazon-bedrock` / `amazon-bedrock-runtime`。

### 官方路

机制来自 cc-switch（`codexProviderPresets.ts` 的「OpenAI Official」卡 + `codex_config.rs`）与本机 codex 0.161 实测；**浏览器登录那一段的实测红线（为什么要复刻 Codex CLI 客户端、动态注册为什么会 401、两个 token 端点、刷新口径）见本节末尾**。

- `~/.codex/auth.json` 是 **Codex 自己的**登录文件（`codex login` / `codex logout` 也管它）：apim 只在
  导入时写它，之后**只读**。
- `config.toml` **只摘三个键**：顶层 `model_provider` / `model` / **apim 自己写的** `model_catalog_json`
  （手写的目录不动），其余一律保留 —— cc-switch 是整份清空，因为它有 provider 数据库兜底；apim 没有，
  而 `.apim.bak` 会被下一次导入覆盖，清空就是真丢用户手写的 `[projects]` / `[plugins]` / `notify`。

## 写什么形状

### `config.toml`（第三方路）

顶层四个键：`model_provider` / `model` / `model_reasoning_effort` / `model_catalog_json`（相对路径，按
`CODEX_HOME` 解析）。认证只写一个键：`experimental_bearer_token`。

- **思考强度是顶层 `model_reasoning_effort`**，不写就等于没有；apim 固定写 `high`。
- **`wire_api` 只接受 `"responses"`**（0.134+ 删了 `"chat"`，写了会硬报错）→ 中转站得提供
  `/v1/responses`。

### `apim-models.json`（模型目录）

- 它是**整表替换**（不是合并），相对路径按 `CODEX_HOME` 解析。
- **条目必须带 `base_instructions`**：不能缺、也不能给空串（两样都会让整个目录解析失败）；也可以用
  `model_messages.instructions_template` 代替。
- `supports_reasoning_summaries`（codex ≥0.144.5）与 `supports_parallel_tool_calls`
  （0.144.5 ~ 0.148.0-alpha.15）被部分版本当必填 → **两个字段名都给**最安全。
- 四档思考等级（`medium` / `high` / `xhigh` / `max`）声明在每个目录条目上，交给 codex 的 `/model` 去选，
  **面板不做逐个选择**（用户明确要求）；默认档 = 顶层 `model_reasoning_effort` = `high`。
- **手写迷你条目**（照 GLM / DeepSeek 官方 Codex 文档 + cc-switch 实测模板）：`shell_type:
  shell_command`、`apply_patch_tool_type: freeform`、中性 `base_instructions`、`input_modalities`
  fail-open 给 `[text, image]`、上下文窗口用 codex 给未知模型的默认值 **272000**（与「没有目录」时的行为
  一致）。
- **不要克隆 `codex debug models --bundled` 里的 GPT 条目**：那会带进 `tool_mode: code_mode_only`、
  `use_responses_lite: true`、`max_context_window: 872000` 和 62KB 的 GPT harness 提示词 —— 对第三方模型
  全是错的（v1 就是这么错的一版：目录 64KB/模型）。
- **这个文件存在的唯一理由**：自定义 provider 的模型不在目录里时，codex 会打
  `Model metadata for ... not found. Defaulting to fallback metadata`，并且 `/model` 里看不到它们。

### `auth.json`（官方路）

`auth_mode: "chatgpt"` + `OPENAI_API_KEY: null` + `tokens{id_token, access_token, refresh_token,
account_id}` + `last_refresh`。

- **`refresh_token` 必须带**：codex 自己拿它刷新；client id 与 apim 用的是同一个
  `app_EMoamEEZ73f0CkXaXp7hrann`、同样不发 `resource`，所以这份凭据 codex 能自续。
- apim 侧那份凭据是**单份**（`~/.config/apim/openai-oauth.json`），只服务内置 `openai`。
- **浏览器登录缺省复刻 Codex CLI 客户端**（Pi / cc-switch 同款：固定公开 client id
  `app_EMoamEEZ73f0CkXaXp7hrann`、`localhost` 回调、不发 nonce、不申请 `resource`）。**不要改回缺省走
  动态注册**：实测动态注册签发的 token 里 `https://api.openai.com/auth` 只有 `per_user_salt` +
  `encrypted_auth_metadata`（无 `chatgpt_account_id`），请求 `backend-api/wham/usage` 一律 401；
  `APIM_OAUTH_CLIENT=apim` 是显式切回的口子（写错的值会被点名写进诊断日志，不静默）。token 端点按
  `/api/accounts/oauth/token` → `/oauth/token` 依次尝试；刷新 token 与登录同口径（Codex CLI 档位不带
  `resource`），服务端不轮换 refresh token 时沿用旧的。

## 怎么写（文件 IO 与顺序）

- **`config.toml` 必须用 `toml_edit`**：`toml` 序列化会丢注释。写前备份成 `config.toml.apim.bak`，再
  tmp + rename。加注释要挂在 **key 的 decor**（`leaf_decor_mut`）上 —— 挂到 value 的 decor 会把值挤到
  下一行、产出非法 TOML。
- **`config.toml` 是符号链接时要跟随写入**（dotfiles 常这么管）：直接在链接路径上 tmp + rename 会把链接
  替换成普通文件、与仓库版本分叉。但**备份不跟随**，始终放 `~/.codex/` 下 —— 免得含 token 的备份落进
  用户的 dotfiles 仓库。
- **含密钥的落盘一律 600，且必须「建文件时就 600」**（`OpenOptions::mode`，不要 `fs::write` + `chmod`：
  中间有一个 0644 窗口）。`config.toml` 与它的 `.apim.bak` 备份里都有 `experimental_bearer_token` ——
  用 `fs::write` 会按 umask 摊成 0644 = 密钥副本全机可读。
- **写盘顺序与回滚**（第三方路）：先 `config_file::read/apply`（纯内存，这一步失败磁盘没动）→ 写目录 →
  写 config → 校验；**校验不过要用备份把两处改动都还原**，否则用户看到「导入失败」、codex 配置其实已经
  切到新厂商了。

## 模型列表从哪来

- **导入面板的模型列表必须与 `m` 键完全一致**：同一个 `probe::fetch_models` / `parse_models`，只取模型名。
- **不要做端点能力判断**：`supported_endpoint_types` 是 new-api 后端的端点映射配置，不是能力探测，实测会
  漏报（ikun 把 `gpt-6-sol` 标成只有 `openai`，实际 `/responses` 完全能用）—— 照它筛会藏掉能用的模型。

## 怎么校验

- **第三方路**：跑 `codex debug models` 反向校验勾选的模型都在。**校验是硬前提**：没装 codex 直接报错。
- **官方路**：跑 `codex login status`，**两个流（stdout + stderr）和退出码都要看**，而且必须认到
  `Logged in using ChatGPT` 这一句：
  - 真机 codex **0.161** 把 `Logged in using ChatGPT` 写在 **stderr**、**exit 0**（未登录 / 配置非法也是
    stderr，但 exit 1）；
  - **API Key 登录**同样 exit 0，输出是 `Logged in using an API key - sk-***` —— 把它当成功等于把
    「官方 OAuth 已导入」说成假话；
  - `cli_auth_credentials_store = keyring|ephemeral` 时**直接拒绝**：codex 根本不读 `auth.json`，apim 也
    不写系统钥匙串 —— 拒绝好过写一份看不见的凭据；
  - 失败时用备份**同时**还原 `auth.json` 与 `config.toml` 两处，**不能用 `&&` 短路**（第一处还原失败
    不该跳过第二处）。

## 怎么重载

- **导入后必须让 codex 重启才能生效**：模型目录（`model_catalog_json`）只在 app-server daemon **启动时**
  读一次，之后一直缓存（TUI 与桌面端挂同一个 daemon）。实测现象：不重启时 `codex exec` 已能用新模型、
  但 `/model` 里还是旧的内置 GPT 表（cc-switch 的 v3.16.1 release notes 也只是提示用户重启）。apim 做得
  更直接：导入成功后杀在跑的 `codex app-server`；`APIM_NO_RESTART_CODEX=1` 可关。
- **找进程必须严格匹配「可执行文件名正好是 codex + 子命令位（`argv[2]`）正好是 `app-server`」**：按
  「参数里有 `app-server`」或命令行 contains 会把 `codex --profile app-server`、
  `ps | grep codex app-server` 这类无关进程 / 用户会话也杀掉。**只扫描一次、只杀扫描到的 pid**（重扫会
  杀掉刚起、已加载新配置的 daemon）。
- **官方路**成功后照旧重启 daemon。
- **想确认该不该重启 / 重启有没有生效**：`ps -o pid,lstart,command -p $(pgrep -f "app-server" | tr '\n' ',')`
  看那几个 daemon 的启动时间 —— 早于导入时间就说明它还缓存着旧目录。

## 怎么认出正在用的密钥（`★` 与 `codex_route`）

- **不存 apim 侧台账**：`★` 是**回读客户端现场**算出来的 —— 读 `config.toml` 的顶层 `model_provider`
  → `[model_providers.<key>]`，拿 `experimental_bearer_token`（只有 `env_key` 时退化成比 `base_url`）与
  apim 密钥对账：**表名 + token 都一致**才算。
- 因此用户手改了 codex 配置，`★` 会跟着变，不会留在旧密钥上。
- 多个客户端都用同一把时并排成角标（`★C`，字母取自 `Agent::badge()`；注册表自检测试守着「角标不许
  撞车」）。
- **重算时点**：启动、切厂商（`j` / `k`）、`r` 刷新、5 分钟自动刷新、导入成功后。
- 额度面板 AUTH 区那行「Codex：官方 OAuth / 官方 API Key / provider x / 未登录」也是**回读 `~/.codex`
  现场**（`codex_route`，与 `★` 同节奏）。

## `AUTH` 行（官方路的入口，不是一个密钥）

- **只在内置 `openai` 分页**，下标 = 过滤后密钥数、排在密钥行之后（`App::auth_row_index` /
  `auth_row_selected`）。它在 `open_import` 里**提前分流**，所以导入面板本身仍然不认识任何客户端细节。
- 在它上面按 `x` 走官方路导入；按 `c` / `i` / `d` / `m` / `e` 给「只支持 x」的提示，而不是假装没有密钥。
- `clamp_selections` / `move_down` 的密钥上限要跟着它 **+1**。
- 已知小账（见 `docs/TODO.md`）：手改 `openai.yaml` 为 `kind: non_model` 后，那一行按 `c/i/d/m/e` 提示
  「只支持 x」而 `x` 又按非模型门被拒（两句话打架）；密钥过滤命中为空时 AUTH 行仍是唯一选中行，与
  「没有匹配的密钥」的提示看着矛盾。

## 官方路的单向性与采纳

- **只读 Codex 现场、永不写 `auth.json`**：Codex 刷新后只把新 token 写回 `auth.json`，apim 在自己的刷新
  节奏里（`refresh_active_keys`）**读回来**。
- **ownership 是硬门**：只有 `auth.json` 里的 refresh token 与 apim 自己那份**逐字符相同**才采纳 ——
  用户自己 `codex login` 的另一个账号绝不会被抄进 apim。
- **fail-closed**：凭据不全、读不出有效期、判不了 → 什么都不做（最坏是「这次没同步」）。
- **幂等**：只在收到的 token 确实更新时才写盘。
- 已知限制：服务端真的轮换 refresh token 时 ownership 对不上 → 不同步，退回「按 `o` 重新授权」。决策与
  重开条件见 `docs/adr/0009`。

## 内置 `openai` 不可复制

- 闸在 `recipe/dup.rs::ensure_copyable`（`duplicate_recipe` 开头调用）：TUI `y` 与 CLI `provider copy`
  都绕不过去，消息里指向 `provider add`。
- 理由：OAuth 凭据是**全局一份**、只认 id 正好是 `openai` 的厂商，副本会是一个「看着像 OpenAI、却永远
  登录不上、也没有 AUTH 行」的厂商。决策与重开条件见 `docs/adr/0008`。

## 真机踩过的坑与已知小账

- **登录校验绑死了 codex 的措辞**：将来 codex 换掉 `Logged in using ChatGPT` 会硬失败 + 回滚（安全但
  用户可见）。真出问题就在 `LOGGED_IN_MARKER` 上加一条备选措辞，并同步真机 opt-in 测试
  `codex_real_official_end_to_end`。
- **CLI `import-codex` 不区分 ambiguous 守护进程**：TUI 会说「有 N 个类似进程没敢动，若它正开着请手动
  重启」，CLI 只说「重开 Codex 生效」。要修就把 `app/mod.rs` 那段文案镜像到 `cli/auth.rs`；顺带
  `App::apply_oauth_codex` 的四个重启分支与 CLI 的 happy path 目前没有测试。
- **摘掉三个键时，挂在它们上面的注释一起没了**（`toml_edit::DocumentMut::remove` 连 decor 一起删）：
  README 的「其余不动（含注释与顺序）」说的是**别的键**，别读成「连被删键的注释也保留」。要真保留得把
  prefix 挪到下一个键上，但那条注释本来就说的是被删掉的键，搬过去反而误导 —— 倾向保持现状，只在文档里
  别过度承诺。
- **provider 子表整块替换**：用户写在 `[model_providers.<key>]` 里的额外键会随替换消失（「只切激活项」
  之外的第二处既有行为）。

## 何时该重开

- **codex 换掉登录成功措辞 / 模型目录字段**：加备选措辞或字段，并同步真机 opt-in 测试。
- **codex 不再把模型目录缓存到 daemon 启动那一刻**：重启那一步可以撤掉。
- **真的需要「同一个 OpenAI 厂商 + 多个 ChatGPT 账号」**：先解决「Codex 官方登录位只有一个」的**呈现**
  问题，再谈凭据分表与放开复制（`docs/adr/0008`）。
- **出现第三个客户端且三家的请求形状开始收敛**：才值得重新评估 `Agent` 这套分派（`docs/adr/0002`）。
- **服务端真的轮换 refresh token**（到「每次都要重新登录」的程度）：放宽 ownership 判定需要新 ADR。
