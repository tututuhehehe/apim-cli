#!/bin/sh
# apim 安装脚本：从 GitHub Releases 下载对应平台的预编译二进制并校验 sha256。
#
#   curl -fsSL https://raw.githubusercontent.com/tututuhehehe/apim-cli/main/install.sh | sh
#
# 环境变量：
#   APIM_VERSION      指定版本（如 v0.1.0），默认取最新 Release
#   APIM_INSTALL_DIR  安装目录，默认 /usr/local/bin（可写时）否则 ~/.local/bin
#
# 支持平台：macOS arm64/x64、Linux x64/arm64（Windows 请下载 .zip 手动放置）。
set -eu

REPO="tututuhehehe/apim-cli"
BIN="apim"
VERSION="${APIM_VERSION:-latest}"

err() {
    printf 'apim installer: %s\n' "$1" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || err "缺少依赖：$1"
}

# ---- 探测平台 → target triple -------------------------------------------------
detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin)
            case "$arch" in
                arm64 | aarch64) echo "aarch64-apple-darwin" ;;
                x86_64 | amd64) echo "x86_64-apple-darwin" ;;
                *) err "不支持的 macOS 架构：$arch" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                aarch64 | arm64) echo "aarch64-unknown-linux-gnu" ;;
                x86_64 | amd64) echo "x86_64-unknown-linux-gnu" ;;
                *) err "不支持的 Linux 架构：$arch" ;;
            esac
            ;;
        *)
            err "不支持的系统：$os（Windows 请从 Releases 下载 .zip 手动安装）"
            ;;
    esac
}

# ---- 解析版本号 ---------------------------------------------------------------
resolve_version() {
    if [ "$VERSION" != "latest" ]; then
        printf '%s' "$VERSION"
        return
    fi
    # 跟随 /releases/latest 的重定向拿到 tag，例如 .../releases/tag/v0.1.0
    url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")" ||
        err "无法访问 GitHub Releases（网络问题？）"
    tag="${url##*/}"
    [ -n "$tag" ] || err "无法解析最新版本号"
    printf '%s' "$tag"
}

# ---- 选择安装目录 -------------------------------------------------------------
pick_dir() {
    if [ -n "${APIM_INSTALL_DIR:-}" ]; then
        printf '%s' "$APIM_INSTALL_DIR"
    elif [ -w /usr/local/bin ] 2>/dev/null; then
        printf '%s' "/usr/local/bin"
    else
        printf '%s' "$HOME/.local/bin"
    fi
}

main() {
    need curl
    need tar
    command -v shasum >/dev/null 2>&1 || command -v sha256sum >/dev/null 2>&1 ||
        err "缺少依赖：shasum 或 sha256sum（用于校验下载）"

    target="$(detect_target)"
    tag="$(resolve_version)"
    archive="$BIN-$tag-$target.tar.gz"
    base="https://github.com/$REPO/releases/download/$tag"
    dir="$(pick_dir)"

    printf 'apim installer: %s %s → %s\n' "$tag" "$target" "$dir"

    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT INT TERM

    printf 'apim installer: 下载 %s\n' "$archive"
    curl -fsSL -o "$tmp/$archive" "$base/$archive" ||
        err "下载失败：$base/$archive（该版本可能没有此平台的构建）"
    curl -fsSL -o "$tmp/$archive.sha256" "$base/$archive.sha256" ||
        err "下载校验和失败：$base/$archive.sha256"

    printf 'apim installer: 校验 sha256\n'
    (cd "$tmp" && {
        if command -v shasum >/dev/null 2>&1; then
            shasum -a 256 -c "$archive.sha256" >/dev/null
        else
            sha256sum -c "$archive.sha256" >/dev/null
        fi
    }) || err "sha256 校验失败，已中止（下载可能被篡改或损坏）"

    tar -xzf "$tmp/$archive" -C "$tmp"
    [ -f "$tmp/$BIN" ] || err "压缩包内未找到 $BIN"

    mkdir -p "$dir"
    # 先装到临时名再原子替换，避免覆盖正在运行的二进制时报错
    install -m 755 "$tmp/$BIN" "$dir/$BIN.tmp.$$" 2>/dev/null ||
        { cp "$tmp/$BIN" "$dir/$BIN.tmp.$$" && chmod 755 "$dir/$BIN.tmp.$$"; }
    mv -f "$dir/$BIN.tmp.$$" "$dir/$BIN"

    printf 'apim installer: 已安装 %s\n' "$("$dir/$BIN" --version 2>/dev/null || echo "$BIN $tag")"

    case ":$PATH:" in
        *":$dir:"*) ;;
        *)
            printf '\n注意：%s 不在 PATH 中，请把它加进去，例如：\n' "$dir"
            printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.profile   # 或 ~/.zshrc\n' "$dir"
            ;;
    esac
}

main "$@"
