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

| 键 | 作用 |
|---|---|
| `j` / `↓` | 往下 |
| `k` / `↑` | 往上 |
| `Tab` / `←` / `→` | 左边厂商 ↔ 右边密钥表 |
| `c` | 复制当前密钥（完整 key，不是掩码） |
| `a` | 添加密钥 |
| `e` | 编辑当前密钥 |
| `d` | 删除当前密钥（先确认） |
| `r` | 刷新当前厂商的状态和额度 |
| `q` / `Esc` | 退出 |

## 增删改查

**添加（`a`）**：弹出表单，填别名、分组（可空）、密钥，厂商用 `←`/`→` 切换。密钥可以直接 `⌘V` 粘贴。`Enter` 保存，立即写盘并自动检测。

**编辑（`e`）**：表单里带出当前值。改别名就是重命名，改分组、换 token 都在这里。改完 `Enter`，左边的 `provider.别名` 两边文件会自动同步。

**删除（`d`）**：弹确认框，`Enter` 删，`Esc` 取消。

**查**：列表本身就是查——状态列是探活结果（`● 可用` / `● 失败` / `● 无额度`），右下角额度面板显示选中密钥的余额。

表单内：

| 键 | 作用 |
|---|---|
| `Tab` | 下一项 |
| `←` / `→` | 移动光标；在「厂商」行是切换厂商 |
| `Enter` | 保存 |
| `Esc` | 取消 |

别名或密钥为空、或 `厂商.别名` 重复时，底部会红字提示，不会写盘。

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

把 YAML 放到 `~/.config/apim/recipes/`，不用重新编译。可抄 `recipes/deepseek.yaml`。

```bash
mkdir -p ~/.config/apim/recipes
cp recipes/deepseek.yaml ~/.config/apim/recipes/my-relay.yaml
```

改里面的 `id`、`name`、`base_url`，以及额度接口的 `url` / JSON 路径。之后在 TUI 里按 `a`，厂商就能 `←`/`→` 切到它。
