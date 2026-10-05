#!/bin/sh
# 把 homebrew-tap 的 Formula/apim.rb 更新到指定版本：重算 4 平台 sha256 →
# 重写 formula → git commit + push。
#
#   scripts/update-tap.sh <version> [tap 目录，默认 ../homebrew-tap]
#
# 前置：对应 tag 的 Release 资产已存在；tap 目录是一个已 clone 的 homebrew-tap。
set -eu

REPO="tututuhehehe/apim-cli"
VERSION="${1:?用法: scripts/update-tap.sh <version> [tap目录]}"
TAP_DIR="${2:-../homebrew-tap}"
TAG="v$VERSION"
FORMULA="$TAP_DIR/Formula/apim.rb"

[ -f "$FORMULA" ] || {
    echo "找不到 $FORMULA" >&2
    echo "请先 git clone git@github.com:tututuhehehe/homebrew-tap.git，或用第二个参数指定目录" >&2
    exit 1
}

# 空输入的 sha256。`curl | shasum` 的管道退出码是 shasum 的，`set -eu` 抓不到 curl 失败 ——
# 网络抽风时会把空哈希当真值写进 formula 并推送（v0.1.7 就是这么坏的）。
EMPTY_SHA="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

sha() {
    tmp="/tmp/apim-tap-$TAG-$1-$$.tar.gz"
    if ! curl -fsSL --retry 3 --retry-delay 2 \
        "https://github.com/$REPO/releases/download/$TAG/apim-$TAG-$1.tar.gz" -o "$tmp"; then
        rm -f "$tmp"
        echo "下载 $1 失败，中止（formula 未改动）" >&2
        exit 1
    fi
    if [ ! -s "$tmp" ]; then
        rm -f "$tmp"
        echo "下载 $1 得到空文件，中止（formula 未改动）" >&2
        exit 1
    fi
    h=$(shasum -a 256 "$tmp" | cut -d' ' -f1)
    rm -f "$tmp"
    if [ "$h" = "$EMPTY_SHA" ]; then
        echo "下载 $1 的摘要异常，中止（formula 未改动）" >&2
        exit 1
    fi
    printf '%s\n' "$h"
}

# 四个摘要全部拿到才开始重写 formula：任一失败就退出，不会留下半截/假值
echo "计算 $TAG 的 4 个 sha256…"
MAC_ARM=$(sha aarch64-apple-darwin)
MAC_X64=$(sha x86_64-apple-darwin)
LIN_ARM=$(sha aarch64-unknown-linux-gnu)
LIN_X64=$(sha x86_64-unknown-linux-gnu)

cat >"$FORMULA" <<EOF
class Apim < Formula
  desc "Terminal manager for model-provider API keys (TUI + CLI)"
  homepage "https://github.com/$REPO"
  version "$VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/$REPO/releases/download/$TAG/apim-$TAG-aarch64-apple-darwin.tar.gz"
      sha256 "$MAC_ARM"
    end
    on_intel do
      url "https://github.com/$REPO/releases/download/$TAG/apim-$TAG-x86_64-apple-darwin.tar.gz"
      sha256 "$MAC_X64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/$REPO/releases/download/$TAG/apim-$TAG-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$LIN_ARM"
    end
    on_intel do
      url "https://github.com/$REPO/releases/download/$TAG/apim-$TAG-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$LIN_X64"
    end
  end

  def install
    bin.install "apim"
  end

  test do
    assert_match "apim #{version}", shell_output("#{bin}/apim --version")
  end
end
EOF

echo "已重写 $FORMULA"

cd "$TAP_DIR"
git add -A
if git diff --cached --quiet; then
    echo "formula 无变化，跳过提交"
else
    git commit -q -m "apim $VERSION"
    git push
    echo "已提交并推送 tap"
fi
