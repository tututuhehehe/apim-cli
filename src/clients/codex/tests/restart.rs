//! 重启 codex 守护进程（`restart`）的测试：进程匹配与包装形态识别。

use crate::clients::codex::RestartReport;
use crate::clients::codex::restart::{
    is_codex_server, looks_like_wrapped_codex_server, restart_daemon_with,
};

/// 匹配必须严格：只杀真正的 codex app-server，不能误杀用户会话或无关进程。
#[cfg(unix)]
#[test]
fn codex_server_matching_is_strict() {
    // 真 daemon（daemon + pid-update-loop 两个形态）
    assert!(is_codex_server(
        " 87965 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex app-server --listen unix:// --managed-daemon"
    ));
    assert!(is_codex_server(
        " 87990 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex app-server daemon pid-update-loop"
    ));
    // 用户的 codex 会话：没有 app-server 子命令 → 不杀
    assert!(!is_codex_server(" 86760 node /opt/homebrew/bin/codex"));
    assert!(!is_codex_server(
        " 86761 /opt/homebrew/lib/node_modules/@openai/codex/vendor/codex"
    ));
    // 命令行里恰好含这两个词的无关进程（apim 自己的测试/ps|grep）→ 绝不能杀
    assert!(!is_codex_server(
        " 92087 /opt/homebrew/bin/bash -c echo \"codex app-server\" | grep foo"
    ));
    assert!(!is_codex_server(
        " 92091 awk /codex/ && /app-server/ {print}"
    ));
    // code-mode host 是另一个二进制名 → 不在范围内
    assert!(!is_codex_server(
        " 8272 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex-code-mode-host"
    ));
    // 用户自己的会话里恰好出现 app-server 这个词 → 绝不能杀（子命令位不是它）
    assert!(!is_codex_server(
        " 100 /opt/homebrew/bin/codex --profile app-server"
    ));
    assert!(!is_codex_server(
        " 101 /opt/homebrew/bin/codex exec app-server"
    ));
    assert!(!is_codex_server(
        " 102 /opt/homebrew/bin/codex -c model_provider=app-server"
    ));
    // 反向：真实守护形态（子命令就在 argv[2]）必须命中
    assert!(is_codex_server(
        " 103 /Users/x/.codex/packages/app-server-daemon/releases/0.159.2-aarch64-apple-darwin/bin/codex app-server --listen unix://"
    ));
}

/// 形态不标准的「像 daemon」只用来提示，绝不据此杀进程。
#[cfg(unix)]
#[test]
fn wrapped_daemon_shapes_are_only_reported() {
    // 包装器 + 独立的 codex + 独立的 app-server → 计数
    assert!(looks_like_wrapped_codex_server(
        " 200 /bin/sh -c \"codex app-server\""
    ));
    assert!(looks_like_wrapped_codex_server(
        " 201 /usr/bin/env -i /opt/homebrew/bin/codex app-server"
    ));
    // 标准形态由 is_codex_server 负责，不该重复计进 ambiguous
    assert!(!looks_like_wrapped_codex_server(
        " 202 /opt/homebrew/bin/codex app-server --listen unix://"
    ));
    // 包装器但后面没有 app-server 子命令 → 不算
    assert!(!looks_like_wrapped_codex_server(
        " 203 node /opt/homebrew/bin/codex"
    ));
    assert!(!looks_like_wrapped_codex_server(
        " 204 /bin/bash -c \"vim notes.md\""
    ));
    // 首参不是常见包装器 → 不算（`awk /codex/ && /app-server/` 这类噪声）
    assert!(!looks_like_wrapped_codex_server(
        " 205 awk /codex/ && /app-server/ {print}"
    ));
}

/// 被禁用时一个进程都不碰（环境变量在 edition 2024 里不能安全地改，所以走这个入口测）。
#[cfg(unix)]
#[test]
fn disabled_restart_touches_nothing() {
    let report = restart_daemon_with(true).unwrap();
    assert_eq!(report, RestartReport::default());
}
