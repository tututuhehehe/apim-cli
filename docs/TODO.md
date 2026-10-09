# TODO — 已知缺口与后续要做的事

**角色**：本仓「长期台账」的唯一位置（见 `AGENTS.md` 的 Doc map）。记录**已知缺口 / 为什么现在不做 / 真要做得动哪些地方**，免得下次有人重新调研一遍；面向维护者，不是用户文档。

**编号**：条目用 `TODO-N`。开工照这个顺序走：

1. `/skill:to-spec` — 把这一条固化成 `.scratch/<feature-slug>/spec.md`（本仓 ticket 规格见 `docs/agents/issue-tracker.md`）
2. `/skill:to-tickets` — 拆成带 `Blocked by:` 的 ticket，一票一文件
3. `/skill:implement` — 一次一张，在预先约定的 seam 上驱动 tdd
4. 收尾 — 结论进 `CHANGELOG.md`，本文件里这一条**删掉**；没做完的遗留项留在本文件（能拆小就拆小）

**边界**：进行中的工作**不写在这里**（写 `.scratch/`）；`CHANGELOG.md` 记过的历史不在这里重复。

## TODO-1 · ~~OpenAI OAuth 凭据按厂商存~~ —— **已关闭（2026-10-09）**

**决议：不做。** OAuth（`AUTH` 行 + `x` 导入 Codex 官方路）**只服务内置 `openai`** 这个特殊通道，凭据保持单份、继续禁止复制内置 `openai`（因为复制牵扯较多）。完整背景与「何时该重开」见 **`docs/adr/0008`**。

下面保留当时调研出的现状与改动清单，**只作留档，不要再按它动手**。

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

## TODO-2 · 官方路导入不回写同步 codex 刷新的 token

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

## TODO-3 · 评审留下的报告级小账（都只在这份手改/边缘配置里出现，暂不做）

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

---

## 开放工作（自原 `DEV-NOTES.local.md` §3 迁入，2026-10-09）

编号接续 `TODO-1`~`TODO-3`；每条保留原有的「现状 / 第一步」，括号里是原编号，便于对照历史。

## TODO-4 · 【待验】install.sh 渠道的完整 happy path（原 §3.1）

**现状**：最后一段（摘要校验通过 → 执行 install.sh）还没真机跑过：最新的 `v0.1.1` 早于 CI 改动、没有 `install.sh.sha256` 资产（实测 404 → 正确地 fail closed）。

**第一步**：等第一个带该资产的 Release 发出来后，把一个「裸二进制」放在 `~/.local/bin/apim`，跑 `apim update --force`，确认输出 `sha256 校验通过` 且二进制被原地替换、版本更新。

## TODO-5 · 客户端 2 落地时的第一步：`models` / `default_model` 改 `Option`（原 §3.2）

**现状**：想让面板跳过「勾选模型 / 选默认模型」两步（Claude Code 这类没有模型清单的客户端），必须先改数据类型。

**第一步**：

- `ImportRequest.models: Vec<String>` + `default_model: String` → `models` 允许空、`default_model: Option<String>`
- `ImportReport.model: Option<String>`、`reasoning_effort: Option<String>`（否则适配器只能塞假值并印进 toast）
- 然后才加 `Agent::needs_models() -> bool`，让 `import_choose_agent` 与 `import_confirm_models` 按它分支
- **现在不加这个开关**：`needs_models` 恒为 true 的假接口比没有更糟（false 分支造不出合法请求）

## TODO-6 · 把「提示文本」也下沉到适配层（原 §3.3）

**现状**：`app/import/mod.rs::import_result` 仍直接格式化 `report.models/model/reasoning_effort/provider_key/backup_path`（codex 形状），并拼 codex 的重启语义。

**第一步**：改成由适配层产出成品提示（例如 `Agent::success_note(request, report) -> String`），app 层不再认识这些字段。**等 TODO-5 一起做**更省。

## TODO-7 · `ImportReport` 的 codex 语义字段（原 §3.4）

**现状**：`provider_key` / `backup_path` 对别的客户端不成立（现在只有 codex，无害）。

**第一步**：与 TODO-5 / TODO-6 同批处理。

## TODO-8 · 密钥行 ★ 的多客户端显示（原 §3.5）

**现状**：`ui/keys.rs` 现在只问 `is_active(Agent::Codex, …)`；`Agent::last_import` 与 `App.last_imports` 已经通用。

**第一步**：加客户端时决定是否显示「★ 导入给 X」多标记。

## TODO-9 · `apim import <agent>` CLI 入口（原 §3.6）

**现状**：`SKILL` 里写明「TUI 才有导入，暂无 CLI」。Alfred / 脚本要能调。

**第一步**：加第二客户端时一并做更划算（可以复用 `Agent::import`）。

## TODO-10 · 【待验】npm / Homebrew 渠道的卸载真机跑一遍（原 §3.7）

