# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与
[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.1] - 2026-09-28

### 新增

- `install.sh` 一行安装脚本（macOS/Linux）：识别平台 → 下载预编译二进制 → 校验 sha256；
  支持 `APIM_VERSION` 锁版本、`APIM_INSTALL_DIR` 指定目录
- npm 分发：`npm install -g apim-cli` / `npx apim-cli`（主包 + 5 个平台子包）
- Homebrew：`brew install tututuhehehe/tap/apim`
- `cargo binstall apim` 的元数据（crate 发布到 crates.io 后可用）
- `docs/RELEASING.md`：维护者发布手册

### 修复

- 测试不再依赖真实剪贴板（无显示环境如 Linux CI 会失败）
- release workflow 的发布步骤补 `--repo`（该 job 未 checkout 时会报 not a git repository）

### 变更

- npm Windows 子包名 `win32` → `windows`（`win32` 会触发 npm 名称风控 spam detection）

## [0.1.0] - 2026-09-28

首个公开版本。

### 新增

- 双栏 TUI：左侧厂商列表（带实时额度与延迟）、右侧密钥表、右下额度面板
- 厂商与密钥的增删改查、焦点感知复制、详情检查器（token 按需显隐）
- 并发探活 + 脚本化额度查询（`balance.kind: script`，密钥经 env 注入，超时 kill）
- 厂商整份复制（recipe + 额度脚本文件独立复制）
- 会话内 `Ctrl+Z` 撤销（内存与磁盘一起回退）
- CLI 子命令：`provider` / `key` / `status` / `copy` / `use`，token 只走 stdin
- `m` 键用选中的 key 拉取模型列表
- 内置 4 家 recipe：DeepSeek / OpenAI / Moonshot AI / OpenRouter
- `apim --version` 版本输出（供安装脚本与更新检测使用）
- MIT 开源协议

[Unreleased]: https://github.com/tututuhehehe/apim-cli/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.1
[0.1.0]: https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.0
