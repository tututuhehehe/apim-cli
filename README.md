# apim

终端里管理模型厂商 API Key：增删改查、看状态、看额度、复制密钥。

## 启动

```bash
cd ~/Documents/VIBE/api管理
cargo run
```

装到 PATH 以后可以直接敲：

```bash
cargo install --path .
apim
```

## 快捷键

`a` / `e` / `d` / `c` 按当前焦点生效：焦点在左边厂商栏就是厂商操作，在右边密钥表就是密钥操作。

| 键 | 厂商栏（左） | 密钥表（右） |
|---|---|---|
| `a` | 添加厂商 | 添加密钥 |
| `e` | 编辑厂商（名称 / Base URL / 主页 / 探活 / 额度） | 编辑密钥（别名 / 分组 / token） |
| `d` | 删除厂商（须先删光它下面的密钥） | 删除密钥 |
| `c` | 复制 Base URL | 复制密钥 |
| `i` | 厂商详情（鉴权 / 端点 / 额度脚本 / 来源 / vars，值不外显） | 密钥详情（`r` 显隐完整 token，`c` 复制） |
| `Enter` | 用默认浏览器打开厂商主页（控制面板） | — |
| `j` / `k` / `Tab` | 移动 / 切换左右栏 | 同左 |
| `r` | 刷新状态和额度 | 同左 |
| `q` / `Esc` | 退出 | 退出 |

## 增删改查

**添加密钥（右侧按 `a`）**：填别名、分组（可空）、密钥，厂商用 `←`/`→` 切换。密钥可以直接 `⌘V` 粘贴。`Enter` 保存，立即写盘并自动检测。

**添加厂商（左侧按 `a`）**：填 ID（小写字母/数字/-，密钥配置里 `provider` 引用它）、显示名称、Base URL，以及三个可选项：

- 主页 URL：该厂商的控制面板地址，选中厂商按 `Enter` 用默认浏览器打开；留空 = 未配置（按 Enter 会提示）。**别把带 token 的一键登录链接贴进来**——主页会出现在列表和 `provider ls` 输出里
- 探活路径：默认 `/models`，拼在 Base URL 后面；留空 = 不探活
- 脚本路径：额度查询脚本（见下「自定义脚本额度」）；留空 = 不查额度

保存后生成 `~/.config/apim/recipes/<id>.yaml`，接着按 `a` 就能给它加密钥。脚本不用自己写：把 `docs/quota-script-prompt.md` 整体复制给任意 AI Agent，附上厂商的官方查询方式，它会按 apim 预留的接口契约写好并给验证命令。

**编辑（`e`）**：密钥表单带出当前值，改别名就是重命名。厂商表单编辑时 ID 锁定；探活路径/脚本路径没改就保存，不会动原 YAML 里手写的配置（DeepSeek 的声明式额度、new-api 的访问令牌 `vars`、内联脚本都原样保留）。清空脚本路径保存即取消脚本额度。

**删除（`d`）**：都弹确认框。厂商下面还有密钥时会拒绝，先删密钥。内置的 DeepSeek 厂商不可删除，但可以 `e` 编辑覆盖（会在用户目录生成同名 YAML）。

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
| `apim key ls [<provider>] [--json]` | 列密钥（token 掩码显示） |
| `apim key add <provider> <别名> [--group <分组>]` | 加密钥；已存在则更新 token |
| `apim key set <厂商.别名> [--alias <新别名>] [--group <分组>\|none]` | 改别名 / 分组 |
| `apim key rm <厂商.别名>` | 删密钥 |
| `apim status [<provider>] [--json]` | 真实探活 + 额度（跑绑定的脚本） |
| `apim copy <厂商.别名> [--base-url]` | 复制密钥 / Base URL 到剪贴板 |
| `apim use <厂商.别名>` | 输出 `export OPENAI_API_KEY=... OPENAI_BASE_URL=...`（`eval $(apim use x)` 用） |

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

## 数据存哪

都在 `~/.config/apim/`，TUI 的增删改直接写这两个文件（权限 600），也可以手动改：

- `config.toml`：别名、分组（不含 token）
- `secrets.toml`：真正的 token，键名是 `"厂商.别名"`

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

首选在 TUI 左侧按 `a`，表单保存即生成 `~/.config/apim/recipes/<id>.yaml`。

复杂厂商（自定义鉴权头、多级 JSON 解析）可以直接写 YAML 放进同一目录，可参考 `recipes/deepseek.yaml`：

```bash
mkdir -p ~/.config/apim/recipes
cp recipes/deepseek.yaml ~/.config/apim/recipes/my-relay.yaml
```

TUI 表单生成的 YAML 和手写的完全等价；编辑时表单只覆盖它认识的字段，手写的 headers、解析规则会保留。

### new-api 系中转站（额度要访问令牌的）

表单已不再提供这个预设（额度统一走脚本），但手写的声明式 YAML 依旧支持；也可以按 `docs/quota-script-prompt.md` 让 AI 写个脚本。手写的话：

多数 new-api 面板的 `/v1/dashboard/billing/subscription` 要么返回假数字，要么不认 API Key。真实余额在 `/api/user/self`，但它只认**访问令牌**（个人设置里生成的那串，不是 sk- Key）。这种要在 `~/.config/apim/recipes/<id>.yaml` 手写：

```yaml
vars:
  access_token: 你的访问令牌
health:
  url: '{base_url}/v1/models'
balance:
  request:
    url: '{base_url}/api/user/self'
    headers:
      Authorization: 'Bearer {access_token}'
  parse:
    divisor: 500000      # new-api: 500000 quota = $1
    currency: USD
    fields:
      total_balance: data.quota
      used: data.used_quota
  render:
    headline: '{total_balance}'
    fields:
      - {label: 剩余, value: '{total_balance}'}
      - {label: 已用, value: '{used}'}
```

探活仍用每条密钥自己的 sk- Key；额度用 `vars` 里的访问令牌（额度是账户级的，同账户多条 Key 显示一样）。文件含令牌，保持 600 权限，别分享。

### 自定义脚本额度（接口长得怪的厂商）

有些厂商的额度没法用「一个请求 + JSON 取值」描述——要发多个请求、算日期、查表映射。比如 GLM Coding Plan：两个接口、按 `unit/number` 挑窗口、按套餐等级映射 MCP 次数。这种走脚本逃生舱 `balance.kind=script`：apim 带着密钥跑一个脚本，把 stdout 逐行显示在额度面板，不再为它们扩配置语法。

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
