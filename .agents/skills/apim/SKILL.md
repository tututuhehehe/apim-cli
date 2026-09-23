---
name: apim
description: 用 apim CLI 管理模型厂商/API Key：厂商与密钥的增删改查、探活与余额查询、编写和绑定额度查询脚本（balance.kind=script）。当用户要加厂商、配密钥、查额度/余额、写额度脚本、或提到 apim 项目本身时使用。
---

# apim 使用指南（CLI + 额度脚本）

apim 是终端 API Key 管理器。AI/脚本一律走 CLI，入口 `apim`（`cargo install --path .` 安装；开发时用 `target/debug/apim`）。数据都在 `~/.config/apim/`：`config.toml`（密钥清单）、`secrets.toml`（token，600 权限）、`recipes/<id>.yaml`（厂商协议）、`scripts/`（额度脚本）。

## CLI 命令速查

```
# 厂商 CRUD
apim provider ls [--json]
apim provider add <id> --name <名> --base-url <URL> [--homepage <主页URL>|none] [--health <路径>|none] [--script <脚本路径>|none]
apim provider set <id> [--name <名>] [--base-url <URL>] [--homepage <主页URL>|none] [--health <路径>|none] [--script <脚本路径>|none]
apim provider rm <id> [--force]          # 有密钥时拒绝；--force 连带删密钥；内置四家(deepseek/openai/moonshot/openrouter)不可删
apim provider copy <源id> [新id] [--name 名]  # 整份复制厂商；外部额度脚本 fs::copy 成独立文件（命名跟随新 id，不引用原脚本）；新 id 缺省 <源id>-copy，被占自动顺延 -copy-2；密钥不跟随

# 密钥 CRUD（token 一律走 stdin，绝不进 argv / shell history）
apim key ls [<provider>] [--json]        # token 掩码显示
apim key add <provider> <别名> [--group <分组>]     # 已存在则覆盖更新 token
apim key set <厂商.别名> [--alias <新别名>] [--group <分组>|none]
apim key rm <厂商.别名>

# 查询 / 快捷
apim status [<provider>] [--json]        # 并发真实探活 + 额度（跑绑定的脚本）
apim copy <厂商.别名> [--base-url]       # 复制密钥 / Base URL
apim use <厂商.别名>                     # 输出 export OPENAI_API_KEY=... OPENAI_BASE_URL=...
```

要点：

- `--script none` = 解绑额度脚本；空串（`--script ""`）同义。`--health none` = 不探活。`--homepage` 是厂商控制面板主页。
- `provider set` 只改传了的字段；`--script` 是显式整体替换，没有「未改保留」语义。
- `provider copy` 复制协议配置不复制密钥；绑定外部脚本时新厂商指向新副本（如 `glm-quota.sh` → `glm-copy-quota.sh`，权限位保留），没绑/内联 run/原文件丢失则无文件动作。TUI 厂商栏同功能按 `y`。
- 坏配置（recipe 误删 / secrets 缺条目）不会锁死 CLI：命令跳过坏条目并打警告，下一次成功写盘自动清除。
- 测试/沙盒：每条会写盘的命令都显式前缀 `APIM_CONFIG_DIR=<临时目录>`（shell 每次调用是新的，export 不跨调用），绝不碰真实 `~/.config/apim`。

## recipe YAML 字段含义

`~/.config/apim/recipes/<id>.yaml`，一个文件描述一个厂商「怎么鉴权、怎么探活、怎么查额度」：

| 字段 | 含义 |
|---|---|
| `id` / `name` | 厂商标识（小写字母/数字/-，密钥配置里 `provider` 引用它）/ 显示名 |
| `base_url` | API 根地址，`{base_url}` 占位符在请求/脚本里展开 |
| `homepage` | 控制面板主页（可选）；TUI 选中厂商按 Enter 用默认浏览器打开 |
| `models_url` | 模型列表端点模板（可选，按 key 浏览模型用）。缺省按序尝试 `{base_url}/models` → `{base_url}/v1/models`，404 自动换下一个；OpenAI 兼容厂商不用配，GLM 这类非标路径的才配（如 `'{base_url}/api/paas/v4/models'`） |
| `auth.kind` | `bearer`（发 `Authorization: Bearer <key>`）｜ `header`（发裸 key）｜ `query`（拼 `?api_key=<key>`）；探活请求也用它 |
| `health` | 探活 GET（`{base_url}` 模板），HTTP 2xx 即算活；留空 = 不探活 |
| `vars` | 自定义变量表。两个用途：注入脚本 env（`APIM_VAR_<大写名>`，如访问令牌）、参与脚本/YAML 的 `{placeholder}` 替换。存敏感值时整个文件保持 600 |
| `balance` | 额度查询绑定，**必须脚本**（见下） |

