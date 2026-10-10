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

**现状（2026-10-10，v0.1.9 发版时**部分**验证）**：已验到的 —— ① 裸二进制（放 `/tmp/apim-bare/apim`）被正确识别成 `install.sh（裸二进制）` 渠道并打印出路径；② 能解析出 `最新：v0.1.9`（v0.1.9 是第一个带 `install.sh.sha256` 资产的 Release，该资产已确认存在）；③ 下载失败时 **fail-closed**：二进制一字未动，报错明确（「查不到最新版本号，无法确定要校验哪个版本的 install.sh」），5 次重试都是这个结果。

**还没验到的**：`摘要校验通过 → 执行 install.sh → 原地替换` 那一段 —— 卡在本机到 `github.com` / `raw.githubusercontent.com` 的网络（同一窗口 `api.github.com` 与 `gh` 一切正常；**不是代码问题**）。

**第一步**（网络恢复后重跑）：`gh release download v0.1.8 -p 'apim-v0.1.8-aarch64-apple-darwin.tar.gz'` → 解包成裸二进制放 `/tmp/apim-bare/apim` → `/tmp/apim-bare/apim update --force` → 期望打印 `sha256 校验通过` 且版本变 0.1.9。**别放 `~/.local/bin`**：它在 PATH 里排在 `/opt/homebrew/bin` 之前，会遮住 npm 那份 `apim`（本次就是因此改用 `/tmp`，等价且不侵入）。

**现状（原）**：最后一段（摘要校验通过 → 执行 install.sh）还没真机跑过：最新的 `v0.1.1` 早于 CI 改动、没有 `install.sh.sha256` 资产（实测 404 → 正确地 fail closed）。

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

## TODO-14 · 超线文件拆分（原 §3.11 + §2.6）

**现状**：`src/openai_auth.rs` 已 1212 行，超过「单文件 ≤ ~300 行」约定（`AGENTS.md` 目录树已同步，但没拆）。既有超线（非某轮引入）：`app/mod.rs` 1751、`app/modal.rs` 869、`ui/inspector.rs` 717、`recipe/mod.rs` 543、`tui.rs` 468、`probe/mod.rs` 378、`app/providers_store.rs` 334；另有 `ui/import.rs` 307 行临界（见 `docs/adr/0007`）。

**第一步**：按现有缝拆 `openai_auth/{mod,profile,login,token,usage,log}.rs`（遵守 `AGENTS.md` 约定 4：`crate::openai_auth::*` 路径不变）；其余大文件另开条目。

## TODO-16 · 「写入闸跳过 → 重读重试一次」的编排没有自动化测试

**现状**：`.scratch/oauth-credential-sync/` 的票 02 引入的写入闸在「磁盘上那份已经不是我们读的那份」时会
跳过写盘，`fetch_usage` 据此重读并整体重试一次（上界写死为 2 次尝试）。**「跳过 → 重试」这条编排路径
没有测试覆盖**，只靠代码审查 + 那个上界；票 02 与票 04 的 `## Done` 都记了这个缺口。

**为什么现在不做**：要测它就得让刷新的 POST 与用量的 GET 可替换，而本仓至今没有 HTTP mock server
（这条代价原先就记在台账里，已随本轮结清）。红线（登录不被覆盖、注销不复活、0600、无新全局
状态）都有测试，缺的只是「重试那一跳」；代价是这一跳改错**不会变红**。

**第一步**：把「一次尝试」的编排抽成一个吃注入闭包的薄函数（先例：`cli/uninstall` 的
`run_with(args, exe, dir)` 注入点、`app/import` 的 runner 注入点），表驱动断言「跳过 → 再来一次」与
「连续两次跳过 → 本轮无读数、不报错」。真想要端到端再引一个只在测试里起的最小 HTTP server —— 那是
另一条更大的账，别搭在这条里。

## TODO-17 · 评审结论落在 `target/` 里（`cargo clean` 就没）

**现状**：`TODO-3` 开头的「来自两轮子代理评审（见 `target/apim-review/`）」指向构建产物目录 —— `target/`
在 `.gitignore` 里，一条 `cargo clean` 或换台机器就没了。committed 文档把可追溯性挂在 `target/` 上，
等于没有证据。

**为什么现在不做**：它不影响任何行为，只是证据链；当初的评审结论已经逐条落进 `TODO-3` 的正文，能追的
部分已经追回来了。

**第一步**：把那句改成「结论已内联在下面各条」；往后新评审的结论直接落到 `docs/`（或写进对应台账条目），
不再引用 `target/`。


## TODO-21 · `lru` 的 low 告警被 ratatui 0.29 挡住（需先升 ratatui 0.30）

