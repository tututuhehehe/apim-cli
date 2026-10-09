# Pi 契约：一键导入到 Pi

**这是什么**：`x` 键把「一把密钥 + 它的厂商 + 勾选的模型」写进 Pi 时，**Pi 侧的契约与实测约束**。

**别处已经写了什么**：面向用户的步骤与「写到哪张表」在 `README.md` 的 *One-click import into Codex / Pi*；
客户端适配为什么是 Rust 子模块而不是 YAML 配方在 `docs/adr/0002`；公共落盘件（写前备份 + 原子 600 +
导入排他锁）与「加客户端的标准三处改动」在 AGENTS.md 约定 13，**别重写一遍**。

## 写到哪

**只写一处**：`<agent-dir>/models.json` 的 `providers.<apim-厂商id>`。

- `<agent-dir>` 默认 `~/.pi/agent`，`PI_CODING_AGENT_DIR` 可改（面板提示也读它）。
- 写盘前备份成 `models.json.apim.bak`（原来没有这个文件就不产生备份）。
- **`settings.json` 一个字都不动**（红线）：一键导入只干「往模型列表里加 provider + 模型」；默认 provider /
  默认模型 / `enabledModels` 都是用户自己的设定（Pi 里有 `/model` + `Ctrl+S`），apim **既不读也不写**
  （有逐字节断言的测试）。
- **Pi 的 `auth.json` 只读、不写**（红线）：那里面还有订阅凭据（`type: "oauth"`，含 refresh token），
  而且 Pi 用 `proper-lockfile` 自己管、读取时**逐条校验**（任一条不合法整份加载失败）—— apim 写它既帮不
  上忙又可能把你登出订阅。apim 的 key 一律写在 `models.json` 的 `apiKey` 里。

## 写什么形状

`providers.<apim-厂商id>` 里写：`name` / `baseUrl`（缺 `/v1` 时补上，与 codex 侧同一个
`normalize_base_url`）/ `api = "openai-completions"` / `apiKey` / `models`（勾选的那几个）。

- **provider 键一律加 `apim-` 前缀（只约束写、不约束认）**：Pi 自带一大批同名 provider（`deepseek` /
  `openai` / `openrouter` …），`models.json` 里同名的条目会被 `applyModelsJson` 用来**覆盖那个内置
  provider 的 baseUrl**（等于把用户的 OpenAI 指到我们的中转站）。加前缀永远不会撞名，`/model` 里也一眼
  看出是 apim 写的；但**认 `★` 时不看名字**（见下）。
- **模型条目用 Pi 的默认值兜底**：只写 `id` / `name` / `reasoning: true` / `input: [text, image]`，
  **不写** `contextWindow` / `maxTokens` / `cost` —— Pi 对缺省用自己的保守默认（128000 / 16384 / 零价），
  apim 不替它编数字。（与 codex 那边写 272000 不同：那是 codex 给未知模型的默认值。）
- **只动我们认识的键**：`providers.<键>` 里的 `headers` / `compat` / `modelOverrides` / `authHeader` 与
  其它 provider 全部原样保留（`pi/tests/import.rs` 有断言守）；`settings.json` 完全不碰。

## 怎么写

- **先在内存里改完**（失败时磁盘一点没动）→ 写盘 → 校验 → **校验不过就用备份还原**（原来没有
  `models.json` 的文件就删掉）—— 顺序与 codex 第三方路一致。
- 写盘与备份都走 `clients/file_io.rs` → `config::write_private`：**建文件时即 600**。`models.json` 与
  它的 `.apim.bak` 里都有明文 `apiKey`，用 `fs::write` 会按 umask 摊成 0644。

## 怎么校验

- **靠 `pi --list-models`**（同 codex 的 `codex debug models`）：输出是定宽表，要匹配 `provider` 与
  `model` **两列都对**（同名模型挂在别的 provider 下不算）。
- **没装 Pi 直接报错**（校验是硬前提，不是「尽力而为」）。找可执行文件的顺序：`APIM_PI_BIN` →
  `PI_BIN` → `PATH` → 三个常见安装路径（`/opt/homebrew/bin`、`/usr/local/bin`、`/usr/bin`）。
- 不通过 → 用备份还原 `models.json`。

## 怎么重载

- **Pi 没有常驻进程可杀**：`needs_reload()` 返回 `false`，提示语是「在 Pi 里打开 `/model`（或重开）即可
  看到新模型」（`Agent::reload_hint`，面板不写客户端分支）。

## 怎么认出正在用的密钥（`★`）

- Pi **没有**「唯一激活的 provider」：`defaultProvider` 只是启动默认值，`/model` / `Ctrl+P` / 会话记录都
  可能用别的 —— 所以 `★` **扫配置里的每一份凭据**：
  - `<agent-dir>/auth.json` 里 `/login` 存的 `type: "api_key"` 的 `key`（`type: "oauth"` 是**订阅**凭据，
    不算）；
  - `models.json` 里**每个带 `apiKey`** 的 provider（没有 `apiKey` 的不算 —— 它可能靠环境变量 / 登录用，
    那个值看不见；只看地址会把地址相同的别的密钥误标）。
- 能看见明文就**只比 token**（同一个 token 就是同一把）；`$ENV` / `!cmd` 这类看不见的写法才退化成比
  `base_url`。所以**内置 provider 用着 apim 的 key 也认**，provider 叫什么名字都不影响。
- **与 codex 侧的语义差别（有意为之）**：codex 同一时刻只有一个激活 provider → `★` = 当前激活的那个在用；
  Pi 是「配置里有的凭据都算在用」→ 导入过几把就有几个 `★P`。这是两个客户端的真实差别，不是实现偷懒。

## 范围：只支持 API key 这一路

Pi 的订阅渠道是 `/login` 的 OAuth（凭据在 `auth.json`），apim 拿不到也不该碰。`apim uninstall` 只**报**
残留、不删（同 codex）：`models.json` 里的 `providers.apim-*` 条目与含明文 apiKey 的
`models.json.apim.bak`。

## 真机踩过的坑与已知小账

- **不动 `settings.json` 的代价（接受）**：用户设了非空 `enabledModels` 时，新模型不在 `/model` 的
  scoped 视图里（也不进 `Ctrl+P` 循环），要他自己 `Ctrl+S` 存一次 —— 那一步 Pi 自己会追加
  （`AgentSession._addPersistedDefaultToNonEmptyScope`）。因此 **Pi 没有「选默认模型」这一步**：
  `Agent::default_model_step()` 返回 `None`，面板勾完模型直接开写；只勾一个模型时也自动跳过。
- **`★` 的多客户端标记**：`ui/keys.rs` 目前只问 `is_active(Agent::Codex, …)`；加客户端时再决定要不要显示
  「★ 导入给 X」这种多标记（`docs/TODO.md`）。

## 何时该重开

- **Pi 改了 `models.json` / `auth.json` 的形状或内置 provider 列表**：形状常量与 `apim-` 前缀的理由都要
  重新核一遍（本页的约束都来自 Pi 源码 + 真机实测）。
- **Pi 出了「无模型清单」的对接方式**（不需要 `models` 列表）：`ImportRequest.models` / `default_model`
  的 `Option` 化就是那一步的前置（`docs/TODO.md`）。
- **Pi 的 `settings.json` 也能安全地由外部写**（例如 Pi 自己提供了 CLI 开关）：那时才谈「一键设默认
  provider」，否则永远不动它。
- **出现第三个客户端且三家的请求形状开始收敛**：才值得重新评估 `Agent` 这套分派（`docs/adr/0002`）。
