# apim 额度脚本代写提示词

> **用法**：把本文件从下一行的分隔线开始、整体复制给任意 AI Agent（ZCode / Claude / ChatGPT 都行），
> 然后把最末「厂商信息」一节填上你要接的厂商的官方查询方式（文档、curl 示例、或口述），
> Agent 会产出两个文件 + 验证命令。密钥永远不要贴给 Agent，验证时用环境变量自己传。
> 接的不是大模型（翻译、搜索这类 API）时，在「厂商信息」里说清楚，Agent 会按 1.5 节走非模型厂商那套。

---

你是一名「API 额度查询脚本」工程师。apim 是一个本地运行的模型厂商 API Key 管理器（Rust 单二进制 TUI），
它对「厂商额度怎么查」预留了一个**通用脚本接口**。你的任务：根据用户在文末提供的厂商官方查询方式，
产出 **一个查询脚本** 和 **一份 recipe YAML**，并给出安装与验证步骤。

## 1. 接口契约（必须严格遵守）

### 1.1 recipe YAML

apim 通过 `~/.config/apim/recipes/<id>.yaml` 认识厂商。额度走脚本时这样写：

```yaml
id: <厂商id，小写字母/数字/->
name: <显示名>
base_url: <厂商 API 根地址>
auth:
  kind: bearer        # 或 header（裸 Key 无 Bearer 前缀）
health:               # 探活：一个简单 GET，HTTP 2xx 即算活
  method: GET
  url: '{base_url}/models'
balance:
  kind: script        # 关键：额度查询交给脚本
  command: ~/.config/apim/scripts/<厂商id>-quota.sh
  timeout_secs: 15    # 可选，缺省 15
```

`auth` 同时服务于探活请求：`bearer` 发 `Authorization: Bearer <key>`，`header` 发 `Authorization: <key>`。

### 1.2 apim 会注入的环境变量（脚本从 env 拿密钥，不要让用户提供）

| 变量 | 含义 |
|---|---|
| `APIM_TOKEN` | 当前密钥的 API Key（用户在 TUI 里存的） |
| `APIM_BASE_URL` | recipe 的 base_url |
| `APIM_ALIAS` / `APIM_PROVIDER` | 密钥别名 / 厂商 id |
| `APIM_VAR_<大写名>` | recipe `vars:` 里的自定义变量（如访问令牌 → `APIM_VAR_ACCESS_TOKEN`） |

### 1.3 输出契约

- **exit 0**：stdout 逐行显示在 apim 额度面板，**第一行会被高亮**（放最重要的数字）。行数建议 1~4 行。
- **exit 非 0**：stderr 的内容（截断 200 字符）显示为红色错误。请求失败、Key 无效都要走这条路。
- **超时**（缺省 15s）：进程被杀，显示「脚本超时」。脚本里每个网络请求自带 `--max-time`。
- stdout 为空也算失败（「脚本无输出」）。

### 1.4 已知坑（GLM 实测踩过的）

1. **HTTP 200 不等于成功**：有的厂商（如 GLM monitor 接口）坏 Key 也回 200，错误藏在 body 里
   （`{"success":false}` / `"code":401`）。必须校验响应体里的状态字段，失败就 stderr + exit 1。
2. **数值可能是浮点**：做整数运算前先取整（`printf '%.0f'` 或 jq `floor`）。
3. `~` 开头的路径只在 recipe 的 `command:` 里会被展开，脚本内部自己引用路径要展开或用绝对路径。
4. 密钥只在 env 里，**绝不能**写进脚本、写进 recipe、echo 到输出。
5. 脚本可以用本机任何工具：curl、jq、python3 等；macOS 无 jq 时提醒用户 `brew install jq`。

### 1.5 非模型厂商（翻译 / 搜索这类 API）

大模型以外的 API（翻译、搜索、图像……）在 apim 里是**非模型厂商**，`kind: non_model` 标明。
它一样有密钥 / 别名 / 分组 / 主页 / 额度脚本，但**没有模型列表、不能导入客户端、也没有探活**
——所以 recipe 里不写 `health`，`auth` 也可以整段省略（只服务 HTTP 请求，非模型没有请求）：

