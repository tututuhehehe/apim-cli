---
name: apim
description: 用 apim CLI 管理模型厂商/API Key：厂商与密钥的增删改查、探活与余额查询、编写和绑定额度查询脚本（balance.kind=script）。当用户要加厂商、配密钥、查额度/余额、写额度脚本、或提到 apim 项目本身时使用。
---

# apim 使用指南（CLI）

apim 是本仓库的终端 API Key 管理器（TUI + CLI）。心智模型：**TUI 管人，CLI 管机器（AI/脚本）**，同一份数据（`~/.config/apim/`），互相实时可见。命令行入口 `apim`（`cargo install --path .` 安装；开发时用 `target/debug/apim`）。

## 命令速查

```
# 厂商 CRUD
apim provider ls [--json]
apim provider add <id> --name <名> --base-url <URL> [--homepage <主页URL>|none] [--health <路径>|none] [--script <脚本路径>|none]
apim provider set <id> [--name <名>] [--base-url <URL>] [--homepage <主页URL>|none] [--health <路径>|none] [--script <脚本路径>|none]
apim provider rm <id> [--force]          # 有密钥时拒绝；--force 连带删密钥；内置(deepseek/openai/moonshot/openrouter)不可删

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

- `--script none` = 解绑额度脚本；空串（`--script ""`）同义。`--health none` = 不探活。`--homepage` 是厂商控制面板主页，TUI 选中厂商按 `Enter` 用默认浏览器打开。
- `provider set` 只改传了的字段，是显式整体替换 `--script`，没有 TUI 的「未改保留」语义。
- 坏配置（recipe 误删 / secrets 缺条目）不会锁死 CLI：命令会跳过坏条目并打警告，下一次成功写盘自动清除；TUI 则保持严格报错。
- 测试/沙盒：设 `APIM_CONFIG_DIR=<临时目录>` 重定向整个配置目录，绝不碰真实 `~/.config/apim`。

## 余额查询脚本（balance.kind=script）

额度查不了的怪接口厂商（多请求、要算日期、要查表映射）走脚本逃生舱：apim 带着密钥跑脚本，stdout 逐行进额度面板。完整的代写提示词（给任意 AI 用）在 **`docs/quota-script-prompt.md`**，接新厂商时整体复制它并附官方查询方式即可。

### 脚本契约（必须严格遵守）

- **env 注入**：`APIM_TOKEN`（密钥）、`APIM_BASE_URL`、`APIM_ALIAS`、`APIM_PROVIDER`，recipe `vars:` 每项 → `APIM_VAR_<大写名>`。密钥只走 env。
- **exit 0**：stdout 逐行显示（首行高亮，建议 1~4 行）；**exit 非 0**：stderr（截断 200 字符）显示为红色错误；超时（recipe `timeout_secs`，缺省 15s）杀进程；stdout 超 50 行截断。
- 已知坑：HTTP 200 不等于成功（有的厂商坏 Key 也回 200，要校验 body 里的 success/code 字段）；jq 数值可能是浮点，运算前取整。

### 绑定与解绑

脚本建议放 `~/.config/apim/scripts/<厂商id>-quota.sh`（可执行）。三个等价入口：CLI `--script`、TUI 表单「脚本路径」、直接写 recipe YAML。绑定后 `apim status <厂商>` 与 TUI 面板跑同一脚本，结果一致。

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

现有实例参考：GLM = `~/.config/apim/recipes/glm.yaml` + `~/.config/apim/scripts/glm-quota.sh`（zsh + curl + jq，含信封校验）。

## 红线

- **密钥永不进仓库/argv/日志/文档**，一律 stdin 或 env；示例用 `sk-...` 占位。
- 改 Rust 代码后必跑：`cargo fmt && cargo clippy -q --all-targets -- -W clippy::all`（零警告）+ `cargo test`；UI 改动跑 `cargo run -- --snapshot` 核对。