**现状**：`npm uninstall -g apim-cli` / `brew uninstall apim` 会真的动全局安装；本机只在 install.sh 渠道做了 e2e（临时目录里的假裸二进制），另两条渠道只有纯函数的命令串断言。

**第一步**：`npm install -g apim-cli` 后跑 `apim uninstall --yes`，确认 `node_modules/apim-cli*` 与 `.bin` shim 都没了；brew 同理。Windows 上「`apim.exe` 被本进程占住 → npm EBUSY」的提示也只是按文件占用常识写的（本机无 Windows），一并验。

## TODO-11 · CI 加 Windows `cargo check`（原 §3.8）

**现状**：`ci.yml` 只在 ubuntu 跑 fmt/clippy/test，`cfg(windows)` 分支只有 `release.yml` 打 tag 时才被编译 → 编译错要到发版才暴露（背景见 `docs/adr/0004`）。

**第一步**：ubuntu runner 上 `rustup target add x86_64-pc-windows-msvc && cargo check --target x86_64-pc-windows-msvc`（`check` 不需要链接，能覆盖 `cfg(windows)` 的编译；`--all-targets` 还能捎带编 Windows 下的测试代码）。

## TODO-12 · OAuth 凭据的 last-writer-wins 竞争（登录任务 vs 在途探针）（原 §3.9）

**现状**：`save` 只串行化了「写」本身（`WRITE_LOCK` 只包住落盘），但 `fetch_usage` 是「load → 可能 POST 刷新 → save」的读改写：探针在登录落盘前 load、在登录落盘后 save，会把刚登进去的凭据覆盖回旧那份，而 toast 已经说「已连接」。

**触发条件**：自动刷新（5 分钟一档）正好落在登录窗口（≤10 分钟）内，且凭据已过期（才真的会 POST）。

**第一步**：`save` 里做 compare-and-swap（在 `WRITE_LOCK` 内重读，`client_id` + `refresh_token` 变了就跳过），或在 `oauth_login_running` 时不派发 OAuth 探针。评审结论：属报告项，不阻塞当时合并。

## TODO-13 · `refresh` 的档位门与落盘顺序没有测试守住（原 §3.10）

**现状**：`next_refresh_token` 与 `IdTokenIdentity` 这两个抽出来的 helper 有测试，但「refresh 只对非 codex 档位带 `resource`」和「新 token 先落盘再推导 account id」只有读代码验证——改回去不会有测试变红。

**第一步**：抽一个纯函数 `refresh_form(cred) -> Vec<(&str,&str)>` 再表驱动测它；落盘顺序可以抽「先 save 后 derive」的编排函数，用假 http 闭包注入（真守死要引 HTTP mock server）。

## TODO-14 · 超线文件拆分（原 §3.11 + §2.6）

**现状**：`src/openai_auth.rs` 已 1212 行，超过「单文件 ≤ ~300 行」约定（`AGENTS.md` 目录树已同步，但没拆）。既有超线（非某轮引入）：`app/mod.rs` 1751、`app/modal.rs` 869、`ui/inspector.rs` 717、`recipe/mod.rs` 543、`tui.rs` 468、`probe/mod.rs` 378、`app/providers_store.rs` 334；另有 `ui/import.rs` 307 行临界（见 `docs/adr/0007`）。

**第一步**：按现有缝拆 `openai_auth/{mod,profile,login,token,usage,log}.rs`（遵守 `AGENTS.md` 约定 4：`crate::openai_auth::*` 路径不变）；其余大文件另开条目。

## TODO-15 · brew 渠道长期落后 + 发布清单要把 brew 前置（原 §3.14）

**现状**：tap 历史 `apim 0.1.3` → 直接跳到 `0.1.7`，也就是 0.1.4~0.1.6 三次发版**漏了 brew 这步**（与 TODO-10 同源），brew 用户一直停在 0.1.3。

**第一步**：把「发版清单」里 brew 那步前置到 npm 验证之后立刻做（`scripts/update-tap.sh X.Y.Z`，且脚本现在下载失败会硬失败）；本机实测 brew 渠道要先把 npm 那份卸掉再 `brew install tututuhehehe/tap/apim`（两条渠道都往 `/opt/homebrew/bin/apim` 落，会撞）。发布教训已记进 `docs/RELEASING.md`。

**截至 v0.1.7**：brew formula 的 sha256 已与 Release 资产、本地下载三方核对一致。

---

## 已归位（不在本文件）

- **「故意不做 + 理由」与已定取舍** → `docs/adr/`（ADR-0001~0007，含 OAuth 的四个有意取舍与三处容忍的重复）
- **发布流程教训**（管道退出码、brew 漏更、npm 平台子包超时） → `docs/RELEASING.md`“踩过的坑与教训”
- **进行中的特性** → `.scratch/<feature-slug>/`（见 `docs/agents/issue-tracker.md`）
- **已完成的历史** → `CHANGELOG.md`
