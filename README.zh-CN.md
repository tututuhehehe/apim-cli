# apim

终端里管理模型厂商 API Key：增删改查、看状态、看额度、复制密钥、一键导入到 Codex 或 Pi。

[English](README.md) | **简体中文**

[![Release](https://github.com/tututuhehehe/apim-cli/actions/workflows/release.yml/badge.svg)](https://github.com/tututuhehehe/apim-cli/actions/workflows/release.yml)
[![CI](https://github.com/tututuhehehe/apim-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/tututuhehehe/apim-cli/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

<p align="center">
  <img src="docs/images/tui-main.png" alt="apim TUI 主界面：左侧厂商列表带实时额度与延迟，右侧密钥表，底部额度详情；底栏为按键提示" width="900">
</p>

## 安装

**一行命令（macOS / Linux）**：自动识别平台、下载对应预编译二进制、校验 sha256 并安装：

```bash
curl -fsSL https://raw.githubusercontent.com/tututuhehehe/apim-cli/main/install.sh | sh
```

`APIM_INSTALL_DIR` 指定安装目录（默认 `/usr/local/bin` 可写时用它，否则 `~/.local/bin`）；`APIM_VERSION` 指定版本（如 `v0.1.0`，默认最新 Release）。

**npm（macOS / Linux / Windows）**：通过一层薄封装安装对应平台的预编译二进制：

```bash
npm install -g apim-cli   # 或直接跑：npx apim-cli
```

**手动下载**：从 [Releases](https://github.com/tututuhehehe/apim-cli/releases) 下载对应平台压缩包（资产名 `apim-<tag>-<target>.tar.gz` / `.zip`，同目录附 `.sha256` 校验和），解压后把 `apim` 放进 `PATH`。

```bash
# 以 macOS Apple Silicon 为例
tag=v0.1.0
base="https://github.com/tututuhehehe/apim-cli/releases/download/$tag"
curl -fsSL -O "$base/apim-$tag-aarch64-apple-darwin.tar.gz"
tar -xzf "apim-$tag-aarch64-apple-darwin.tar.gz"
sudo mv apim /usr/local/bin/
```

| 平台 | 资产后缀 |
|---|---|
| macOS Apple Silicon | `aarch64-apple-darwin` |
| macOS Intel | `x86_64-apple-darwin` |
| Linux x64 | `x86_64-unknown-linux-gnu` |
| Linux arm64 | `aarch64-unknown-linux-gnu` |
| Windows x64 | `x86_64-pc-windows-msvc`（实验性，额度脚本依赖 `sh`） |

**从源码安装**：

```bash
git clone https://github.com/tututuhehehe/apim-cli.git
cd apim-cli
cargo install --path .
```

Rust 用户也可以 `cargo binstall apim` 直接拉预编译二进制（crate 已配好 binstall 元数据，发布到 crates.io 后即可用）。

### 卸载

`apim uninstall` 与 `apim update` 认同一套渠道，从当初装它的那条渠道卸掉：npm 走 `npm uninstall -g apim-cli`、Homebrew 走 `brew uninstall apim`、install.sh 装的裸二进制直接删（是软链就把链一起删，不留断链）。`target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本一律不删。

```bash
apim uninstall --dry-run   # 只报告会删什么，不动手
apim uninstall             # 先确认再卸，密钥保留
apim uninstall --purge     # 连 ~/.config/apim（config、recipes、scripts、secrets）一起删
apim uninstall --yes       # 跳过确认（脚本 / 非交互）
```

**密钥不会因为卸载而消失**（不加 `--purge` 就不动配置目录）；`--purge` 只删目录名正好是 `apim` 的目录，而且**不递归删软链**（dotfiles 托管 `~/.config/apim` 的情况留给你自己处理）。`--dry-run` 会把要动的每一个路径都列出来（用软链装的就包括真身）；`--json` 在各种情况下形状一致（开发构建 / cargo 副本会报 `channel: null` 并以非零退出，而不是默默什么都不做）。

两处故意不碰：shell rc / `PATH`（install.sh 从不写 rc），以及一键导入写进 `~/.codex/` 的内容 —— 它们和你手写的配置在同一个文件里，只能提示不能代删。要手动收尾：

```bash
rm ~/.codex/apim-models.json              # 1. apim 生成的模型目录
# 2. ~/.codex/config.toml 里：删掉 [model_providers.apim-*] 块，以及顶层的
#    model_provider / model_catalog_json 指针
rm ~/.codex/config.toml.apim.bak          # 3. 改写前的备份，里面也有明文密钥
```

## 使用

```bash
apim            # 无参数：进 TUI
apim help       # CLI 用法
apim --version  # 版本
```

## 快捷键

`a` / `e` / `d` / `c` 按当前焦点生效：焦点在左边厂商栏就是厂商操作，在右边密钥表就是密钥操作。

| 键 | 厂商栏（左） | 密钥表（右） |
|---|---|---|
| `a` | 添加厂商 | 添加密钥 |
| `e` | 编辑厂商（名称 / Base URL / 主页 / 探活 / 额度） | 编辑密钥（别名 / 分组 / token） |
| `d` | 删除厂商（须先删光它下面的密钥） | 删除密钥 |
| `c` | 复制 Base URL | 复制密钥 |
| `y` | 复制厂商：整份 recipe 另存为新厂商（id 自动 `<id>-copy`，名称加「副本」），绑定的外部额度脚本复制成独立文件，改两边的脚本互不影响；`secrets.toml` 里的密钥不跟随 | — |
| `i` | 厂商详情（鉴权 / 端点 / 额度脚本 / 来源 / vars，值不外显） | 密钥详情（`r` 显隐完整 token，`c` 复制） |
| `Enter` | 用默认浏览器打开厂商主页（控制面板） | — |
| `m` | — | 用**当前选中的这把 key** 拉取它的模型列表（模型可见性随 key/分组不同；弹窗内 `/` 聚焦搜索框实时过滤、`Esc` 退出搜索回到列表（过滤保留）、`j/k` 滚动、`c` 复制模型名、`Esc` 关闭弹窗） |
| `x` | — | 把选中密钥 + 它的厂商 + 勾选的模型**一键导入到 Codex**（写 `~/.codex/config.toml`）；面板三步：`⏎` 下一步 → 勾选模型（`空格` 勾选、`a` 全选/清空、`/` 搜索）→ 选默认模型（`j/k` 移动、`h` 返回上一步、`⏎` 导入；只勾一个模型时跳过这步）。详见下节 |
| `j` / `k` | 上下移动 | 上下移动 |
| `Tab` / `h` / `l` | 切换左右栏（h 左 = 厂商栏，l 右 = 密钥栏；已在边缘侧时不动） | 同左 |
| `/` | 过滤厂商（匹配 id 或显示名） | 过滤密钥（匹配别名或分组） |
| `r` | 刷新状态和额度 | 同左 |
| `Ctrl+Z` | 撤销上一次写操作（本次打开面板后新增/修改/删除的厂商与密钥，含复制产生的文件）；可连续按逐步回退 | 同左 |
| `q` / `Esc` | 退出 | 退出 |

按 `/` 打开搜索框，边输入边实时过滤（大小写不敏感，两个列表各自独立）：`Enter` 应用并关闭，`Esc` 取消并恢复进入前的值。过滤生效时底栏显示「筛选: xxx (n/m)」，`j`/`k` 只在过滤后的行间移动；此时无弹窗按 `Esc` 先清除过滤而不是退出（`q` 仍直接退出）。注意：过滤生效时改名/新增的条目若不匹配当前过滤词，会暂时从视图隐身（数据没丢），`Esc` 清除过滤即可见。

## 增删改查

**添加密钥（右侧按 `a`）**：填别名、分组（可空）、密钥，厂商用 `←`/`→` 切换。密钥可以直接 `⌘V` 粘贴。`Enter` 保存，立即写盘并自动检测。

**添加厂商（左侧按 `a`）**：填 ID（小写字母/数字/-，密钥配置里 `provider` 引用它）、显示名称、Base URL，以及三个可选项：

- 主页 URL：该厂商的控制面板地址，选中厂商按 `Enter` 用默认浏览器打开；留空 = 未配置（按 Enter 会提示）。**别把带 token 的一键登录链接贴进来**——主页会出现在列表和 `provider ls` 输出里
- 探活路径：默认 `/models`，拼在 Base URL 后面；留空 = 不探活
- 脚本路径：额度查询脚本（见下「自定义脚本额度」）；留空 = 不查额度

保存后生成 `~/.config/apim/recipes/<id>.yaml`，接着按 `a` 就能给它加密钥。脚本不用自己写：把 `docs/quota-script-prompt.md` 整体复制给任意 AI Agent，附上厂商的官方查询方式，它会按 apim 预留的接口契约写好并给验证命令。

**编辑（`e`）**：密钥表单带出当前值，改别名就是重命名。厂商表单编辑时 ID 锁定；探活路径/脚本路径没改就保存，不会动原 YAML 里手写的配置（内联脚本、`vars` 访问令牌等都原样保留）。清空脚本路径保存即取消脚本额度。**改了厂商配置保存后，该厂商的旧读数与在途探针会作废、立刻用新配置重探**（避免面板显示按旧配置算出的数字）；撤销厂商配置变更同样会重探。

**删除（`d`）**：都弹确认框。厂商下面还有密钥时会拒绝，先删密钥。内置厂商（DeepSeek / OpenAI / Moonshot AI / OpenRouter）不可删除，但可以 `e` 编辑覆盖（会在用户目录生成同名 YAML）。

**复制（厂商栏 `y`）**：把选中厂商整份复制成新厂商——auth/vars/探活路径/额度绑定全带走（`vars` 里可能存的访问令牌也会一起复制），id 自动取 `<源id>-copy`（被占则 `-copy-2`…），名称加「副本」后缀。绑定了外部额度脚本的，脚本**文件本身复制一份**到 `~/.config/apim/scripts/`（命名跟随新 id，如 `glm-quota.sh` → `glm-copy-quota.sh`；同名已存在则顺延 `-2`、`-3`，绝不覆盖），新厂商指向新文件，之后改脚本互不影响；没绑脚本或内联 `run` 就没有文件要复制。`secrets.toml` 里的密钥**不**跟随（复制的是协议配置，不是凭据），复制完按 `a` 给新厂商配自己的 key。

**撤销（`Ctrl+Z`）**：本次打开面板后的写操作都进历史（新增/修改/删除厂商、密钥，以及复制厂商连带产生的 YAML / 脚本副本），在主界面按 `Ctrl+Z` 逐步回退最近一步——内存和磁盘一起回退，底部 toast 会说明撤销了哪一步（弹窗内不响应）。只读动作（探活、复制到剪贴板、打开主页、浏览/搜索）不进历史；历史是本次会话的，退出 TUI 即清空。

**查**：状态列是探活结果（`● 可用` / `● 失败` / `● 无额度`），右下角额度面板显示选中密钥的余额。按 `i` 打开详情检查器：厂商栏看 recipe 全貌（鉴权 / 端点 / 额度脚本 / 来源 / vars——变量值只显示 `••••`），密钥栏看完整信息，`r` 直接在弹窗里显隐完整 token（不用复制出剪贴板），`c` 复制。刷新节奏：**打开时所有厂商各刷一次，之后每 5 分钟自动全量刷新**（探活 + 额度一起）；切换厂商只读缓存、不触发请求；`r` 随时手动刷新当前厂商，刚保存的密钥会立即探测。

表单内：

| 键 | 作用 |
|---|---|
| `Tab` / `↑` / `↓` | 下一项 / 上一项 |
| `←` / `→` | 移动光标；在「厂商」行是切换厂商 |
| `Enter` | 保存 |
| `Esc` | 取消 |

必填项为空、ID 重复、Base URL 不以 `http(s)://` 开头等，底部红字提示，不会写盘。

## CLI（AI / 脚本友好）

TUI 管人，CLI 管机器：`cargo install --path .` 之后所有操作都能走命令行（`apim help` 看全量用法）。数据同一份，CLI 改完 TUI 立即可见，反之亦然。

| 命令 | 作用 |
|---|---|
| `apim provider ls [--json]` | 列厂商（含额度绑定方式、密钥数） |
| `apim provider add <id> --name <名> --base-url <URL> [--homepage <主页URL>\|none] [--health <路径>\|none] [--script <脚本路径>\|none]` | 建厂商 |
| `apim provider set <id> [--name <名>] [--base-url <URL>] [--homepage <主页URL>\|none] [--health <路径>\|none] [--script <脚本路径>\|none]` | 改厂商（只动传了的字段） |
| `apim provider rm <id> [--force]` | 删厂商（有密钥时拒绝，`--force` 连带删密钥；内置不可删） |
| `apim provider copy <源id> [新id] [--name 名]` | 整份复制厂商（auth/vars/探活/额度全带走，`secrets.toml` 里的密钥不跟随）；外部额度脚本复制成独立文件（命名跟随新 id，同名已存在则顺延 `-2`）；新 id 缺省 `<源id>-copy`，被占自动顺延 |
| `apim key ls [<provider>] [--json]` | 列密钥（token 掩码显示） |
| `apim key add <provider> <别名> [--group <分组>]` | 加密钥；已存在则更新 token |
| `apim key set <厂商.别名> [--alias <新别名>] [--group <分组>\|none]` | 改别名 / 分组 |
| `apim key rm <厂商.别名>` | 删密钥 |
| `apim status [<provider>] [--json]` | 真实探活 + 额度（跑绑定的脚本） |
| `apim copy <厂商.别名> [--base-url]` | 复制密钥 / Base URL 到剪贴板 |
| `apim use <厂商.别名>` | 输出 `export OPENAI_API_KEY=... OPENAI_BASE_URL=...`（`eval $(apim use x)` 用） |
| `apim update [--check] [--force] [--json]` | 自动识别当前是从哪条渠道装的（**npm / Homebrew / install.sh**），走同一条渠道更新。install.sh 渠道会下载**该 tag 的**官方脚本、校验 sha256 之后才执行（不是 `curl \| sh`）。`--check` 只报当前/最新版本不动手；`--force` 版本相同时也重装一遍 |
| `apim uninstall [--yes] [--purge] [--dry-run] [--json]` | 从同一条渠道卸载（见[卸载](#卸载)）。不加 `--purge` 密钥照旧保留；`--dry-run` 只报告；`--yes` 跳过确认 |

**密钥安全**：token 一律走 stdin，不进命令行参数（防 `ps` 和 shell history）：

```bash
echo '你的key' | apim key add glm main
```

### 余额查询脚本绑定

一个厂商的额度查询 = 绑定一个脚本（recipe 的 `balance.kind: script` + `command:` 指向可执行文件）。三个入口，效果等价：

1. **TUI**：厂商表单的「脚本路径」字段；
2. **CLI**：`apim provider add/set ... --script <路径>` 绑定，`--script none` 解绑（CLI set 是显式指令，直接整体替换，不做 TUI 那套「没改就保留」）；
3. **直接写 YAML**：AI 代写路线，见 `docs/quota-script-prompt.md`。

脚本建议放 `~/.config/apim/scripts/<厂商id>-quota.sh`（约定而非强制）。绑定后 `apim status <厂商>` 和 TUI 额度面板跑的是同一个脚本，结果一致；脚本的输入输出契约（env 注入、stdout 逐行、exit 非 0 报错）见 README 下文「自定义脚本额度」。

### AI 接入一个新厂商的全流程

```bash
# 1. 把 docs/quota-script-prompt.md 整体复制给 AI，附上厂商官方的查询方式
#    → AI 产出 ~/.config/apim/scripts/<id>-quota.sh（并可代跑安装命令）
# 2. AI 注册厂商并绑定脚本：
apim provider add glm --name "GLM Coding Plan" --base-url https://open.bigmodel.cn \
  --health /api/monitor/usage/quota/limit --script ~/.config/apim/scripts/glm-quota.sh
# 3. 用户自己配密钥（key 不过 AI 的手）：
echo '你的key' | apim key add glm main
# 4. AI 自我验证：
apim status glm --json
```

## 一键导入到 Codex / Pi

密钥表里选中一把密钥按 `x`，把「这把密钥 + 它的厂商 + 勾选的模型」写进某个客户端的配置，不用再手改 `~/.codex/config.toml`。面板三步：**选客户端**（Codex 或 Pi）→ **勾选模型**（列表与 `m` 键一致；`空格` 勾选、`a` 全选/清空、`/` 搜索，`⏎` 下一步）→ **选默认模型**（从刚勾的那些里挑一个当客户端的默认模型，`j/k` 移动、`h` 返回上一步、`⏎` 导入）。只勾了一个模型时第三步自动跳过。

导入完成前它会**让客户端自己读一遍新配置**（`codex debug models` / `pi --list-models`）确认勾选的模型都在，然后给一条成功提示；不成功会当场把原因显示在面板里。

> **codex 只在守护进程启动时读一次模型目录，导入后必须重启 codex 才能让 `/model` 刷新。** apim 导入成功后会**自动重启 codex 的 app-server 守护进程**（杀掉在跑的，codex 下次启动自动起新的），toast 里会告知；不想让它动进程就设 `APIM_NO_RESTART_CODEX=1`，那时手动 `pkill -f "codex app-server"` 即可。
>
> 为什么非要重启：TUI 和桌面端都挂在同一个常驻 daemon 上，而不重启时 **`codex exec` 已经能用新模型、`/model` 里却还是旧列表** —— 这是最容易误判成「没导入成功」的现象（cc-switch 官方指南也写了同一件事）。

### 写到哪

**模型不在 `config.toml` 里**，这是 Codex 自己的机制（GLM / DeepSeek 官方接入文档也是这么配的）：模型写在 `model_catalog_json` 指向的**独立 JSON 文件**里，`config.toml` 只放一行指针（apim 还会在它上面写一行注释提醒）。

| 位置 | 内容 |
|---|---|
| `~/.codex/config.toml` 顶层 | `model_provider` = 厂商 id、`model` = 默认模型、`model_reasoning_effort` = 默认思考强度、`model_catalog_json = "apim-models.json"` |
| `~/.codex/config.toml` 的 `[model_providers.<厂商id>]` | `name` / `base_url`（自动补 `/v1`）/ `wire_api = "responses"` / `experimental_bearer_token` |
| `~/.codex/apim-models.json` | 勾选模型的元数据：每个模型都带 `medium/high/xhigh/max` 四档思考等级与默认档，`/model` 就是从这读的 |

几个刻意的选择：

- **密钥行上的 ★ 是现场读出来的，不是 apim 记着的**。apim **不存**「上次导入了谁」：启动时、切厂商（`j`/`k`）时、按 `r` 刷新时、5 分钟自动刷新时、每次导入成功后，都重新读一遍**客户端自己的配置** —— Codex 看顶层 `model_provider` → 那个 `[model_providers.<id>]` 块的 `experimental_bearer_token`；Pi 看 `defaultProvider`（项目级 `.pi/settings.json` 优先）那个 provider 的凭据，并按 **pi 自己的优先级**取（先 `auth.json`，再 `models.json` 的 `apiKey`；`auth.json` 对 apim 是**只读**的 —— 那里面还有你的订阅 OAuth 凭据，pi 自己加锁管）。**真正在用的**那把才带角标 `★C` / `★P`（字母是客户端短标，多个客户端叠成 `★C,P`）。手改了客户端配置（换 token、换默认 provider、删掉那个块），下次重算 ★ 就跟着消失，不会骗你。
- **认 ★ 只看 key，不看名字**（Codex 侧同样如此）：所以你自己起的 provider 名、或者 pi 内置的 `deepseek`，只要用着 apim 里那把 key，★ 也会亮。
- **只切换激活项，不删旧配置**。Codex 允许 `config.toml` 里同时存在多个 `[model_providers.*]`，但同一时刻只有 `model_provider` 指向的那一个生效。所以导入新厂商时旧的 provider 块原样保留（想切回去改一下 `model_provider` 就行），你手写的注释、`[projects.*]`、`[tui]` 也不会被重写。
- **代价：旧块里的旧 token 也一起留着**。apim 不会清理旧 provider 块，所以切走之后那个厂商的 `experimental_bearer_token` 仍然明文躺在 `~/.codex/config.toml` 里 —— 不打算再用就手动删掉那个块（或在那个厂商侧轮换/吊销这把 key）。
- **密钥直接写进 `experimental_bearer_token`**。`~/.codex/config.toml` 本来就是 600 权限。在 apim 里轮换这把 key 后，记得重新按一次 `x` 同步。
- **思考强度不用选**：目录里每个模型都声明 `medium/high/xhigh/max` 四档，顶层 `model_reasoning_effort` 固定写 `high`（面板上不让你逐个挑）。想换档就在 codex 里用 `/model` 选，或直接改 `apim-models.json` / `config.toml`。
- **每次导入前备份**：改写前把现有内容存成 `~/.codex/config.toml.apim.bak`，想回退直接拿它覆盖回去。备份与改写后的 `config.toml` 都会被设成 600（里面有 token；codex 自己建的 0644 也会被收紧）；如果 codex 校验不过，apim 会用备份把两处改动自动还原。`config.toml` 是符号链接（dotfiles 管理）时会写入链接指向的真实文件、不替换链接，而备份始终留在 `~/.codex/` 下。
- **模型条目是照 codex 官方字段手写的迷你条目**（GLM / DeepSeek 官方 Codex 接入文档 + cc-switch 跨版本实测的最小模板）：`shell_type: "shell_command"`、`apply_patch_tool_type: "freeform"`、一句中性的 `base_instructions`（codex 把它当必填字段），并带 `supports_reasoning_summaries` 与 `supports_parallel_tool_calls` 两个**老版 codex 会当必填**的字段。所以每个模型只要 **~1.5KB**，也**不会**把 GPT 专属的东西（`code_mode_only`、`use_responses_lite`、872k 上下文窗口、62KB 的 GPT harness）塞给第三方模型。上下文窗口用 codex 给未知模型的默认值 272000，想按模型写真实值直接改 `apim-models.json`。
- **校验用你本机的 codex**：写完让它自己解析一遍新配置（`codex debug models`），勾选的模型都在才算导入成功。
- **只支持 Responses 协议**：Codex 0.134+ 已经删掉 `wire_api = "chat"`，中转站必须提供 `/v1/responses`，否则一律 400。面板**不替你做端点能力判断** —— 它列出的模型列表和密钥表按 `m` 看到的完全一致（同一个接口、同一份解析），你勾哪些就导哪些。
- **上下文窗口 / 输入模态**：目录条目统一用 codex 给未知模型的默认值（272000 窗口、`[text, image]`）。想按模型写真实值（比如 DeepSeek 官方 `/v1/models` 会返回 `context_window`），直接改 `~/.codex/apim-models.json` 里对应条目即可。
- 厂商 id 撞上 Codex 保留名（`openai` / `ollama` / `lmstudio` / `amazon-bedrock*`）时会自动加前缀写成 `apim-openai`。
- 这份目录是**整表替换**（实测：只放一个模型进去，`codex debug models` 就只输出那一个），所以在用自定义 provider 时 `/model` 里只会出现你勾选的模型，内置 OpenAI 模型不再列出。想拿回内置表，把 `~/.codex/config.toml` 里的 `model_catalog_json` 一行删掉即可。

> 需要本机装好 `codex`（用它生成并校验模型目录）；apim 从 `PATH` 找它，也可以用 `APIM_CODEX_BIN` 指定路径。

### 一键导入到 Pi

同一个 `x` 面板也能导到 **Pi**（第一步选 `Pi`）。Pi 加第三方 provider 是纯数据的事（`models.json`），所以 apim 写两处：

| 位置 | 内容 |
|---|---|
| `~/.pi/agent/models.json` 的 `providers.apim-<厂商id>` | `name` / `baseUrl`（缺 `/v1` 自动补）/ `api = "openai-completions"` / `apiKey` / `models`（勾选的那几个） |
| `~/.pi/agent/settings.json` | `defaultProvider = apim-<厂商id>`、`defaultModel = 第三步选的那个模型` |

几个刻意的选择：

- **provider 键一律带 `apim-` 前缀 —— 但这只管「写」**。Pi 自带一大批内置 provider（`deepseek` / `openai` / `openrouter` …），而 `models.json` 里同名的条目会**覆盖那个内置 provider 的 `baseUrl`** —— 等于悄悄把你的 OpenAI 模型指到中转站。加前缀永远不会撞名，`/model` 里也一眼看出是 apim 写的；而认 ★ 时**不看名字**：你用着 apim 里那把 key 的 provider（自己起的名字或 pi 内置的都行）都会亮。
- **pi 的 `auth.json` 一个字都不写**。apim 把密钥写在 `models.json` 的 `apiKey` 里（pi 官方文档给兼容端点的写法）；`auth.json` 是 `/login` 的凭据库（里面有订阅的 OAuth refresh token），pi 用 `proper-lockfile` 自己管、读取时逐条校验（任一条坏掉整份加载失败），所以 apim 只**读**它来判断现在用的是哪把 key。
- **只动 apim 负责的那几个键**：`models.json` 里别的 provider 与用户手写的 `headers` / `compat` / `modelOverrides` / `authHeader`、`settings.json` 里别的设置，全部原样保留（有测试守着）。
- **模型条目留最小集合**（`id` / `name` / `reasoning: true` / `input: [text, image]`），其余交给 pi 自己的保守默认（128000 上下文 / 16384 输出 / 零价）—— apim 不替它编数字。想按模型写真值就自己改 `models.json`。
- **默认模型镜像 pi 自己的行为**：除写 `defaultProvider` / `defaultModel` 外，`enabledModels` 非空时会把 `<provider>/<model>` 追加进去 —— 与你在 pi 里按 `Ctrl+S` 存默认模型时一致。不这么做的话，设了 `enabledModels` 的用户会「导入成功但选不到」。
- **校验用你本机的 pi**：写完跑 `pi --list-models`，勾选的每个模型都要**挂在我们的 provider 键下**出现；不过就用 `.apim.bak` 备份把两处都还原。
- **不需要重启**：pi 没有常驻进程，打开 `/model`（或重开）就能看到新 provider。
- **只走 API key 这一路**：pi 的订阅是 `/login` 的 OAuth（凭据在 `auth.json`），apim 既不读也不写。

> 需要本机装好 `pi`（用它校验结果）；apim 从 `PATH` 找它，也可以用 `APIM_PI_BIN` 指定路径。

**以后再加客户端**（Claude Code …）是 Rust 侧的事：一个 `Agent` 变体 + 一个 `clients/<id>/` 子模块（写哪里 / 怎么写 / 怎么校验 / 怎么重载 / 怎么认出正在用的密钥）+ 一条分派，面板不用改；**不做 YAML 配方** —— 各家配置格式、鉴权变量名、生效方式都不一样，不是同一套协议（详见 AGENTS.md 约定 13）。

## 数据存哪

都在 `~/.config/apim/`，TUI 的增删改直接写这两个文件（权限 600），也可以手动改：

- `config.toml`：别名、分组（不含 token）
- `secrets.toml`：真正的 token，键名是 `"厂商.别名"`

导入目标在别处：`~/.codex/config.toml` + `~/.codex/apim-models.json`（Codex，备份为 `config.toml.apim.bak`）与 `~/.pi/agent/models.json` + `~/.pi/agent/settings.json`（Pi，`PI_CODING_AGENT_DIR` 可改整个目录，备份为 `<原名>.apim.bak`）。

apim 侧**不存**导入记录：密钥行上的 ★ 是按客户端自己的配置现场算出来的（见上文）。

手动改的格式示例：

```toml
# ~/.config/apim/config.toml
[[keys]]
provider = "deepseek"
alias = "default"
# group = "个人"   # 可选
```

```toml
# ~/.config/apim/secrets.toml
[tokens]
"deepseek.default" = "sk-你的密钥"
```

```bash
mkdir -p ~/.config/apim
chmod 600 ~/.config/apim/config.toml ~/.config/apim/secrets.toml
```

## 加一个新厂商

内置厂商开箱即用：**DeepSeek、OpenAI、Moonshot AI、OpenRouter** 已编译进二进制，不用写 recipe，在 TUI 左侧选中后按 `a` 直接加密钥即可（OpenAI 的余额接口仅部分账户有权限，无权限时额度面板报 HTTP 错误属预期）。同 id 放一份 YAML 到 `~/.config/apim/recipes/` 即可覆盖内置定义。

自定义厂商首选在 TUI 左侧按 `a`，表单保存即生成 `~/.config/apim/recipes/<id>.yaml`。

复杂厂商（自定义鉴权头、多级 JSON 解析）可以直接写 YAML 放进同一目录，可参考 `recipes/deepseek.yaml`：

```bash
mkdir -p ~/.config/apim/recipes
cp recipes/deepseek.yaml ~/.config/apim/recipes/my-relay.yaml
```

TUI 表单生成的 YAML 和手写的完全等价；编辑时表单只覆盖它认识的字段，手写的 headers、解析规则会保留。

### new-api 系中转站（额度要访问令牌的）

多数 new-api 面板的 `/v1/dashboard/billing/subscription` 要么返回假数字，要么不认 API Key。真实余额在 `/api/user/self`，但它只认**访问令牌**（个人设置里生成的那串，不是 sk- Key）。写一个脚本（`~/.config/apim/scripts/<id>-quota.sh`）：

```sh
#!/bin/sh
# 访问令牌放 recipe 的 vars: {access_token: ...}，apim 注入为 APIM_VAR_ACCESS_TOKEN
RESP="$(curl -sS --max-time 10 \
  -H "Authorization: Bearer ${APIM_VAR_ACCESS_TOKEN:?}" \
  "${APIM_BASE_URL}/api/user/self")"
printf '%s' "$RESP" | jq -r '"剩余 $\(.data.quota / 500000 | floor)  （已用 $\(.data.used_quota / 500000 | floor)）"'
```

recipe 里绑定并放访问令牌：

```yaml
vars:
  access_token: 你的访问令牌
health:
  url: '{base_url}/v1/models'
balance:
  kind: script
  command: ~/.config/apim/scripts/<id>-quota.sh
```

探活仍用每条密钥自己的 sk- Key；额度用 `vars` 里的访问令牌（额度是账户级的，同账户多条 Key 显示一样）。文件含令牌，保持 600 权限，别分享。

### 模型列表端点（`m` 键）

密钥表里选中某条 key 按 `m`，用**这把 key** 拉取它可见的模型列表（模型可见性随 key/分组不同）。端点自动按序尝试：recipe 显式 `models_url` → 探活路径（以 `/models` 结尾时）→ `{base_url}/models` → `{base_url}/v1/models`，404 自动换下一个（其余错误直接返回，保留真实原因）。绝大多数 OpenAI 兼容厂商无需配置；GLM 这类非标路径的在 recipe 里加一行：

```yaml
models_url: '{base_url}/api/paas/v4/models'
```

### 自定义脚本额度（所有厂商统一走这条路）

**额度查询一律是脚本**：apim 带着密钥跑一个脚本，把 stdout 逐行显示在额度面板（首行高亮）。脚本想怎么查、怎么算都行——单请求的规整接口（DeepSeek/Moonshot/OpenRouter…）几行 shell + jq 搞定，要发多个请求、算日期的（GLM Coding Plan）也装得下。

```yaml
balance:
  kind: script
  command: ~/.config/apim/scripts/glm-quota.sh   # 外部可执行文件（尊重 shebang）
  # run: |                                        # 或内联脚本，经 shell -c 执行
  #   echo "剩余 45/100"
  timeout_secs: 15                                # 缺省 15
```

契约：

- apim 注入环境变量：`APIM_TOKEN`（密钥）、`APIM_BASE_URL`、`APIM_ALIAS`、`APIM_PROVIDER`，以及 `vars` 里的每项 `APIM_VAR_<大写名>`。密钥只走 env，不进命令行参数（`ps` 看不到）。
- exit 0：stdout 每行一条进面板，首行高亮；exit 非 0 / 超时：stderr（截断）显示为红色错误。
- 脚本可用本机任何工具（curl、jq、python……），等于在配置里写「这个厂商的额度怎么查」。GLM 的完整实例：`~/.config/apim/scripts/glm-quota.sh` + `~/.config/apim/recipes/glm.yaml`。

TUI 里也可以配：厂商表单的「脚本路径」就是它；编辑时路径没改就原样保留手写的 `run:`/`shell:`/`timeout_secs`，清空即取消脚本额度。

让 AI 代写：把 `docs/quota-script-prompt.md` 整体复制给任意 Agent，再附上厂商官方的查询方式（文档 / curl 示例），它会产出脚本 + recipe 并给验证命令——密钥只在验证时用环境变量传，不用贴给 AI。

内置四家（DeepSeek/OpenAI/Moonshot/OpenRouter）的 recipe 编译在二进制里，但它们引用的脚本在 `~/.config/apim/scripts/`——本机已就位；换新机器时按 `docs/quota-script-prompt.md` 让 AI 重新生成，或从旧机器拷贝脚本目录。

## 发布

维护者用的发版流程（CI 发版、npm、Homebrew tap、回滚）见 [docs/RELEASING.md](docs/RELEASING.md)。

## License

[MIT](LICENSE) © 2026 tututuhehehe
