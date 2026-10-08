# TODO — 已知缺口与后续要做的事

给维护者看的待办清单（不是用户文档）。每条都要写清**现状 / 为什么现在不做 / 真要做得动哪些地方**，
免得下次有人重新调研一遍。

## 1. OpenAI OAuth 凭据按厂商存（让复制的 OpenAI 也能登录）

**现状**：`~/.config/apim/openai-oauth.json` 是**全局一份**凭据。只有 id 正好是 `openai` 的厂商
能用它：

- `o` 键登录（`tui.rs` 里 `current_provider_id() == Some("openai")` 才放行）；
- 密钥表的 `AUTH` 行（`ui::oauth_state` / `ui::oauth_row_status` 同一个门）；
- `x` 把凭据导入 Codex 官方路（`clients/codex/official.rs` 读的就是这一份）；
- CLI `apim auth openai login|status|logout|import-codex`（`openai_auth::path()` 不接厂商 id）。

**因此现在内置 `openai` 一律不允许复制**：`recipe/dup.rs::ensure_copyable` 拦下 TUI `y` 与
CLI `provider copy`（消息里指向 `provider add`）。否则用户会得到一个 `openai-copy`：看着像
OpenAI、能配密钥、却永远登录不上，也没有 AUTH 行 —— README 的快捷键表里也写明了这一点。

**要做的事**：把凭据改成按厂商 id 存（例如 `openai-oauth.<id>.json`，或一份 JSON 里按 provider id
分表），并把上面四处「id 必须是 openai」的判断换成「这个厂商有 OAuth 能力」（recipe 上加个字段，
比如 `oauth: openai_codex`，缺省无 —— 这样也不会往表单里加「预设类型」，见 AGENTS.md 约定 2）。

要动的地方：

- `src/openai_auth.rs`：`path` / `load` / `save` / `remove` / `login` / `fetch_usage` / `credential_path`
  全部带上 provider id；旧的单文件凭据要在读取时迁移（当成 `openai` 的）。
- `src/app/mod.rs`：`oauth_balance` / `oauth_checking` / `oauth_seq` / `codex_route` 这些状态目前是
  「一份」，要么跟着当前厂商 id 走，要么改成 `HashMap<String, _>`；`spawn_oauth_probe` 与
  `refresh_all_keys` 的触发条件也要跟着改。
- `src/ui/{keys,balance}.rs`：AUTH 行与额度面板的 AUTH 区块从「厂商 id == openai」改成
  「当前厂商有 OAuth 能力」。
- `src/cli/auth.rs`：`auth openai ...` 加 `--provider <id>`（缺省 `openai`），或另开
  `apim auth <provider> ...`。
- **要决定的事**：Codex 的官方路只有**一个**登录位（`~/.codex/auth.json`）。如果 apim 里有两个
  厂商各自有 OAuth 凭据，`x` 该写谁的？合理的口径是「写当前厂商那一份，覆盖前照旧备份」，
  但要在 UI 上说清「Codex 官方登录位只有一个，导入 B 会顶掉 A」。

## 2. 官方路导入不回写同步 codex 刷新的 token

**现状**：`x` 把 apim 的 OAuth 凭据写进 `~/.codex/auth.json` 之后，codex 自己会刷新 access token
（`last_refresh` / access token 的 `exp` 到期前 5 分钟）并把新 token 只写回 `auth.json`。apim 那份
`~/.config/apim/openai-oauth.json` 就此落后。

**为什么现在不做**：服务端通常不轮换 refresh token（轮换时 apim 那份才真的失效），代价只是
「codex 刷新过之后 apim 的额度查询要用旧 access token 自己再刷一次」；真要同步得做
compare-and-swap（cc-switch 那套：比对 auth.json 里还是不是我们写进去的 refresh token 再回写），
是个独立功能。

**要做的**：在 `refresh_active_keys` 的节奏上顺带回读 `auth.json`，若它带着 ChatGPT 凭据且
refresh token 比 apim 那份新，就回写 apim 的凭据（写前校验 ownership，别把用户自己 `codex login`
的另一个账号抄进 apim）。

## 3. 评审留下的报告级小账（都只在这份手改/边缘配置里出现，暂不做）

来自两轮子代理评审（见 `target/apim-review/`）的 P2，父会话决定**只记账**，不在这轮改：

- **AUTH 行的提示与厂商类型无关**：手改 `~/.config/apim/recipes/openai.yaml` 为 `kind: non_model` 后，
  在那行按 `c/i/d/m/e` 会提示「只支持 x」，而 `x` 又按约定 15 被拒（`open_import` 的非模型门）。
  真要修就在 `App::note_auth_row_only_imports`（`src/app/mod.rs`）里先看 `is_model()`，
  非模型走「非模型厂商…」那套文案，并补一条测试。
- **密钥过滤命中为空时**，AUTH 行仍是唯一选中行（此时按 `x` 会真的写 Codex），而空面板提示写着
  「没有匹配的密钥」。提示本身没说错（AUTH 不是密钥），但两句看着矛盾。要修就改
  `ui/keys.rs` 的空面板文案（或过滤生效时不把 AUTH 当选中位）—— **别改选择模型**，
  现有 `clamp_selections_keeps_the_auth_row_selected` 是照着当前口径钉的。
- **摘掉三个键时，挂在它们上面的注释一起没了**（`toml_edit::DocumentMut::remove` 连 decor 一起删）。
  README 的「其余不动（含注释与顺序）」说的是别的键。要真保留得把 prefix 挪到下一个键上，
  但那条注释本来就说的是被删掉的键，搬过去反而误导 —— 倾向保持现状，只在文档里别过度承诺。
- **`verify_login` 绑死 codex 的措辞**：将来 codex 换掉 `Logged in using ChatGPT` 会硬失败 + 回滚
  （安全但用户可见）。真出问题就在 `clients/codex/official.rs` 的 `LOGGED_IN_MARKER` 上加一条
  备选措辞，并同步真机 opt-in 测试 `codex_real_official_end_to_end`。
- **CLI `import-codex` 不区分 ambiguous 守护进程**：TUI 会说「有 N 个类似进程没敢动，若它正开着
  请手动重启」，CLI 只说「重开 Codex 生效」。要修就把 `app/mod.rs` 那段文案镜像到 `cli/auth.rs`。
  顺带：`App::apply_oauth_codex` 的四个重启分支与 CLI 的 happy path 目前没有测试。