## 余额查询脚本（每个厂商一个，唯一的额度实现方式）

**所有厂商——包括内置四家（deepseek/openai/moonshot/openrouter）——额度一律走脚本**，没有声明式配置。apim 带着密钥跑脚本，stdout 逐行进额度面板（首行高亮）。本机六个现成实例在 `~/.config/apim/scripts/`：GLM（两接口+日期计算）、DeepSeek/Moonshot/OpenAI/OpenRouter（单请求+ jq）、ikun（new-api 面板，访问令牌走 `APIM_VAR_ACCESS_TOKEN`）。

### 绑定方式

```yaml
balance:
  kind: script                                    # 文档标记，可选
  command: ~/.config/apim/scripts/<id>-quota.sh   # 外部可执行文件（尊重 shebang），~ 会展开
  # run: |                                        # 或内联脚本，二选一；经 /bin/sh -c 执行
  #   echo "剩余 45/100"
  # shell: /bin/zsh                               # 仅 run 时有效，缺省 /bin/sh
  timeout_secs: 15                                # 可选，缺省 15
```

三个等价入口：直接写 recipe YAML、CLI `--script`、TUI 表单「脚本路径」。脚本建议放 `~/.config/apim/scripts/<id>-quota.sh` 并 `chmod +x`。绑定后 `apim status <id>` 与额度面板跑同一脚本。

### 脚本能拿到的参数（env 注入，脚本从 env 读，不要让用户手填）

| 变量 | 含义 |
|---|---|
| `APIM_TOKEN` | 当前密钥的 API Key（用户在 TUI/CLI 里存的，随选中/绑定的 key 变化） |
| `APIM_BASE_URL` | recipe 的 `base_url`（去尾斜杠） |
| `APIM_ALIAS` / `APIM_PROVIDER` | 密钥别名 / 厂商 id |
| `APIM_VAR_<大写名>` | recipe `vars:` 里的每项变量，如访问令牌 `access_token` → `APIM_VAR_ACCESS_TOKEN`（new-api 系面板的 /api/user/self 只认它，不认 sk- Key） |

密钥只走 env，不进 argv（`ps` 看不到）、不进脚本文件。

### 输出契约（必须严格遵守）

- **exit 0**：stdout 逐行显示，**首行高亮当 headline**（放最重要的数字，建议 1~4 行）。
- **exit 非 0**：stderr（截断 200 字符）显示为红色错误。网络失败、Key 无效、字段缺失都要走这条路，**宁可报错不显示假数字**。
- **超时**：`timeout_secs`（缺省 15s）杀进程；stdout 超 50 行截断；stdout 为空也算失败。
- 已知坑：
  - HTTP 200 不等于成功（GLM/new-api 系坏 Key 也回 200）——校验 body 的 `success`/`code`/`status` 字段，失败 stderr + exit 1；
  - `sh` 的 `echo` 会解释反斜杠转义，含 `\"` 的 JSON 会被吃坏——用 `printf '%s' "$RESP" | jq`；
  - jq 数值可能是浮点，整数运算前取整；`bc` 做小数运算。

### 新接一个厂商的标准流程

```bash
# 1. 按 docs/quota-script-prompt.md 写好脚本后，注册并绑定：
apim provider add <id> --name <名> --base-url <https://...> --health <探活路径> \
  --script ~/.config/apim/scripts/<id>-quota.sh
# 2. 密钥由用户自己配（AI 不要经手 token）：
echo 'sk-...' | apim key add <id> main
# 3. 验证（期望 balance.ok=true 且 lines 有额度行）：
apim status <id> --json
```

## 红线

- **密钥永不进仓库/argv/日志/文档**，一律 stdin 或 env；示例用 `sk-...` 占位。
- 改 Rust 代码后必跑：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`；UI 改动跑 `cargo run -- --snapshot` 核对。
