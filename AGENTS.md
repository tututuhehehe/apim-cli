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
│   └── codex/         Codex 适配（mod.rs 只是门面：子模块声明 + 再导出 + codex_home/config_hint）
│       ├── import.rs  ImportRequest/ImportReport、保留 id 加前缀、base_url 补 /v1、写入编排 + 回滚
│       ├── lock.rs    codex home 排他锁（并发导入不互相覆盖）
│       ├── restart.rs RestartReport、按进程表找 codex daemon 并重启（严格匹配 argv[2]）
│       ├── config_file.rs  ~/.codex/config.toml 读改写（toml_edit 保注释保顺序）+ 备份 + 原子写
│       ├── catalog.rs 模型目录：手写官方迷你条目（~1KB/模型）+ `codex debug models` 端到端校验
│       ├── store.rs   ~/.config/apim/codex.toml（★ 标记用的「当前导入项」）
│       └── tests/     沙盒测试（按源码文件分）+ 真机 opt-in（`--ignored codex_real_end_to_end`）
├── cli/               CLI 子命令（AI/脚本的机器接口，与 TUI 共用底层）
│   ├── mod.rs         Args 解析（--flag 值/布尔）、Ctx（config+recipes 目录，可注入测试）、分发与帮助
│   ├── provider.rs    provider ls/add/set/rm/copy（--script 绑定/解绑；copy 整份复制含脚本文件）
│   ├── keys.rs        key ls/add/set/rm（token 只走 stdin，不进 argv）
│   ├── query.rs       status（并发探活+额度，--json）/ copy / use
│   ├── uninstall/     apim uninstall：mod.rs 流程 / cleanup.rs 删程序（含软链）与 --purge 配置目录 / report.rs 人机两套输出
│   ├── update/        apim update：channel.rs 认渠道 / install_sh.rs 下载+校验+执行 / http.rs 取 tag
│   └── tests.rs       CLI 沙盒测试（临时目录全流程）
├── form/              通用表单引擎（密钥表单、厂商表单共用）
│   ├── mod.rs         Field（文本/选择/只读）、Form、按键分发、表单构造器
│   ├── edit.rs        LineEdit：单行编辑（值 + 光标）
│   └── tests.rs       表单引擎测试
├── app/               应用状态机
│   ├── mod.rs         App 结构、start、导航、探活调度（探针代际：配置变更后旧结果丢弃）
│   ├── import/        一键导入面板：mod.rs 流程控制 + flow.rs 状态 + apply.rs 写盘回执 + keys.rs 按键
│   │                  （tests/ 按 flow / apply 分）
│   ├── modal.rs       Modal 枚举 + 打开/保存分发/删除确认分发
│   ├── keys_store.rs  密钥保存/删除（写 config.toml + secrets.toml）
│   ├── undo.rs        Ctrl+Z 撤销栈（本次会话的写操作）+ 回退内存与磁盘
│   └── providers_store.rs  厂商保存/删除/整份复制（recipe YAML + 额度脚本文件）
├── ui/                一个面板一个文件
│   ├── mod.rs         draw 分发 + theme + pane_block/centered + 公共零件（搜索框/滚动偏移）
├── util.rs            跨模块小工具（truncate 等；只放「多处各写了一遍」的东西）
│   ├── header.rs      顶栏/底栏（底栏按焦点显示 c 复制什么）
│   ├── providers.rs   左栏厂商列表
│   ├── keys.rs        右侧密钥表 + 状态标签
│   ├── balance.rs     右下额度面板
│   ├── form_modal.rs  表单弹窗（光标截断渲染）
│   ├── import.rs      一键导入面板渲染
│   └── confirm.rs     删除确认弹窗
├── recipe/            厂商协议
│   ├── mod.rs         Recipe/Auth/HttpCall 模型、YAML 加载（builtin→manifest→user 逐级覆盖，加载期校验 id 字符集）、is_valid_id、{token}/{base_url} 模板替换
│   ├── script.rs      ScriptSpec（balance.kind=script，command/run 二选一，自定义 serde 校验）
│   ├── dup.rs         厂商整份复制（新 id 自动顺延 + 额度脚本文件副本）
│   └── store.rs       用户 recipe 读写（~/.config/apim/recipes/*.yaml）
├── config/            密钥清单
│   ├── mod.rs         KeyEntry、读取 config.toml + secrets.toml（严格版给 TUI，宽松版 load_keys_lenient 给 CLI 自救）
│   └── store.rs       原子写入（tmp+rename，tmp 名带 pid，600 权限）
└── probe/             并发探活（health + balance 并发，tokio::join!）
    ├── mod.rs         Health/ProbeResult、client、http 一路（探活 + 模型列表拉取的鉴权请求）
    └── script.rs      脚本执行器（env 注入/超时 kill/stderr 截断 200/stdout 50 行上限）+ expand_tilde
docs/
├── quota-script-prompt.md  额度脚本代写提示词（整体复制给 AI Agent 用）
└── RELEASING.md        维护者发布手册（发版、npm、Homebrew、回滚）
README.md             英文说明（默认，GitHub 首页）
README.zh-CN.md       中文说明（与英文版内容同步）
install.sh            一行安装脚本（macOS/Linux：下载 Release 二进制 + 校验 sha256）
npm/                  npm 分发（平台子包模型）：bin/apim.js 主包 shim
scripts/              publish-npm.mjs（从 Release 资产组装并发布 6 个 npm 包）
.github/workflows/    ci.yml（fmt/clippy/test/JS 检查）、release.yml（tag 发 5 平台二进制）、publish-npm.yml（手动发 npm）
recipes/              内置 recipe ×4（deepseek/openai/moonshot/openrouter，include_str! 编译进二进制）
├── deepseek.yaml
├── openai.yaml
├── moonshot.yaml
└── openrouter.yaml
```

## 运行时数据（都在仓库外）

- `~/.config/apim/config.toml` — 密钥清单（provider/alias/group，无 token）
- `~/.config/apim/secrets.toml` — token，键名 `"厂商.别名"`，600 权限
- `~/.config/apim/recipes/*.yaml` — 用户厂商 recipe，同 id 覆盖内置
- `~/.config/apim/codex.toml` — apim 最后一次一键导入到 Codex 的记录（只用于 ★ 标记与面板提示，不反向决定写什么）
- `~/.codex/config.toml`、`~/.codex/apim-models.json` — 一键导入（`x` 键）写的，前者每次改写前备份成 `config.toml.apim.bak`
- `APIM_CONFIG_DIR` 环境变量可重定向整个配置目录（测试用）

## 核心约定

1. **密钥永不进仓库**。`.gitignore` 已排除 secrets.toml/.env；写文档、注释、提交信息时一律用 `sk-...` 占位。动手前 `grep -r "sk-"` 扫一遍。
2. **加厂商不改 Rust，额度一律走脚本**。厂商表单只配：ID/名称/Base URL/主页/探活路径/**脚本路径**（指向 ~/.config/apim/scripts/ 下可执行脚本）；编辑时路径没改就保留手写配置，清空即取消。每个厂商一个额度脚本（`balance.kind: script`，env 注入 APIM_TOKEN/APIM_VAR_*，stdout 逐行直显），声明式 http 解析已退役；不要再扩 DSL，也不要往表单加预设类型。AI 代写脚本的标准提示词在 docs/quota-script-prompt.md。
3. **Recipe 覆盖顺序**：builtin(include_str) → `<repo>/recipes/`（开发时）→ `~/.config/apim/recipes/`，后读的同 id 覆盖先读的。`origin: None` = 内置，不可删除只可编辑覆盖。
4. **模块路径稳定**：子模块类型经 mod.rs re-export（如 `crate::app::Modal`），拆文件不破坏外部 import。
5. **改完必跑**：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`。UI 改动跑 `cargo run -- --snapshot`（主界面）/ `--snapshot-form` / `--snapshot-provider-form` / `--snapshot-inspector` 出纯文本渲染核对。
6. **添加功能前先确认 git 状态，全程用 git 管理便于回退**。动手前 `git status` 看工作区：有未提交的旧改动就先提交或 `git stash`，别和新功能混在一起；`git log --oneline -3` 确认当前在哪个提交上，心里有可回退的锚点。功能完成（fmt+clippy+test 通过）后一次性提交：先 `git status` + `git diff --stat` 核对只包含本次功能相关文件（不混入 secrets/临时文件），再提交。要回退用 `git checkout <提交号> -- <路径>`（局部）或 `git revert`（整体）。
7. **README 默认英文**（`README.md`），中文版在 `README.zh-CN.md`，两版内容保持同步：改一版必须同步另一版，顶部语言切换链接别删。
8. 提交信息中文，一行主题 + 要点列表；功能一次一提交。
9. **发版走 `docs/RELEASING.md`**：版本号单一来源是 `Cargo.toml`；打 `v*` tag 触发 5 平台构建；npm/Homebrew 按手册各自更新；不要手改 Release 资产。
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
    - 厂商 id 撞上保留名（`openai`/`ollama`/`lmstudio`/`amazon-bedrock*`）时加 `apim-` 前缀。

12. **`apim update` 只认三条渠道**（install.sh / npm / Homebrew，见 `docs/RELEASING.md` 的速查表）：`src/cli/update.rs` 按可执行文件路径认渠道（npm 看 `node_modules/apim-cli`、brew 看 `Cellar/apim`，其余当 install.sh 装的裸二进制），裸二进制那条复用官方 install.sh，但**不是 `curl | sh`**：URL 钉到本次要更新到的 tag、下载到临时文件、先做形状校验（是 shell 脚本 / 是本仓库安装器 / 含 sha256 校验）、再按 Release 发布的 `install.sh.sha256` 校验摘要（**拿不到摘要就拒绝执行**），最后用 `sh <file>` 跑；`APIM_INSTALL_DIR` 钉在当前二进制的**真实位置**（先 canonicalize，否则符号链接会被替换掉）保证原地更新。**`target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本一律不更新**（前者会被 Release 覆盖掉开发二进制，后者是 `cargo install` 留下的、被 PATH 遮挡的多余副本）。加渠道要同时改 `Channel` 与它的识别规则、测试和 RELEASING 的表；`apim uninstall` 复用同一套 `detect_channel`，新渠道的卸载动作会被 `uninstall_program` 的穷尽 `match` 拦下（编译器逼你补），但提示语与单测仍要手工过一遍。

13. **加一个客户端（Claude Code / pi …）就是三处改动**，别在面板里写客户端专属分支：
    - `src/clients/mod.rs`：加 `Agent` 变体（所有 `match` 会被编译器强制补全）+ 一条分派；
    - `src/clients/<id>/`：新子模块，实现「写哪里 / 怎么写 / 写完后怎么校验 / 怎么重新加载」；
    - `app/import` 与 `ui/import` **不用改**：它们只经 `Agent` 的 `import/remember/reload/last_import/label/config_hint/note` 调客户端，文案全取自 `Agent`。
    - 例外（已知）：密钥行的 ★ 目前读 `App::last_imports` 里 Codex 那一条，`Agent::last_import` 已经通用，加客户端时只需确认 ★ 是否要一起显示多客户端。
    - **不要**给客户端造 YAML 配方（约定 2 的数据化范围是厂商协议）；客户端之间不是同一套协议，各写 Rust 更直白。
    - 「不需要选模型的客户端」暂时还得先做一步重构：`ImportRequest.models/default_model` 现在是必填，要先改成 `Option` 才能让面板跳过勾选模型两步（见 `DEV-NOTES.local.md`）。

## 验证命令速查

```bash
cargo run                              # 进 TUI（跑当前代码）
cargo run -- --snapshot                # 真实接口拉数据渲染成文本（不进 TUI）
cargo run -- --snapshot-inspector      # 详情弹窗快照：假状态不拉接口；=provider 出厂商详情
cargo run -- --snapshot-import         # 一键导入面板快照：第一步选客户端
cargo run -- --snapshot-import-models  # 一键导入面板快照：第二步勾选模型
cargo run -- --snapshot-import-default # 一键导入面板快照：第三步选默认模型
cargo test                             # 单测（recipe/表单/CLI 沙盒/codex 适配等）
cargo test -- codex_real_end_to_end --ignored --nocapture   # 需本机装 codex：真机端到端（生成目录 + 让 codex 校验）
cargo run -- update --check                # 认安装渠道 + 报当前/最新（不动手）
apim provider ls --json                # CLI 冒烟（跑已发布版；本地代码用 cargo run -- provider ls）
```

> 开发期不要 `cargo install --path .`（见约定 10）。
