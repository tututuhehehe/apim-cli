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
apim provider copy <源id> [新id] [--name 名]  # 整份复制厂商；外部额度脚本 fs::copy 成独立文件（命名跟随新 id，同名已存在则顺延 -2，不引用原脚本；绝不覆盖）；新 id 缺省 <源id>-copy，被占自动顺延 -copy-2；secrets.toml 里的密钥不跟随（但 recipe.vars 会带走）

# 密钥 CRUD（token 一律走 stdin，绝不进 argv / shell history）
apim key ls [<provider>] [--json]        # token 掩码显示
apim key add <provider> <别名> [--group <分组>]     # 已存在则覆盖更新 token
apim key set <厂商.别名> [--alias <新别名>] [--group <分组>|none]
apim key rm <厂商.别名>

# 查询 / 快捷
apim status [<provider>] [--json]        # 并发真实探活 + 额度（跑绑定的脚本）
apim copy <厂商.别名> [--base-url]       # 复制密钥 / Base URL
apim use <厂商.别名>                     # 输出 export OPENAI_API_KEY=... OPENAI_BASE_URL=...
apim update [--check] [--force] [--json] # 认渠道（npm/Homebrew/install.sh）后从原渠道更新自己
apim uninstall [--yes] [--purge] [--dry-run] [--json]  # 从原渠道卸掉自己（--purge 连配置+密钥一起删）
```

要点：

- `--script none` = 解绑额度脚本；空串（`--script ""`）同义。`--health none` = 不探活。`--homepage` 是厂商控制面板主页。
- `provider set` 只改传了的字段；`--script` 是显式整体替换，没有「未改保留」语义。
- `provider copy` 复制协议配置不复制 `secrets.toml` 密钥（但 recipe 的 `vars` 会原样带走，可能含访问令牌）；绑定外部脚本时新厂商指向新副本（如 `glm-quota.sh` → `glm-copy-quota.sh`，同名已存在则顺延 `-2`，权限位保留），没绑/内联 run/原文件丢失则无文件动作。TUI 厂商栏同功能按 `y`。
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

**所有厂商——包括内置四家（deepseek/openai/moonshot/openrouter）——额度一律走脚本**，没有声明式配置。apim 带着密钥跑脚本，stdout 逐行进额度面板（首行高亮）。本机七个现成实例在 `~/.config/apim/scripts/`：GLM（两接口+日期计算）、DeepSeek/Moonshot/OpenAI/OpenRouter（单请求+ jq）、ikun（new-api 面板，访问令牌走 `APIM_VAR_ACCESS_TOKEN`）、opencode-go（官方 usage 端点 + percent 整百分比 + 日期计算）。

> **写脚本需要的全部知识（env 注入表、输出契约、recipe 字段、完整流程）都在本 skill 里，不需要读 `src/`。** 执行器只做四件事：注入 env → 按 shebang 跑脚本 → 收 stdout/stderr → 按 exit code 判定，没有别的魔法。

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
  - 数值单位要先确认：有的接口 percent 是 0-100「整百分比」，有的返回 0-1 小数——别想当然乘 100 或除 100（CodexBar 就曾把整百分比 1 当成 0.01，显示成 100%）;
  - macOS 没有 GNU date：ISO8601 毫秒时间先去毫秒（`sed -E 's/[.][0-9]+Z$/Z/'`），再 `date -ju -f "%Y-%m-%dT%H:%M:%SZ" "$ts" "+%s"` 转 epoch；epoch 转本地时间用 `date -r <epoch> "+%m-%d %H:%M"`；
  - 坏 Key/鉴权失败可能回空 body 或 401，curl 照样 exit 0——能不能成功只看「有没有拿到期望字段」，不能只看 HTTP 码。

### 新接一个厂商：写额度脚本的完整流程（无需读源代码）

> `docs/quota-script-prompt.md` 是给「外部 AI 整体代写」用的提示词；本小节是 apim 自己动手的固定流程。四步：**查接口 → 映射 apim → 写脚本 + mock 测试 → 注册 + 真实验证**。

**第 0 步 · 查接口：确定三样东西**
找厂商官方文档里的「余额/额度/usage」API；没有官方文档时，搜第三方解析实现（cc-switch、CodexBar、pi 插件等常在 README/issue 里贴真实响应或解析代码），从那里抄精确 JSON 结构。必须确定：
1. 请求 URL + 方法 + **鉴权方式**（Bearer / 裸 header / query？）；
2. 返回 JSON 的**字段路径**（用 jq 能直接取到）；
3. 字段的**单位与语义**：0-100 整百分比还是 0-1 小数？美元还是次数？空/缺省长什么样？
4. **失败时的状态字段在哪**（HTTP 200 也可能藏错误：`success`/`code`/`status`/`error`）。

**第 1 步 · 映射到 apim 接口**

| 厂商概念 | apim 落点 |
|---|---|
| API 根地址 | recipe `base_url`（`{base_url}` 占位符自动展开） |
| 鉴权方式 | `auth.kind`：`bearer`（`Authorization: Bearer <key>`）/ `header`（裸 key）/ `query`（`?api_key=<key>`） |
| 探活端点（GET，2xx=活，留空=不探活） | `health`：传 `'{base_url}/<路径>'` |
| 模型列表端点 | `models_url`；默认依次试 `{base_url}/models` → `{base_url}/v1/models`，OpenAI 兼容不用配 |
| 控制面板主页 | `homepage`（TUI 选中厂商 Enter 打开） |
| 额度/余额查询 | **只能脚本**：`balance.kind=script` + `command:`；额度不走声明式配置 |
| API Key 注入 | 脚本从 `APIM_TOKEN` 读（绝不让用户手填） |
| 额外凭据（面板访问令牌等） | recipe `vars:` 定义 → 脚本读 `APIM_VAR_<大写名>` |

**第 2 步 · 写脚本**（骨架：`~/.config/apim/scripts/<id>-quota.sh`，写完 `chmod +x`）

```sh
#!/bin/sh
# apim 额度脚本：<厂商>（<接口一句话>）
set -u
KEY="${APIM_TOKEN:-}"; BASE="${APIM_BASE_URL:-<官方默认>}"
[ -n "$KEY" ] || { echo "APIM_TOKEN 未注入" >&2; exit 1; }