```yaml
# ~/.config/apim/recipes/deepl.yaml
id: deepl
name: DeepL 翻译
kind: non_model        # 关键：非模型厂商
base_url: https://api-free.deepl.com
homepage: https://www.deepl.com/your-account
balance:
  kind: script
  command: ~/.config/apim/scripts/deepl-quota.sh
```

要点：

- **额度脚本是这类厂商唯一的接入点**：调哪个端点、key 放 header 还是 query、怎么算「还剩多少」，
  全在脚本里。面板与 `apim status` 的状态列就是**脚本的成败**（脚本跑通 = 可用），所以失败路径更要写准
  （坏 key 必须 stderr + exit 1，不能吐假数字）。
- 注册用 `apim provider add <id> --name <名> --base-url <URL> --kind non-model --script <路径>`；
  **不要传 `--health`**（给非模型厂商传探活会被直接拒绝）。
- 双凭据（Access Key + Secret、或需要 region / project id 这类额外参数）的厂商：把额外值放 recipe 的
  `vars:`，脚本里是 `APIM_VAR_<大写名>`；TUI 表单不编辑 `vars`，直接写 YAML。
- 类型创建后不可改（`provider set --kind` 会被拒绝），要换就删了重建或 `provider copy`。

## 2. 参考实现（GLM Coding Plan，实测可用）

`~/.config/apim/recipes/glm.yaml`：

```yaml
id: glm
name: GLM Coding Plan
base_url: https://open.bigmodel.cn
auth:
  kind: header        # monitor 接口吃裸 Key，无 Bearer 前缀
health:
  method: GET
  url: '{base_url}/api/monitor/usage/quota/limit'
balance:
  kind: script
  command: ~/.config/apim/scripts/glm-quota.sh
  timeout_secs: 15
```

`~/.config/apim/scripts/glm-quota.sh`（两个接口、jq 挑窗口、按窗口算百分比）：

```zsh
#!/bin/zsh
# apim 额度脚本：GLM Coding Plan
# 契约见 docs/quota-script-prompt.md；依赖 curl + jq

set -u

KEY="${APIM_TOKEN:-}"
BASE="${APIM_BASE_URL:-https://open.bigmodel.cn}"

if [[ -z "$KEY" ]]; then
  echo "APIM_TOKEN 未注入" >&2
  exit 1
fi

AUTH="Authorization: $KEY"

# 坏 Key 也回 HTTP 200，必须按 body 里的 success 字段判断
check_envelope() { # $1=响应体 $2=接口名
  if ! echo "$1" | jq -e '.success == true' >/dev/null 2>&1; then
    local msg
    msg="$(echo "$1" | jq -r '.msg // "返回异常"' 2>/dev/null)" || msg="返回异常"
    echo "$2: ${msg}" >&2
    exit 1
  fi
}

QUOTA="$(curl -sS --connect-timeout 5 --max-time 10 \
  -H "$AUTH" -H "Accept-Language: zh-CN,zh" \
  "$BASE/api/monitor/usage/quota/limit")"

if [[ $? -ne 0 || -z "$QUOTA" ]]; then
  echo "额度接口请求失败，请检查 Key 或网络" >&2
  exit 1
fi
check_envelope "$QUOTA" "额度接口"

# data.limits[] 里按 unit/number 挑窗口：unit=3,number=5 是 5 小时；unit=6,number=1 是周
pick_limit() {
  echo "$QUOTA" | jq -c --argjson u "$1" --argjson n "$2" \
    '[.data.limits[]? | select(.type == "CREDIT_LIMIT" and .unit == $u and .number == $n)][0]'
}

to_int() {  # jq 数值可能是浮点，统一取整
  printf '%.0f' "${1:-0}" 2>/dev/null || echo 0
}

pct() { # remaining total -> 整数百分比
  local r="$(to_int "$1")" t="$(to_int "$2")"
  if [[ "$t" -gt 0 ]]; then echo $(( r * 100 / t )); else echo 0; fi
}

show_window() { # $1=标签 $2=unit $3=number
  local item="$(pick_limit "$2" "$3")"
  if [[ -z "$item" || "$item" == "null" ]]; then
    echo "$1  ?/?"
    return
  fi
  local total remaining
  total="$(echo "$item" | jq -r '.usage // 0')"        # usage = 窗口总额度
  remaining="$(echo "$item" | jq -r '.remaining // 0')"
  echo "$1  ${remaining}/${total} · $(pct "$remaining" "$total")%"
}

show_window "5小时" 3 5
show_window "周额度" 6 1

# 接口不返回 MCP 的额度/剩余，只展示事实：本月已用
MONTH_START="$(date '+%Y-%m-01 00:00:00')"
NOW="$(date '+%Y-%m-%d %H:%M:%S')"

TOOL="$(curl -sS --connect-timeout 5 --max-time 10 --get \
  -H "$AUTH" -H "Accept-Language: zh-CN,zh" \
  --data-urlencode "startTime=$MONTH_START" \
  --data-urlencode "endTime=$NOW" \
  "$BASE/api/monitor/usage/tool-usage")"

MCP_USED=0
if [[ -n "$TOOL" ]] && echo "$TOOL" | jq -e '.success == true' >/dev/null 2>&1; then
  MCP_USED="$(to_int "$(echo "$TOOL" | jq -r '.data.totalUsage.totalSearchMcpCount // 0')")"
fi

LEVEL="$(echo "$QUOTA" | jq -r '.data.level // "?"')"
echo "MCP    本月已用 ${MCP_USED} · ${LEVEL}"
```

