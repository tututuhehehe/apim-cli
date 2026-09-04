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
| `e` | 编辑厂商（名称 / Base URL / 探活 / 额度） | 编辑密钥（别名 / 分组 / token） |
| `d` | 删除厂商（须先删光它下面的密钥） | 删除密钥 |
| `c` | 复制 Base URL | 复制密钥 |
| `j` / `k` / `Tab` | 移动 / 切换左右栏 | 同左 |
| `r` | 刷新状态和额度 | 同左 |
| `q` / `Esc` | 退出 | 退出 |

## 增删改查

**添加密钥（右侧按 `a`）**：填别名、分组（可空）、密钥，厂商用 `←`/`→` 切换。密钥可以直接 `⌘V` 粘贴。`Enter` 保存，立即写盘并自动检测。

**添加厂商（左侧按 `a`）**：填 ID（小写字母/数字/-，密钥配置里 `provider` 引用它）、显示名称、Base URL，以及两个可选接口：

- 探活路径：默认 `/models`，拼在 Base URL 后面；留空 = 不探活
- 额度路径 + 额度取值：比如 NewAPI 系中转站填 `/v1/dashboard/billing/subscription` + `hard_limit_usd`

保存后生成 `~/.config/apim/recipes/<id>.yaml`，接着按 `a` 就能给它加密钥。

**编辑（`e`）**：密钥表单带出当前值，改别名就是重命名。厂商表单编辑时 ID 锁定；如果探活/额度的路径没改，会保留原 YAML 里手写的 headers、解析规则（DeepSeek 的三行额度明细不会被表单冲掉）。

**删除（`d`）**：都弹确认框。厂商下面还有密钥时会拒绝，先删密钥。内置的 DeepSeek 厂商不可删除，但可以 `e` 编辑覆盖（会在用户目录生成同名 YAML）。

**查**：状态列是探活结果（`● 可用` / `● 失败` / `● 无额度`），右下角额度面板显示选中密钥的余额。

表单内：

| 键 | 作用 |
|---|---|
| `Tab` / `↑` / `↓` | 下一项 / 上一项 |
| `←` / `→` | 移动光标；在「厂商」行是切换厂商 |
| `Enter` | 保存 |
| `Esc` | 取消 |

必填项为空、ID 重复、Base URL 不以 `http(s)://` 开头等，底部红字提示，不会写盘。

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
