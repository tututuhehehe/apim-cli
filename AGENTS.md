# AGENTS.md — apim 项目说明

## 项目背景

终端里的模型厂商 API Key 管理器（TUI + CLI）：本地一个二进制管全部 Key —— 增删改查、复制、探活、查额度，后续以 CLI 子命令接入 Alfred Workflow。面向用户的部分（安装、按键、CLI）见 `README.md`。

## 技术选型（已定，勿改）

- **Rust**：单二进制，Alfred 调用要秒开。
- **可扩展性靠数据不靠代码，范围只到厂商协议**：加一个中转站 = 丢一个 YAML（怎么鉴权 / 探活 / 查额度 / 展示），不重新编译。**只管同一套协议内的厂商差异**；跨协议与客户端差异各写 Rust 子模块（见约定 2、13），不做 DSL。

## 目录结构（按功能单元拆分，单文件 ≤ ~300 行）

> 树的**覆盖口径**（`tests/docs.rs` 的守卫按它查，三层）：
> 1. 树里写下的每个路径都必须**真实存在**；
> 2. `src/` 下每个**非测试**源文件都要被点名（测试代码按目录归拢：`tests/` 目录或 `tests.rs`）；
> 3. 仓库根下每个**非隐藏目录**（除 `target/`）都要被点名。
> 树承载「职责一句话」，不是 `ls` 的复制品 —— 目录内部的文件不必逐一亮相，亮相了就必须存在。

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
│   ├── codex/         Codex 适配（mod.rs 门面；契约见 `docs/clients/codex.md`）
│   │   ├── import.rs  写入编排 + 回滚
│   │   ├── official.rs 官方路（ChatGPT 登录）导入
│   │   ├── restart.rs RestartReport、找 codex daemon 并重启
│   │   ├── config_file.rs  ~/.codex/config.toml 保注释读写 + 备份 + 原子写
│   │   ├── catalog.rs 模型目录生成与校验 + 定位 codex 可执行文件
│   │   ├── active.rs  回读现场：现在在用哪把密钥（★）
│   │   └── tests/     沙盒测试（按源码文件分）+ 真机 opt-in（`--ignored codex_real_end_to_end`）
│   └── pi/            Pi 适配（mod.rs 门面；契约见 `docs/clients/pi.md`）
│       ├── import.rs  写入编排 + 回滚
│       ├── config.rs  models.json 读写（备份 + 原子 600）；auth.json 只读
│       ├── verify.rs  跑 `pi` 自己校验勾选的模型
│       ├── active.rs  回读 auth.json + models.json 的凭据：现在在用哪把密钥（★ 角标）
│       └── tests/     沙盒测试（假 pi 脚本）+ 真机 opt-in（`--ignored pi_real_end_to_end`）
├── cli/               CLI 子命令（AI/脚本的机器接口，与 TUI 共用底层）
│   ├── mod.rs         Args 解析（--flag 值/布尔）、Ctx（config+recipes 目录，可注入测试）、分发与帮助
│   ├── provider.rs    provider ls/add/set/rm/copy（--script 绑定/解绑、--kind 只在 add；copy 整份复制含脚本文件）
│   ├── auth.rs        OpenAI Codex OAuth 登录/状态/退出命令
│   ├── keys.rs        key ls/add/set/rm（token 只走 stdin，不进 argv）
│   ├── query.rs       status（并发探活+额度，--json）/ copy / use
│   ├── uninstall/     apim uninstall：mod.rs 流程 / cleanup.rs 删程序（含软链）与 --purge 配置目录 / report.rs 人机两套输出
│   ├── update/        apim update：mod.rs 流程 / channel.rs 认渠道 / install_sh.rs 下载+校验+执行 / http.rs 取 tag
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
tests/                文档守卫（仓库级不变量：目录树 ↔ `src/`），跑 `cargo test` 即执行
docs/                 每个文档的角色与归属见下方 Doc map（每类信息只有一个家）
README.md             英文说明（默认，GitHub 首页）
README.zh-CN.md       中文说明（与英文版内容同步）
install.sh            一行安装脚本（macOS/Linux：下载 Release 二进制 + 校验 sha256）
npm/                  npm 分发（平台子包模型）：bin/apim.js 主包 shim
scripts/              publish-npm.mjs（从 Release 资产组装并发布 6 个 npm 包）
.github/workflows/    ci.yml（检查 + msrv）、release.yml（发 5 平台）、publish-npm.yml（自动发 npm）
recipes/              内置 recipe ×4（deepseek/openai/moonshot/openrouter，include_str! 编译进二进制）
```

## 运行时数据（都在仓库外）

- `~/.config/apim/config.toml` — 密钥清单（provider/alias/group，无 token）
- `~/.config/apim/secrets.toml` — API token，键名 `"厂商.别名"`，600 权限
- `~/.config/apim/openai-oauth.json` — apim 自己的 OpenAI Codex OAuth 凭据，600 权限。伴生：`openai-oauth-host-id`（稳定 host ID）、`openai-oauth.log`（最近一次登录 / 刷新的逐步诊断：状态码 + 遮罩后的响应体，不含 token；登录失败也写完整错误链）
- `~/.config/apim/recipes/*.yaml` — 用户厂商 recipe，同 id 覆盖内置；非模型厂商靠 `kind: non_model` 标识（缺省即模型，语义见 `docs/provider-kinds.md`）
- `~/.codex/config.toml`、`~/.codex/apim-models.json` — 一键导入到 Codex（`x` 键）写的（见 `docs/clients/codex.md`）
- `~/.pi/agent/models.json` — 一键导入到 Pi（`x` 键）写的；`PI_CODING_AGENT_DIR` 可改整个目录（见 `docs/clients/pi.md`）
- `APIM_CONFIG_DIR` 环境变量可重定向整个配置目录（测试用）

★ 不落在 apim 自己的文件里：密钥行的 ★ 是**回读各客户端配置现场**算出来的（codex 见约定 11、pi 见约定 14）。

## 核心约定

1. **密钥永不进仓库**。`.gitignore` 已排除 secrets.toml/.env；写文档、注释、提交信息时一律用 `sk-...` 占位。动手前 `grep -r "sk-"` 扫一遍。
2. **加厂商不改 Rust，额度一律走脚本**。厂商表单只配：ID/名称/Base URL/主页/**非模型勾选框**/探活路径/**脚本路径**（指向 ~/.config/apim/scripts/ 下可执行脚本）；编辑时路径没改就保留手写配置，清空即取消。每个厂商一个额度脚本（`balance.kind: script`，env 注入 APIM_TOKEN/APIM_VAR_*，stdout 逐行直显），声明式 http 解析已退役；不要再扩 DSL，也不要往表单加预设类型（厂商类型就两种，见约定 15）。AI 代写脚本的标准提示词在 docs/quota-script-prompt.md。
3. **Recipe 覆盖顺序**：builtin → `<repo>/recipes/`（开发时）→ `~/.config/apim/recipes/`，同 id 后者覆盖前者；`origin: None` = 内置（不可删，只可编辑覆盖）。
4. **模块路径稳定**：子模块类型经 mod.rs re-export（如 `crate::app::Modal`），拆文件不破坏外部 import。二进制 crate 里**没人用的 `pub use` 会被 `unused_imports` 判成警告**，所以 mod.rs 只再导出模块外真的在用的东西。
5. **改完必跑**：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`。UI 改动还要跑快照核对 —— 命令与各 `APIM_SNAPSHOT_*` 开关见下面的「验证命令速查」（唯一家）。
6. **添加功能前先确认 git 状态，全程用 git 管理便于回退**。动手前 `git status` 看工作区：有未提交的旧改动就先提交或 `git stash`，别和新功能混在一起；`git log --oneline -3` 确认当前在哪个提交上，心里有可回退的锚点。功能完成（fmt+clippy+test 通过）后一次性提交：先 `git status` + `git diff --stat` 核对只包含本次功能相关文件（不混入 secrets/临时文件），再提交。要回退用 `git checkout <提交号> -- <路径>`（局部）或 `git revert`（整体）。
7. **README 默认英文**（`README.md`），中文版 `README.zh-CN.md` **必须同步改**；顶部语言切换链接别删。
8. 提交信息中文，一行主题 + 要点列表；功能一次一提交。
9. **发版走 `docs/RELEASING.md`**：版本号单一来源是 `Cargo.toml`；打 `v*` tag 触发 5 平台构建，Release 转正后由 `release.yml` **显式 dispatch** `publish-npm.yml`，npm 自动跟随发布（**不能靠 `on: release: [published]`** —— 原因（GITHUB_TOKEN 的事件不再触发 workflow，实测 v0.1.4 踩过）与替代方案见 RELEASING 里那条教训）；Homebrew 仍按手册手动更新；不要手改 Release 资产。
10. **开发跑本地代码一律 `cargo run -- <args>`，别 `cargo install --path .`**：本机 `apim` 是 npm 装的正试版（`/opt/homebrew/bin/apim`），PATH 里 `~/.cargo/bin` 排在它之后 —— `cargo install` 出的二进制不会被 `apim` 命中，只会变成过期副本让人误判「改了没生效」。

11. **一键导入到 Codex（`x` 键）**：契约细节（`config.toml` 怎么写、模型目录条目、官方路 `auth.json`、`★` 回读、`AUTH` 行、内置 `openai` 不可复制）与真机实测坑见 `docs/clients/codex.md` —— 改这条路之前先读它。红线：含密钥的落盘（`config.toml` 与它的 `.apim.bak`）一律 **600，且建文件时即 600**。
12. **`apim update` 只认三条渠道**（install.sh / npm / Homebrew，见 `docs/RELEASING.md` 的速查表）：`src/cli/update.rs` 按可执行文件路径认渠道（npm 看 `node_modules/apim-cli`、brew 看 `Cellar/apim`，其余当 install.sh 装的裸二进制），裸二进制那条复用官方 install.sh，但**不是 `curl | sh`**：URL 钉到本次要更新到的 tag、下载到临时文件、先做形状校验（是 shell 脚本 / 是本仓库安装器 / 含 sha256 校验）、再按 Release 发布的 `install.sh.sha256` 校验摘要（**拿不到摘要就拒绝执行**），最后用 `sh <file>` 跑；`APIM_INSTALL_DIR` 钉在当前二进制的**真实位置**（先 canonicalize，否则符号链接会被替换掉）保证原地更新。**`target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本一律不更新**（前者会被 Release 覆盖掉开发二进制，后者是 `cargo install` 留下的、被 PATH 遮挡的多余副本）。**npm 渠道更新前要同时核对主包与**当前平台子包**的版本**（`npm view <pkg> version`）：npm 的发布是异步的、主包会先可见，而 npm 对 optional 依赖失败是静默跳过 —— 只看主包就会装出一个跑不起来的 shim（v0.1.4 实测）。落后于 GitHub tag 时报出两个版本号并拒绝安装（`--force` 可越过）。加渠道要同时改 `Channel` 与它的识别规则、测试和 RELEASING 的表；`apim uninstall` 复用同一套 `detect_channel`，新渠道的卸载动作会被 `uninstall_program` 的穷尽 `match` 拦下（编译器逼你补），但提示语与单测仍要手工过一遍。

13. **加一个客户端（Claude Code / pi …）就是三处改动**，别在面板里写客户端专属分支：
    - `src/clients/mod.rs`：加 `Agent` 变体（所有 `match` 会被编译器强制补全）+ 一条分派（`label/badge/config_hint/reload_hint/default_model_step/active_key_ids/import/needs_reload/reload`）；`ImportRequest` / `ImportReport` 是共用的（面板只读这两个形状，不认客户端细节）；
    - `src/clients/<id>/`：新子模块，实现「写哪里 / 怎么写 / 写完后怎么校验 / 怎么重新加载 / 怎么从自家配置里认出正在用的密钥」—— **契约细节写进 `docs/clients/<id>.md`，模块文档只留一行指针**；公共件在 `clients/file_io.rs`（跟随符号链接 + 写前备份 + 原子 600）与 `clients/lock.rs`（导入排他锁），别重写一遍；
    - `app/import` 与 `ui/import` **不用改**：它们只经 `Agent` 的这套方法调客户端（唯一例外是内置 `openai` 的 `AUTH` 行，见约定 11）。密钥行的 ★ 也按客户端通用：`Agent::active_key_ids` 各自回读现场、`Agent::badge()` 拼角标（注册表自检测试守着「角标不许撞车」）；「选默认模型」那一步按客户端可关（`default_model_step()` 返回 `None` 就跳过，只勾一个模型时也自动跳过）。
    - **不要**给客户端造 YAML 配方（约定 2 的数据化范围是厂商协议）；客户端之间不是同一套协议，各写 Rust 更直白。

14. **一键导入到 Pi（`x` 键）**：契约细节（`models.json` 写什么、`apim-` 前缀、`pi --list-models` 校验、`★` 怎么认、`auth.json` 为什么只读）与真机实测坑见 `docs/clients/pi.md` —— 改这条路之前先读它。两条不许破的红线：**`settings.json` 一个字都不动**、Pi 的 `auth.json` **只读不写**。
15. **厂商类型 = 两个分页，类型创建时定死**（模型 / 非模型）：分页、表单、快捷键、状态口径上对非模型的差异与红线见 `docs/provider-kinds.md` —— 改分页 / 厂商表单 / 非模型行为之前先读它。
16. **待办写在 `docs/TODO.md`**：已知缺口连同「现状 / 为什么现在不做 / 真要动哪些地方」记在那里，别只留在脑子里；做了就从台账删掉。

## 验证命令速查

```bash
cargo run                              # 进 TUI（跑当前代码）
cargo run -- --snapshot                # 真实接口拉数据渲染成文本（不进 TUI）
APIM_SNAPSHOT_TAB=non-model cargo run -- --snapshot          # 非模型分页的主界面快照
APIM_SNAPSHOT_KIND=non-model cargo run -- --snapshot-provider-form   # 添加表单：勾上「非模型」的样子
APIM_SNAPSHOT_PROVIDER=<id> cargo run -- --snapshot-provider-form   # 编辑表单（ID 与类型只读；非模型没有探活那一行）
cargo run -- --snapshot-form           # 密钥表单弹窗快照
cargo run -- --snapshot-inspector      # 详情弹窗快照：假状态不拉接口（APIM_SNAPSHOT_INSPECTOR=provider 出厂商详情）
cargo run -- --snapshot-import         # 一键导入面板快照：第一步选客户端
cargo run -- --snapshot-import-models  # 一键导入面板快照：第二步勾选模型
cargo run -- --snapshot-import-default # 一键导入面板快照：第三步选默认模型
cargo test                             # 单测（recipe/表单/CLI 沙盒/codex+pi 适配等）
cargo +1.88 check --locked --all-targets   # 声明的 MSRV 要真能编（CI 的 msrv job 跑同一条）
cargo test -- codex_real_end_to_end --ignored --nocapture   # 需本机装 codex：真机端到端（生成目录 + codex 校验）
cargo test -- codex_real_official_end_to_end --ignored --nocapture  # 需本机装 codex：官方路（合成凭据；不碰真实 ~/.codex）
cargo test -- pi_real_end_to_end --ignored --nocapture      # 需本机装 pi：真机端到端（写两份 JSON + 让 pi 列模型）
APIM_SNAPSHOT_AGENT=pi cargo run -- --snapshot-import-models   # 换客户端出快照（默认第一个；「选默认模型」只对 codex 有）
cargo run -- update --check                # 认安装渠道 + 报当前/最新
apim provider ls --json                # CLI 冒烟（跑已发布版；本地代码见约定 10）
```

## Agent skills

### Doc map — 每类信息只有一个家，其它位置只放指针

| 信息 | 唯一位置 |
|---|---|
| 进行中的特性（spec + ticket） | `.scratch/<feature-slug>/`（规格见 `docs/agents/issue-tracker.md`） |
| 已知缺口 / 待做 / 待验 + 第一步 | `docs/TODO.md`（条目编号 `TODO-N`） |
| 已定的取舍、以及「故意不做 + 理由」 | `docs/adr/NNNN-*.md`（索引见 `docs/adr/README.md`）—— 防止重开已决的事 |
| 已完成的历史 | `CHANGELOG.md` |
| 面向用户的使用手册（CLI / 按键 / recipe / 额度脚本） | `README.md`；中文版 `README.zh-CN.md` 同源翻译 |
| 客户端契约细节（Codex / Pi 的 TOML、JSON 形状） | `docs/clients/*.md`（一个客户端一份） |
| 厂商类型（模型 / 非模型）的语义与不适用面 | `docs/provider-kinds.md` |
| 项目技能（面向 agent 的用法包） | `.agents/skills/*/SKILL.md` |
| 领域文档 / 工单规格（agent 开工前先读什么） | `docs/agents/*.md` |
| 发版手册 / 额度脚本提示词 | `docs/RELEASING.md`、`docs/quota-script-prompt.md` |
| 本机私有笔记（只在本机有效、不宜进仓库） | `DEV-NOTES.local.md`（gitignored） |
| 会话交接 | 不落库：`/skill:handoff` 写到 `$TMPDIR` |

开工一个 `TODO-N` 时：`/skill:to-spec` 把它固化成 `.scratch/<slug>/spec.md` → `/skill:to-tickets` 拆成带 `Blocked by:` 的 ticket（一票一文件）→ `/skill:implement` 一次一张。收尾时结论进 `CHANGELOG.md`，遗留项回 `docs/TODO.md`。
