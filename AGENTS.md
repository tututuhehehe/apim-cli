# AGENTS.md — apim 项目说明

## 项目背景

终端里的模型厂商 API Key 管理器（TUI + 未来的 CLI）。用户维护多家厂商 / 中转站的 API Key（DeepSeek 官方、各类 NewAPI 中转站等），痛点是 Key 复制来复制去、查额度要开网页。目标：本地一个二进制管全部 Key——增删改查、复制、探活、查额度；后续以 CLI 子命令接入 Alfred Workflow 作为统一入口。

## 技术选型（已定，勿改）

- **Rust**（用户本机有 rustc，无 Go）。产物是单二进制，Alfred 调用秒开。
- **可扩展性靠数据不靠代码**：厂商协议 = YAML recipe（怎么鉴权、怎么探活、怎么查额度、怎么展示）。加一个中转站 = 丢一个 YAML，不重新编译。只有协议特别歪才写 adapter（目前没有）。
- TUI 用 ratatui；HTTP 用 reqwest(rustls)；密钥存 TOML，不进仓库。

## 目录结构（按功能单元拆分，单文件 ≤ ~300 行）

```
src/
├── main.rs            入口 + 参数分发（--snapshot* 是给测试用的渲染快照）
├── tui.rs             事件循环、按键路由、快照渲染
├── clipboard.rs       复制到剪贴板（arboard → pbcopy 兜底）
├── form/              通用表单引擎（密钥表单、厂商表单共用）
│   ├── mod.rs         Field（文本/选择/只读）、Form、按键分发、表单构造器
│   └── edit.rs        LineEdit：单行编辑（值 + 光标）
├── app/               应用状态机
│   ├── mod.rs         App 结构、start、导航、探活调度
│   ├── modal.rs       Modal 枚举 + 打开/保存分发/删除确认分发
│   ├── keys_store.rs  密钥保存/删除（写 config.toml + secrets.toml）
│   └── providers_store.rs  厂商保存/删除（生成/删除用户 recipe YAML）
├── ui/                一个面板一个文件
│   ├── mod.rs         draw 分发 + theme + pane_block/centered
│   ├── header.rs      顶栏/底栏（底栏按焦点显示 c 复制什么）
│   ├── providers.rs   左栏厂商列表
│   ├── keys.rs        右侧密钥表 + 状态标签
│   ├── balance.rs     右下额度面板
│   ├── form_modal.rs  表单弹窗（光标截断渲染）
│   └── confirm.rs     删除确认弹窗
├── recipe/            厂商协议
│   ├── mod.rs         Recipe/Auth/HttpCall 模型、YAML 加载（builtin→manifest→user 逐级覆盖）、{token}/{base_url} 模板替换
│   ├── balance.rs     额度 JSON → BalanceView（dotted path 取值、数组多卡片、货币格式化）
│   ├── script.rs      BalanceMode（http 缺省 | script 逃生舱）+ ScriptSpec，自定义 serde
│   └── store.rs       用户 recipe 读写（~/.config/apim/recipes/*.yaml）
├── config/            密钥清单
│   ├── mod.rs         KeyEntry、读取 config.toml + secrets.toml
│   └── store.rs       原子写入（tmp+rename，600 权限）
└── probe.rs           并发探活（health + balance 并发；balance 分 http/脚本两路，脚本=env 注入+超时+stdout 逐行）
docs/
└── quota-script-prompt.md  额度脚本代写提示词（整体复制给 AI Agent 用）
recipes/
└── deepseek.yaml      内置 recipe（include_str! 编译进二进制）
```

## 运行时数据（都在仓库外）

- `~/.config/apim/config.toml` — 密钥清单（provider/alias/group，无 token）
- `~/.config/apim/secrets.toml` — token，键名 `"厂商.别名"`，600 权限
- `~/.config/apim/recipes/*.yaml` — 用户厂商 recipe，同 id 覆盖内置
- `APIM_CONFIG_DIR` 环境变量可重定向整个配置目录（测试用）

## 核心约定

1. **密钥永不进仓库**。`.gitignore` 已排除 secrets.toml/.env；写文档、注释、提交信息时一律用 `sk-...` 占位。动手前 `grep -r "sk-"` 扫一遍。
2. **加厂商不改 Rust**。厂商表单只配：ID/名称/Base URL/探活路径/**脚本路径**（额度唯一入口，指向 ~/.config/apim/scripts/ 下可执行脚本）；编辑时路径没改就保留手写配置，清空即取消。额度底层两种形态（BalanceMode）：`kind: script`（env 注入 APIM_TOKEN 等，stdout 逐行直显）+ 声明式 http（缺省，request+parse+render，留给内置 recipe 和手写 YAML，如 new-api 系），不要再为怪接口扩 DSL，也不要往表单加预设类型。AI 代写脚本的标准提示词在 docs/quota-script-prompt.md。
3. **Recipe 覆盖顺序**：builtin(include_str) → `<repo>/recipes/`（开发时）→ `~/.config/apim/recipes/`，后读的同 id 覆盖先读的。`origin: None` = 内置，不可删除只可编辑覆盖。
4. **模块路径稳定**：子模块类型经 mod.rs re-export（如 `crate::app::Modal`），拆文件不破坏外部 import。
5. **改完必跑**：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`。UI 改动跑 `cargo run -- --snapshot`（主界面）/ `--snapshot-form` / `--snapshot-provider-form` 出纯文本渲染核对。
6. 提交信息中文，一行主题 + 要点列表；功能一次一提交。

## 验证命令速查

```bash
cargo run                              # 进 TUI
cargo run -- --snapshot                # 真实接口拉数据渲染成文本（不进 TUI）
cargo test                             # 14 个单测（recipe 解析/存取、config 往返、表单引擎）
cargo install --path .                 # 装进 PATH
```
