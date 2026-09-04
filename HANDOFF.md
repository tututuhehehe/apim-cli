# HANDOFF — apim 交接进度

> 更新时间：2026-09-04 ｜ 会话：Grok（agent）
> 仓库：本目录（git，main 分支）
> 配套文档：AGENTS.md（架构与约定）、README.md（使用手册）

---

## 1. 当前状态（先说结论）

**TUI 已可用且在真实使用中。** 用户已实际通过 TUI 编辑过密钥（分组「默认」是用户自己改的，写盘成功）。DeepSeek 官方接口全链路打通：探活 `/models`、额度 `/user/balance`（实测 ¥4.22）。

| 能力 | 状态 |
|---|---|
| 双栏 TUI（厂商 / 密钥表 / 额度面板） | ✅ |
| 密钥增删改查（a/e/d，写回 TOML） | ✅ |
| 厂商增删改查（a/e/d，生成/删除 recipe YAML） | ✅ |
| 焦点感知复制（厂商栏=Base URL，密钥栏=token） | ✅ |
| 探活 + 额度并发查询、状态标签（可用/失败/无额度） | ✅ |
| 表单引擎（Tab 导航、⌘V 粘贴、校验、错误提示） | ✅ |
| 原子写盘（tmp+rename，600 权限） | ✅ |
| 模块按功能拆分（单文件 ≤303 行） | ✅ |
| 测试 14 个全过，clippy 零警告 | ✅ |
| CLI 子命令（给 Alfred 用） | ❌ 未开始 |
| macOS Keychain 存 token | ❌ 未开始（目前 secrets.toml，600 权限） |

## 2. 提交历史

| Commit | 内容 |
|---|---|
| `d5a2287` | 初版：双栏 TUI、DeepSeek recipe、探活+额度、复制 |
| `baeb7bd` | 厂商面板 CRUD + 通用表单引擎 + 焦点感知复制 |
| `733430f` | 按功能单元拆分模块（app/ ui/ recipe/ config/ form/，对外路径 re-export 不变） |

## 3. 关键设计决策（为什么这么做）

1. **Rust 而非 Go/Python**：产物本来就是 CLI 二进制，Alfred 要秒开；用户本机有 rustc 无 Go；「经常拓展」的痛点用 YAML recipe 解决而不是换语言。
2. **三层数据模型**：Instance（`厂商.别名` + token，天天变）→ Recipe（协议，很少变）→ App（代码）。日常加中转站 0 行 Rust。
3. **额度查询 = 请求模板 + dotted-path 解析 + 展示模板**，全部写在 recipe 里。DeepSeek 的数组多卡片（总额/充值/赠款）和 NewAPI 的单值（hard_limit_usd）是同一套引擎的两种配置。
4. **编辑保护手写配置**：厂商表单保存时，探活/额度路径没改就保留原 YAML 的 headers 和 parse/render，避免表单把 DeepSeek 的明细冲掉。
5. **删除有防线**：厂商下有密钥拒绝删；内置 recipe（origin=None）不可删只可覆盖。

## 4. 已验证的真实接口

- `GET https://api.deepseek.com/user/balance` → `{is_available, balance_infos[].{currency,total_balance,granted_balance,topped_up_balance}}`
- `GET https://api.deepseek.com/models` → 200 即探活通过

## 5. 未来扩展方向（按优先级）

### P0 — CLI 层（下一个就做这个）
Alfred 的入口。新建 `cli/` 模块复用 app/config/recipe，子命令：
- `apim list [--json]` — 全部密钥+状态（Script Filter 直接吃）
- `apim copy <厂商.别名> [--base-url]` — 复制 token 或 Base URL
- `apim status [<厂商>] [--json]` — 探活+额度，一次性输出
- `apim use <厂商.别名>` — 输出 `export OPENAI_API_KEY=... OPENAI_BASE_URL=...`（shell eval 用）
- `apim add/edit/rm` — 非交互参数版，Alfred 里也能加 Key
依赖注入建议：把 `tui.rs` 的按键路由抽干净后，CLI 与 TUI 共用 `App`（去掉 ratatui 依赖路径即可复用 probe/config）。

### P1 — 厂商生态
- 补常用 recipe：OpenAI、Anthropic、SiliconFlow、Moonshot、OpenRouter、NewAPI 系中转（`/v1/dashboard/billing/subscription` + `hard_limit_usd` 模板已就绪）
- recipe 支持 `steps:`（两步请求，如先换 token 再查额度）
- TUI 里浏览 `/models` 返回的模型列表

### P2 — 安全与体验
- token 迁入 macOS Keychain（`security` CLI 或 keyring crate；secrets.toml 作迁移源）
- 探活结果缓存 + 后台定时刷新
- 密钥排序/搜索（多了以后需要）；`supports_groups` 真正用起来（分组筛选）
- 状态栏显示剩余额度低于阈值的提醒

### P3 — 远期
- 直接输出 `.env` / `config.yaml` 片段（给 opencode、Claude Code 等工具一键配置）
- 代理测速（多中转站同一模型比延迟）
- Alfred Workflow 打包脚本（bundle 里带 apim 二进制）

## 6. 已知小坑

- 测试临时目录会留在 `target/apim-*`（已 gitignore，无害，可随手 `rm -rf target/apim-*`）。
- `serde_yaml` 已 deprecated 但无替代迁移必要，量小风险可控。
- 编辑厂商时 ID 只读（改了会断开密钥引用），重命名厂商需要「改 ID + 迁移密钥」一起做，未实现。
- TUI 里 `q`/`Esc` 在无弹窗时退出；弹窗打开时 `Esc` 只关弹窗（`tui.rs` 的 was_modal 判断）。
