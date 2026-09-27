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

`.github/workflows/release.yml` 会：矩阵构建 macOS arm64/x64、Linux x64/arm64、Windows x64 → 打包 `apim-vX.Y.Z-<target>.tar.gz|zip` + `.sha256` → 先建草稿 Release 再上传 → 最后转正标 latest。

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

`install.sh` 与 `cargo binstall` 都跟随 Release，**无需改动**（它们按 tag 拼 URL）。

## 5. 发布到 npm（6 个包）

包结构：主包 `apim-cli` + 5 个平台子包 `apim-cli-<os>-<arch>`（见 `scripts/publish-npm.mjs`）。

**首次发布（只能本地手动，因为 OIDC 需要包已存在）**：

```bash
npm login                        # 需要 npm 账号（免费）
node scripts/publish-npm.mjs X.Y.Z --publish
```

发布后到 npmjs.com 给这 6 个包各配一次 Trusted Publisher（仓库 `tututuhehehe/apim-cli`、workflow `publish-npm.yml`）。

**之后每次发版**：GitHub → Actions → **Publish to npm** → Run workflow，填 `X.Y.Z`。
该 workflow 用 OIDC 免 token 发布并带 provenance 签名。

**想先看产物不发布**：

```bash
node scripts/publish-npm.mjs X.Y.Z --out /tmp/apim-npm   # 只打包到 /tmp/apim-npm
```

## 6. 更新 Homebrew tap

tap 仓库：<https://github.com/tututuhehehe/homebrew-tap>（本地克隆在 `../homebrew-tap`）。
formula 里有 4 个 URL + sha256，需要每次发版更新：

```bash
# 重新算 4 个平台的 sha256
for t in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  sha=$(curl -fsSL "https://github.com/tututuhehehe/apim-cli/releases/download/vX.Y.Z/apim-vX.Y.Z-$t.tar.gz" | shasum -a 256 | cut -d' ' -f1)
  echo "$t  $sha"
done
```

把 `Formula/apim.rb` 里的 `version`、4 个 `url`、4 个 `sha256` 换成新值，然后：

```bash
cd ../homebrew-tap && git add -A && git commit -m "apim X.Y.Z" && git push
# 本地验证
brew update && brew upgrade apim   # 或 brew install tututuhehehe/tap/apim
brew test tututuhehehe/tap/apim
```

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
