# 发布流程（RELEASING）

维护者手册：怎么发一个版本，以及各分发渠道分别怎么更新。

## 版本号：单一事实来源

唯一来源是 `Cargo.toml` 的 `version`。发版时改它 + 补 `CHANGELOG.md`，git tag 用 `v` 前缀（`v0.1.0`），npm 包版本不带 `v`。

## 1. 发版前检查

```bash
rustup update stable          # 本地工具链与 CI 对齐，避免"本地过 CI 挂"
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
node --check npm/bin/apim.js && node --check scripts/publish-npm.mjs
```

## 2. 改版本 + 记 CHANGELOG

- `Cargo.toml`：`version = "X.Y.Z"`（不要动 `Cargo.lock` 之外的东西）
- `CHANGELOG.md`：把 `[Unreleased]` 内容落到 `[X.Y.Z] - YYYY-MM-DD`，更新底部 compare 链接

提交（中文一行主题）：

```bash
git add -A && git commit -m "vX.Y.Z：<一句话>"
```

## 3. 打 tag → 自动构建 5 平台

```bash
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main
git push origin vX.Y.Z
```

`.github/workflows/release.yml` 会：矩阵构建 macOS arm64/x64、Linux x64/arm64、Windows x64 → 打包 `apim-vX.Y.Z-<target>.tar.gz|zip` + `.sha256` → 先建草稿 Release 再上传 → 最后转正标 latest，并**额外发布 `install.sh.sha256`**（`apim update` 的 install.sh 渠道要靠它校验脚本；缺了它那条渠道会拒绝执行并给手动命令）。

CI（`.github/workflows/ci.yml`）同时在 main 上跑 fmt/clippy/test/JS 语法检查。

## 4. 验证 Release

```bash
gh run list --limit 3
gh release view vX.Y.Z
# 抽查：下载 + 校验 + 跑版本号
curl -fsSL -O https://github.com/tututuhehehe/apim-cli/releases/download/vX.Y.Z/apim-vX.Y.Z-aarch64-apple-darwin.tar.gz
curl -fsSL -O https://github.com/tututuhehehe/apim-cli/releases/download/vX.Y.Z/apim-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256
shasum -a 256 -c apim-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256
tar -xzf apim-vX.Y.Z-aarch64-apple-darwin.tar.gz && ./apim --version
```

`install.sh` 与 `cargo binstall` 都跟随 Release，**无需改动**（它们按 tag 拼 URL）。Release 里现在还有一个 `install.sh.sha256` 资产，同样是 CI 自动生成的，不要手改。

## 5. 发布到 npm（6 个包）

包结构：主包 `apim-cli` + 5 个平台子包 `apim-cli-<os>-<arch>`（见 `scripts/publish-npm.mjs`）。

**首次发布（只能本地手动，因为 OIDC / Trusted Publisher 需要包已存在）**

npm 现在要求发布必须满足其一：账号开启 2FA，或使用**带 Bypass 2FA 的 granular access token**。
npm 的 2FA 是**安全密钥 / 通行证（WebAuthn：Touch ID、Face ID、实体密钥）**，**没有 6 位验证码**，
所以 CLI 非交互发布必须走 token：

1. https://www.npmjs.com/settings/<你的用户名>/tokens → Generate New Token → **Granular Access Token**
2. 填 Name、Expiration（7 天足够）、Permissions **Read and write**，
   并钩上 ✅ **Bypass two-factor authentication (2FA)**
3. 生成后设置到本地（令牌是凭据：别提交、别写进文档/日志）：

   ```bash
   npm config set //registry.npmjs.org/:_authToken=npm_xxxx
   ```

4. 发布，然后**撤销 token**（一次性用完即弃）：

   ```bash
   node scripts/publish-npm.mjs X.Y.Z --publish
   npm config delete //registry.npmjs.org/:_authToken
   ```

发布后到 npmjs.com 给这 6 个包各配一次 Trusted Publisher（仓库 `tututuhehehe/apim-cli`、
workflow `publish-npm.yml`），之后每次发版走 Actions（OIDC，免 token）。

**之后每次发版**：GitHub → Actions → **Publish to npm** → Run workflow，填 `X.Y.Z`。
该 workflow 用 OIDC 免 token 发布并带 provenance 签名。

**想先看产物不发布**：

```bash
node scripts/publish-npm.mjs X.Y.Z --out /tmp/apim-npm   # 只打包到 /tmp/apim-npm
```

## 6. 更新 Homebrew tap

tap 仓库：<https://github.com/tututuhehehe/homebrew-tap>（本地 clone 在 `../homebrew-tap`）。
一条命令完成（重算 4 平台 sha256 → 重写 formula → commit + push）：

```bash
scripts/update-tap.sh X.Y.Z
```

验证：

```bash
brew update && brew upgrade apim        # 或首次：brew install tututuhehehe/tap/apim
brew test tututuhehehe/tap/apim
```

> 手动做法（脚本失效时）：重算下面 4 个 sha256，填回 `Formula/apim.rb` 的 `version` 与 4 组 `url`/`sha256`。
> ```bash
> for t in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
>   curl -fsSL "https://github.com/tututuhehehe/apim-cli/releases/download/vX.Y.Z/apim-vX.Y.Z-$t.tar.gz" | shasum -a 256
> done
> ```

## 7. 回滚

- **删掉误发的 Release/tag**：`gh release delete vX.Y.Z --yes --cleanup-tag`（注意会连本地 tag 一起清）
- **本地回退**：`git revert <提交>`（整体）或 `git checkout <提交> -- <路径>`（局部）
- **npm 撤版**：发布 72 小时内可 `npm unpublish <pkg>@<ver>`；超时只能发新版本覆盖（`npm deprecate` 标记旧版）

## 各渠道速查

| 渠道 | 命令 | 是否需手动维护 |
|---|---|---|
| install.sh | `curl -fsSL .../install.sh \| sh` | 否（跟随 latest） |
| cargo binstall | `cargo binstall apim` | 否（跟随 Release），但需 crate 已在 crates.io |
| npm | `npm install -g apim-cli` | 是（每次手动触发 workflow） |
| Homebrew | `brew install tututuhehehe/tap/apim` | 是（每次改 formula + sha） |
| 手动下载 | Releases 页 | 否 |
| 源码 | `cargo install --path .` | 否 |

> `cargo install apim` / `cargo binstall apim` 需要 crate 发布到 crates.io（`Cargo.toml` 的 `publish = false` 要先删）。binstall 元数据已配好，发布后即可用。

> `apim update` 只认**三条**自动更新渠道：install.sh（裸二进制：下载**该 tag 的**官方脚本 → 形状校验 + 按 `install.sh.sha256` 校验摘要 → 再执行，钉住原安装目录）、npm（`npm install -g apim-cli@latest`）、Homebrew（`brew upgrade apim`）。**`target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本会被拒绝更新**（返回 None + 给指引），前者免得把开发二进制覆盖成 Release 版，后者是 `cargo install` 留下的多余副本 —— 所以上表里 `cargo install` / `cargo binstall` 那两行**不在** `apim update` 覆盖范围内。新增渠道时同步改 `src/cli/update/channel.rs` 的 `Channel` 与识别规则、单测与本节。`apim uninstall` 复用同一套识别（`update::channel::detect_channel`）：新渠道的卸载动作会被 `src/cli/uninstall/mod.rs` 里 `uninstall_program` 的穷尽 `match` 拦下，编译器会逼你补 —— 但识别规则、命令行与会话提示仍要手工确认。
