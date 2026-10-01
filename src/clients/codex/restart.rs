//! 导入成功后重启 codex 的 app-server 守护进程。
//!
//! codex 的模型目录（`model_catalog_json`）只在 app-server 启动那一刻读一次，之后一直缓存 ——
//! 实测：导入后新开的 codex 会话仍然列出旧目录（`codex exec` 却能直接用新模型），因为
//! TUI/桌面端都挂在同一个常驻 daemon 上。cc-switch 遇到同一件事，也只是提示用户重启 Codex
//! （v3.16.1 release notes）；apim 做得更直接：把在跑的 `codex app-server` 杀掉，
//! codex 下次启动会自动起新的（`APIM_NO_RESTART_CODEX=1` 可关）。

/// 一次自动重启的结果。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RestartReport {
    /// 成功杀掉的守护进程数。
    pub killed: usize,
    /// 命令行里同时出现 codex 与 app-server、但**不是标准形态**的进程数
    /// （例如 `sh -c "codex app-server"`）。不做自动重启，只在提示里让用户手动重启 ——
    /// 放宽匹配去杀它会把用户自己的会话一起带走。
    pub ambiguous: usize,
}

/// 杀掉在跑的 `codex app-server`（见模块注释：不重启则新模型不会出现在 codex 里）。
/// 返回成功杀掉的进程数；0 = 当时没有 daemon 在跑。
pub fn restart_daemon() -> Result<RestartReport, String> {
    restart_daemon_with(std::env::var_os("APIM_NO_RESTART_CODEX").is_some())
}

/// 同上，但「是否禁用」由调用方给（环境变量在 edition 2024 里不能安全地改，测试走这个入口）。
pub(crate) fn restart_daemon_with(disabled: bool) -> Result<RestartReport, String> {
    if disabled {
        return Ok(RestartReport::default());
    }
    let (pids, ambiguous) = scan_codex_servers()?;
    // 只扫描一次、只杀「这次调用开始前就在跑」的那批：
    // 重扫再杀会连带杀掉刚被拉起、已经加载了新配置的 daemon，
    // 甚至杀掉用户此刻新开的会话 —— 既无必要又有害。
    let mut killed = 0;
    for pid in pids {
        let ok = std::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        // 只统计真的杀成功的，别把 EPERM 也算进「已重启 N 个」
        if ok {
            killed += 1;
        }
    }
    Ok(RestartReport { killed, ambiguous })
}

/// 扫一遍进程表：返回（可安全杀掉的守护进程 pid，像 daemon 但形态不标准的进程数）。
///
/// 严格匹配只认「可执行文件叫 codex + 子命令位是 `app-server`」，不碰用户自己的 codex 会话；
/// 不标准的形态（`sh -c "codex app-server"` 之类）只计数、不杀 —— 误杀用户的会话比不帮你重启更糟。
#[cfg(unix)]
fn scan_codex_servers() -> Result<(Vec<u32>, usize), String> {
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid=,command="])
        .output()
        .map_err(|err| format!("执行 ps 失败：{err}"))?;
    let me = std::process::id();
    let mut pids = Vec::new();
    let mut ambiguous = 0;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(pid) = line
            .split_whitespace()
            .next()
            .and_then(|token| token.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me {
            continue;
        }
        if is_codex_server(line) {
            pids.push(pid);
        } else if looks_like_wrapped_codex_server(line) {
            ambiguous += 1;
        }
    }
    Ok((pids, ambiguous))
}

#[cfg(not(unix))]
fn scan_codex_servers() -> Result<(Vec<u32>, usize), String> {
    // 不去动 Windows 上的用户进程；导入后会把「需重启 codex」写进提示
    Ok((Vec::new(), 0))
}

/// 一行 `ps -eo pid=,command=` 输出是不是 codex 的 app-server（守护）进程。
/// 可执行文件名必须正好是 `codex`，且**第一个参数**必须是 `app-server` 子命令。
#[cfg(unix)]
pub(crate) fn is_codex_server(line: &str) -> bool {
    let mut tokens = line.split_whitespace();
    let _pid = tokens.next();
    let Some(binary) = tokens.next() else {
        return false;
    };
    let name = binary.rsplit('/').next().unwrap_or(binary);
    if name != "codex" && name != "codex.exe" {
        return false;
    }
    // 只认子命令位（argv[2]）：`codex app-server [--listen … | daemon pid-update-loop]`。
    // 不能扫「任意 token == app-server」—— 那会把 `codex --profile app-server`、
    // `codex exec "app-server"` 这类用户自己的会话也算进来，杀掉就是误杀。
    matches!(tokens.next(), Some("app-server"))
}

/// 「像 codex 的 daemon，但命令行是包装形态」——例如 `sh -c "codex app-server"`、
/// `env -i /path/codex app-server`。**只用于提示，绝不据此杀进程。**
#[cfg(unix)]
pub(crate) fn looks_like_wrapped_codex_server(line: &str) -> bool {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    // tokens[0] 是 pid；标准形态（tokens[1] 就是 codex）由 is_codex_server 负责
    let Some(wrapper) = tokens.get(1) else {
        return false;
    };
    let name = wrapper.rsplit('/').next().unwrap_or(wrapper);
    if !["sh", "bash", "zsh", "dash", "env", "node"].contains(&name) {
        return false;
    }
    // 引号在 ps 里是字面字符：比较前先剥掉，别让 `"codex` / `app-server"` 漏掉
    let strip = |token: &str| token.trim_matches(['"', '\'']).to_string();
    let mut seen_codex = false;
    for token in &tokens[2..] {
        let token = strip(token);
        let base = token.rsplit('/').next().unwrap_or(&token).to_string();
        if base == "codex" {
            seen_codex = true;
        } else if seen_codex && token == "app-server" {
            return true;
        }
    }
    false
}