**现状（2026-10-09 更新）**：rustls 那条**已修** —— `8593e08`（0.23.43 → 0.23.45，只动 `Cargo.lock`），验证 = 三命令全绿 + `cargo +1.88 check --locked --all-targets` 编过 + 真 TLS 的 `cargo test -- --ignored latest_tag_live` 通过；Dependabot PR #2 已按「被取代」关闭（它基于开工前的旧 main）；GitHub 告警 #3 已转 `fixed`。

**只剩 `lru`（low）一条 open**：

- `lru` 是 `ratatui 0.29.0` 的间接依赖，而 ratatui 硬要 `lru = "^0.12.0"` → **锁文件升不动**（实测 `cargo update -p lru --precise 0.16.3` 报 `failed to select a version`）。
- 修它得把 `Cargo.toml` 的 `ratatui = "^0.29"` 升到 **0.30**（0.30 已发布；0.31/0.32 不存在）—— 对一个 TUI 全靠 ratatui 的项目，这是一次**有 API 破坏面的升级**，必须用 `--snapshot*` 那套快照逐个核对渲染（约定 5）。

**为什么现在不做**：为一条 low 告警做一次 TUI 框架升级，风险/收益不对等，且不阻塞任何功能。

**第一步**（单独开一轮）：升 ratatui 0.30 → 跟着编译器改 API → `cargo run -- --snapshot` 全系列快照逐个比对 → 确认 `lru` 落到 ≥ 0.16.3、告警清零 → 三命令 + MSRV。

---

（以下保留当时的第一手调研原文，只作留档）

> **独立复核补充（2026-10-09，另一个 agent 只读复核 `8593e08`）** —— 三条值得留给下一个人的事实：
>
> 1. **rustls 0.23.45 抬了两个依赖下限**：`rustls-webpki` 0.103.5 → **0.103.14**、`aws-lc-rs` 1.14 → **1.18**（optional）。本次 `--locked` 能只改两行就过，是因为锁里 webpki 已经是 **0.103.15** ≥ 0.103.14、而 `aws-lc-rs` **根本不在我们树里**（provider 是 ring：`reqwest` 的 `rustls-tls` → `__rustls-ring`）。**别以为 rustls 的小版本升级永远只需要动两行** —— 下限不满足时得连 webpki 一起动。
> 2. `cli::update::http::tests::latest_tag_live` 实质是**网络**测试而不是 TLS 测试：`github.com` 不可达时它会**干等 30s 后超时**（同窗口 `curl` 连 TCP 都建不起来，而 `api.github.com` 正常）。它 `#[ignore]`、不进 CI 是对的；**谁把它接进 CI，谁就会得到一个慢速 flaky**。
> 3. 为 `lru` 开的那一轮要注意：最新 `ratatui` 是 **0.30.2**，其 `rust-version` = **1.88.0**，**正好等于本仓声明的 MSRV（零余量）**。

**现状（原）**：远端默认分支报 **2 条依赖漏洞**（1 moderate + 1 low，GitHub Dependabot alerts）；另有一条
dependabot 分支 `dependabot/cargo/rustls-0.23.45` 挂在远端（`rustls` 是 `reqwest` 的间接依赖，`Cargo.toml`
里没有直接声明）。本地 `main` 与远端同步、工作区干净。

**为什么现在不做**：还没看清告警的来源（哪个 crate / 哪条 advisory / 有没有 `fixed_in`），而 `rustls`
这种网络层依赖升级要跑全套测试 + MSRV `--locked` + 至少一遍真机 opt-in 路径（CI 的沙盒测试覆盖不到 TLS
握手）。在没看清之前动依赖，只会把一个「不确定」换成一个「不知道哪里坏」。

**第一步**（先查、不改）：
1. `gh api repos/tututuhehehe/apim-cli/dependabot/alerts --jq '.[] | {number,state,severity,summary:.security_advisory.summary,package:.dependency.package.name,fixed_in:.security_vulnerability.first_patched_version.identifier}'`
   —— 看清是哪两个包、哪条 advisory、下游有没有可用修复版本；
2. `git ls-remote --heads origin | grep rustls` 确认那条分支是「开着的 PR」还是遗留分支；
3. 若指向可直接升级的间接依赖：新开分支上 `cargo update -p <crate> --precise <fixed>` → 跑三命令 +
   `cargo +1.88 check --locked --all-targets`（MSRV 那步是 `--locked`，锁文件一动必须重跑）→ 再决定怎么
   处理那条 dependabot 分支（合并或关掉）。

---

## 已归位（不在本文件）

- **「故意不做 + 理由」与已定取舍** → `docs/adr/`（索引见 `docs/adr/README.md`；含 OAuth 的有意取舍与三处容忍的重复）
- **发布流程教训**（管道退出码、brew 漏更、npm 平台子包超时） → `docs/RELEASING.md`“踩过的坑与教训”
- **进行中的特性** → `.scratch/<feature-slug>/`（见 `docs/agents/issue-tracker.md`）
- **已完成的历史** → `CHANGELOG.md`