RESP="$(curl -sS --connect-timeout 5 --max-time 12 \
  -H "Authorization: Bearer $KEY" -H "Accept: application/json" \
  "$BASE/<余额端点>")" || { echo "请求失败，请检查 Key 或网络" >&2; exit 1; }

# ① 先验 body 状态字段（HTTP 200 ≠ 成功）；缺关键字段也走错误路
printf '%s' "$RESP" | jq -e '.usage != null' >/dev/null 2>&1 || {
  echo "接口异常：$(printf '%s' "$RESP" | jq -r '.error.message // .message // ""' | head -c 200)" >&2
  exit 1
}
# ② 取数值：jq 值可能是浮点，先 %.0f 取整；确认过单位（0-100 整百分比就直接用，别乘 100）
PCT="$(printf '%s' "$RESP" | jq -r '.usage.rolling.percent // 0')"; PCT="$(printf '%.0f' "$PCT")"
# ③ macOS 日期工具没有 GNU date：ISO(毫秒) → epoch → 本地时间，都带 GNU fallback
TS="$(printf '%s' "$RESP" | jq -r '.usage.rolling.resetsAt // ""' | sed -E 's/[.][0-9]+Z$/Z/')"
EPOCH="$(date -ju -f "%Y-%m-%dT%H:%M:%SZ" "$TS" "+%s" 2>/dev/null || date -d "$TS" "+%s" 2>/dev/null || echo 0)"
LOCAL="$(date -r "$EPOCH" "+%m-%d %H:%M" 2>/dev/null || date -d "@$EPOCH" "+%m-%d %H:%M" 2>/dev/null)"
# ④ 输出：1~4 行，首行放最重要的数字（headline）
echo "剩余 $((100 - PCT))% · 重置 $LOCAL"
echo "5 小时窗口已用 ${PCT}%"
```

**第 3 步 · 本地 mock 测试（不碰真实 key、不碰真实网络）**
用一次性 python http.server 模拟厂商响应，`APIM_BASE_URL` 指到 127.0.0.1 跑脚本，验三条路径：成功路径（照抄第 0 步确认的 JSON）→ stdout 符合预期且 exit 0；坏 Key 路径（body 带 error/异常 status）→ stderr 报错且 exit 1；网络失败路径（端口不存在的地址）→ stderr 报错且 exit 1。

```python
# /tmp/mock.py：按第 0 步确认的结构填厂商响应
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        body = json.dumps({...厂商真实响应...}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", 8799), H).serve_forever()
```

```bash
python3 /tmp/mock.py & SRV=$!; sleep 1
APIM_TOKEN=sk-test APIM_BASE_URL=http://127.0.0.1:8799/v1 sh ~/.config/apim/scripts/<id>-quota.sh; echo exit=$?
kill $SRV
```

mock 阶段能低成本验出「数值单位错、状态字段没校验、日期解析错」这类问题，**先 mock 过了再碰真实 key**。兜底 URL 要写进变量（`${APIM_BASE_URL:-...}`），不然 mock 时会把请求发到真实地址。

**第 4 步 · 注册 + 真实验证**

```bash
# 脚本先写好再注册；--health 传路径自动拼 {base_url}；--homepage 可选
apim provider add <id> --name <显示名> --base-url <https://...> \
  --homepage <控制台URL> --health <探活路径> \
  --script ~/.config/apim/scripts/<id>-quota.sh
# 密钥必须用户本人配（AI 绝不经手真实 token）：
echo 'sk-...' | apim key add <id> main
# 真实验证（期望 balance.ok=true 且 lines 有额度行）：
apim status <id> --json
```

**写脚本自查清单（对着过一遍再交）**
- [ ] 每步确认过：URL、鉴权方式、字段路径、字段单位（整百分比还是 0-1、美元还是次数）
- [ ] HTTP 200 也验了 body 状态字段；字段缺失走 stderr + exit 1（宁可报错不显示假数字）
- [ ] `printf '%s' "$RESP" | jq` 解析（`sh` 的 `echo` 会吃 `\"` 转义）
- [ ] 数值运算前 `printf '%.0f'` 取整；小数用 `bc`
- [ ] 每个请求都带 `--connect-timeout`/`--max-time`（总时长受 recipe `timeout_secs` 约束，缺省 15s）
- [ ] macOS 日期：解析用 `date -ju -f`、展示用 `date -r`，都给了 GNU fallback
- [ ] 密钥只从 env 读，不 echo、不写文件、不进 argv
- [ ] 输出 1~4 行、首行是数字 headline
- [ ] mock 三条路径测过、`sh -n` 语法通过、`chmod +x`
- [ ] 注册后 `apim status <id> --json` 用用户的 key 真实验证过

## 一键导入到 Codex / Pi（TUI `x` 键，暂无 CLI）

密钥表里选中一把密钥按 `x`：把「这把密钥 + 它的厂商 + 勾选的模型」写进某个客户端配置。面板三步：选客户端（Codex 或 Pi）→ 勾选模型（列表与 `m` 键一致；`空格` 勾选、`a` 全选、`/` 搜索、`⏎` 下一步）→ 选默认模型（只勾一个时自动跳过）。导入后会让客户端自己读一遍新配置（`codex debug models` / `pi --list-models`）确认成功，密钥行上会打 ★ 角标（`★C`、多个客户端叠成 `★C,P`）标出**当前真正在用**的那几把 —— 它是回读客户端配置现场算出来的，你手改了它们的配置（换 token / 切走默认 provider），下次重算（切厂商 / `r` / 自动刷新）★ 就消失。

**Pi 那边只写一处**：`~/.pi/agent/models.json` 的 `providers.apim-<厂商id>`（`baseUrl` + `api: openai-completions` + `apiKey` + 勾选的 models）；**`settings.json` 一个字都不动**（默认 provider / 默认模型 / `enabledModels` 都是用户自己的设定，Pi 也没有「选默认模型」这一步）；provider 键带 `apim-` 前缀避免蹭到 pi 内置 provider 的 baseUrl（**只管写**）；模型条目只写 id/name/reasoning/input，其余用 pi 的保守默认；不需要重启，打开 `/model` 即可；只走 API key（订阅是 `/login` 的 OAuth，apim 不碰）。★ 判定只认 key：扫 pi 配置里的**每一份**凭据（`auth.json`（`/login` 存的，只读）+ `models.json` 里每个 `apiKey`）与 apim 密钥比 token，所以 provider 叫什么名字都行、`defaultProvider` 是哪个也不影响。

**模型不在 `config.toml` 里**（Codex 的机制，GLM / DeepSeek 官方 Codex 文档也是这个写法）：顶层只写 `model_provider` / `model` / `model_reasoning_effort` / `model_catalog_json="apim-models.json"`，模型元数据（每模型都带 medium/high/xhigh/max 四档思考等级，默认档固定 high；面板不让人挑）在 `~/.codex/apim-models.json`。apim 侧**不存**导入记录（★ 是回读 codex 配置现场算的）。

**导入完 apim 会自动重启 codex 的 app-server 守护进程**（`APIM_NO_RESTART_CODEX=1` 可关）：codex 只在 daemon 启动时读一次模型目录，不重启时 `codex exec` 已能用新模型、但 `/model` 里还是旧表。判断方法：`ps -o pid,lstart,command -p $(pgrep -f "app-server" | tr '\n' ',')` 看 daemon 启动时间是否早于导入时间。

几个容易踩的：

- Codex **可以同时定义多个** `[model_providers.*]`，但同一时刻只有 `model_provider` 指向的那个生效 → apim 只切激活项，旧 provider 块保留（想要多套并存就用官方的 `codex --profile <name>` + `~/.codex/<name>.config.toml`）。
- **`wire_api = "chat"` 已被 codex 删除**（0.134+ 硬报错）。中转站必须提供 `/v1/responses`；加客户端 = `src/clients/mod.rs` 加 `Agent` 变体 + 新建 `src/clients/<id>/` 子模块（写哪里 / 怎么写 / 怎么校验 / 怎么重载 / 怎么认出正在用的密钥）+ 补一条分派；面板（`app/import`、`ui/import`）不用改，文案全取自 `Agent::{label,badge,config_hint,reload_hint,default_model_step}`。**不要**给客户端写 YAML 配方（约定 2 的数据化范围是厂商协议；客户端之间不是同一套协议）。「选默认模型」这一步已经按客户端可关：`Agent::default_model_step()` 返回 `None` 就跳过（pi 就是这样），`ImportRequest.default_model` / `ImportReport.model` 都是 `Option`。

导入面板的模型列表与 `m` 键**完全一致**（同一个解析函数，只取模型名）；**不做端点能力筛选** —— `supported_endpoint_types` 是 new-api 后台的端点映射、会漏报（实测 ikun 把 `gpt-6-sol` 标成只有 `openai`，实际能用），拿它筛选会藏掉能用的模型。
- 模型条目是照 codex 官方字段手写的**迷你条目**（GLM/DeepSeek 官方文档 + cc-switch 实测模板，~1.5KB/模型，带 `supports_reasoning_summaries` 与 `supports_parallel_tool_calls` 以兼容老版 codex）；导入完用 `codex debug models` 反向校验。
- 厂商 id 撞保留名（`openai`/`ollama`/`lmstudio`/`amazon-bedrock*`）时写成 `apim-openai`。
- 验证：`cargo run -- --snapshot-import` / `--snapshot-import-models` 看渲染；`cargo test -- codex_real_end_to_end --ignored --nocapture` 跑真机端到端（需本机装 codex）。

## 红线

- **密钥永不进仓库/argv/日志/文档**，一律 stdin 或 env；示例用 `sk-...` 占位。
- 改 Rust 代码后必跑：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`；UI 改动跑 `cargo run -- --snapshot` 核对。