## 3. 你要产出的东西

1. `~/.config/apim/scripts/<厂商id>-quota.sh` —— 查询脚本（zsh 或 bash，带 shebang，注释说明每个接口）
2. `~/.config/apim/recipes/<厂商id>.yaml` —— recipe（按 1.1 的格式）
3. 注册与验证命令（见第 4 节，用 apim CLI）

要求：

- 严格按第 1 节契约；失败路径（网络错、坏 Key、字段缺失）都要 stderr + exit 1，不能打印假数字。
- 输出 1~4 行，第一行放最重要的剩余额度。
- 接口返回的字段含义不确定时，先向用户确认，别猜语义（尤其是「usage 是总额还是已用」这类）。
- 如果厂商只提供网页没有 API，明确告诉用户做不到 / 需要什么（cookie 等），不要硬编。

## 4. 注册与验证（产出物里要包含这段）

优先用 apim CLI（入口 `apim`，跑本仓库的代码用 `cargo run -- <args>`；文件本身就是数据源，CLI 不可用时直接把两个文件放到位也等效）：

```bash
# 1. 写入两个文件后，注册厂商并绑定脚本：
#    模型厂商：
apim provider add <厂商id> --name <名称> --base-url <https://...> \
  --health <探活路径> --script ~/.config/apim/scripts/<厂商id>-quota.sh
#    非模型厂商（翻译 / 搜索…，无探活、无模型列表）：
apim provider add <厂商id> --name <名称> --base-url <https://...> \
  --kind non-model --script ~/.config/apim/scripts/<厂商id>-quota.sh

# 2. 密钥由用户自己配（token 只走 stdin，AI 不要经手）：
echo 'sk-你的密钥' | apim key add <厂商id> main

# 3. 验证（AI 可直接跑，期望 balance.ok=true 且 lines 有额度行）：
apim status <厂商id> --json
```

改绑 / 解绑：`apim provider set <厂商id> --script <新路径|none>`。

无 CLI 兜底（纯文件）：

```bash
mkdir -p ~/.config/apim/scripts ~/.config/apim/recipes
chmod +x ~/.config/apim/scripts/<厂商id>-quota.sh
chmod 600 ~/.config/apim/recipes/<厂商id>.yaml
# 验证（key 走环境变量，不要写进文件）：
APIM_TOKEN='sk-你的密钥' APIM_BASE_URL='https://...' ~/.config/apim/scripts/<厂商id>-quota.sh
echo "exit=$?"   # 期望 exit=0 且输出额度行
```

## 5. 厂商信息（用户填这里）

- 是不是大模型厂商？（是 / 不是；不是就是非模型厂商，写 `kind: non_model`、不写 health）
- 厂商名称 / 想用的 id：
- base_url：
- 官方查询方式（文档链接 / curl 示例 / 接口返回样例，尽量贴全）：
- 鉴权方式（Bearer？裸 Key header？别的？）：
- 想在面板上看到哪些数字：
