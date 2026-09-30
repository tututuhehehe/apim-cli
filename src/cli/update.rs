//! `apim update`：先认出「当前这个 apim 是从哪条渠道装的」，再走同一条渠道更新。
//!
//! 只有三条发布渠道（`docs/RELEASING.md` 的「各渠道速查」）：
//!
//! | 渠道 | 二进制长在哪 | 更新方式 |
//! |---|---|---|
//! | npm | `node_modules/apim-cli-<平台>/bin/apim` | `npm install -g apim-cli@latest` |
//! | Homebrew | `.../Cellar/apim/<版本>/bin/apim` | `brew upgrade apim` |
//! | install.sh | `/usr/local/bin` 或 `~/.local/bin` 的裸二进制 | 重跑官方 `install.sh`（钉住同一个安装目录） |
//!
//! install.sh 那条不自己下载/解包：直接复用官方脚本（下载 + sha256 校验 + 原子替换都是它
//! 验证过的逻辑），我们只负责认出渠道，并把 `APIM_INSTALL_DIR` 钉在当前二进制所在目录，
//! 保证「原地更新」而不是装到别处留两份。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};

/// 仓库与安装脚本：与 Cargo.toml 的 `repository` / `install.sh` 保持一致。
const REPO: &str = "tututuhehehe/apim-cli";
const INSTALL_SH_URL: &str =
    "https://raw.githubusercontent.com/tututuhehehe/apim-cli/main/install.sh";

/// 当前 apim 的安装渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channel {
    Npm,
    Homebrew,
    /// install.sh 装的裸二进制（`/usr/local/bin/apim`、`~/.local/bin/apim` 等）。
    Binary,
}

impl Channel {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Channel::Npm => "npm",
            Channel::Homebrew => "Homebrew",
            Channel::Binary => "install.sh（裸二进制）",
        }
    }

    /// 这条渠道的更新命令（给用户看的原样命令）。
    fn command_hint(self) -> &'static str {
        match self {
            Channel::Npm => "npm install -g apim-cli@latest",
            Channel::Homebrew => "brew upgrade apim",
            Channel::Binary => {
                "curl -fsSL https://raw.githubusercontent.com/tututuhehehe/apim-cli/main/install.sh | sh"
            }
        }
    }
}

/// 认渠道。`None` = 不是这三条发布渠道装的（典型情况：`target/debug` 下的本地开发构建），
/// 这种一律不动 —— 免得把开发用的二进制覆盖成 Release 版。
///
/// 同时看原路径与 `canonicalize()` 之后的路径：Homebrew 会经过 `/opt/homebrew/bin/apim`
/// 这样的软链，只看原路径认不出来。
pub(crate) fn detect_channel(exe: &Path) -> Option<Channel> {
    let canonical = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let raw = exe.to_string_lossy().replace('\\', "/");
    let real = canonical.to_string_lossy().replace('\\', "/");
    let has = |needle: &str| raw.contains(needle) || real.contains(needle);

    // 本地开发构建（`cargo run` / `cargo build` 的产物）：不属于任何发布渠道
    if has("/target/debug/") || has("/target/release/") {
        return None;
    }
    // npm 全局包：二进制在 node_modules/apim-cli-<平台>/bin/ 下
    if has("node_modules") && has("apim-cli") {
        return Some(Channel::Npm);
    }
    // Homebrew（含 Linuxbrew）：brew 把真身放在 Cellar/apim/<版本>/
    if has("Cellar/apim") {
        return Some(Channel::Homebrew);
    }
    Some(Channel::Binary)
}

/// `apim update [--check] [--force] [--json]`。
pub(crate) async fn run(args: &super::Args) -> Result<()> {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("apim"));
    // None = 本地开发构建，不属于任何发布渠道
    let channel = detect_channel(&exe);
    let current = env!("CARGO_PKG_VERSION");
    let json = args.has("json");
    let check_only = args.has("check");
    let force = args.has("force");

    let latest = latest_tag().await;
    let latest_tag = match &latest {
        Ok(tag) => Some(tag.clone()),
        Err(err) => {
            if !json {
                eprintln!("apim update: 查最新版本失败：{err}");
            }
            None
        }
    };
    let up_to_date = latest_tag
        .as_deref()
        .is_some_and(|tag| tag.trim_start_matches('v') == current);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "current": current,
                "latest": latest_tag.as_deref().map(|tag| tag.trim_start_matches('v')),
                "channel": channel.map(Channel::label),
                "exe": exe.display().to_string(),
                "up_to_date": up_to_date,
                "command": channel.map(Channel::command_hint),
            })
        );
    } else {
        match channel {
            Some(channel) => println!("当前：apim {current}（渠道：{}）", channel.label()),
            None => println!("当前：apim {current}（本地开发构建，不在发布渠道里）"),
        }
        println!("  {}", exe.display());
        match &latest_tag {
            Some(tag) => println!("最新：{tag}"),
            None => println!("最新：查不到（网络问题？）"),
        }
    }

    if check_only {
        if !json && up_to_date {
            println!("已是最新。");
        }
        return Ok(());
    }

    // 本地开发构建（`cargo run` / `cargo build` 的产物）：不覆盖它
    let Some(channel) = channel else {
        if !json {
            println!("\n这是 `cargo run` / `cargo build` 的产物，apim 不覆盖它。");
            println!(
                "要换成发布版：装 install.sh 版 / npm 版 / Homebrew 版（见 README 的安装一节）。"
            );
        }
        return Ok(());
    };

    // 已经是最新且没 --force：不再重装一遍（没意义，还多一次权限风险）
    if up_to_date && !force {
        if !json {
            println!("已是最新，无需更新。想强制重装加 --force。");
        }
        return Ok(());
    }

    run_channel_update(channel, &exe, json)
}

/// 按渠道执行更新。
fn run_channel_update(channel: Channel, exe: &Path, json: bool) -> Result<()> {
    match channel {
        Channel::Npm => {
            if !json {
                println!("\n→ npm 渠道：npm install -g apim-cli@latest");
            }
            let status = Command::new("npm")
                .args(["install", "-g", "apim-cli@latest"])
                .status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => bail!("npm 更新失败（退出码 {}）", s.code().unwrap_or(-1)),
                Err(err) => bail!("跑不了 npm（{err}）；手动执行：npm install -g apim-cli@latest"),
            }
        }
        Channel::Homebrew => {
            if !json {
                println!("\n→ Homebrew 渠道：brew upgrade apim");
            }
            let status = Command::new("brew").args(["upgrade", "apim"]).status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => bail!("brew 更新失败（退出码 {}）", s.code().unwrap_or(-1)),
                Err(err) => bail!("跑不了 brew（{err}）；手动执行：brew upgrade apim"),
            }
        }
        Channel::Binary => {
            // 复用官方安装脚本，并把 APIM_INSTALL_DIR 钉在当前二进制所在目录
            let dir = exe
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            if !json {
                println!(
                    "\n→ 裸二进制渠道：重跑官方 install.sh（装到 {}）",
                    dir.display()
                );
                println!("  {INSTALL_SH_URL}");
            }
            #[cfg(windows)]
            {
                bail!(
                    "Windows 没有 install.sh 渠道；请从 Release 下载 .zip 手动替换，或改用 npm：\n  \
                     npm install -g apim-cli\n  https://github.com/{REPO}/releases"
                );
            }
            #[cfg(not(windows))]
            {
                let status = Command::new("sh")
                    .arg("-c")
                    .arg(format!("curl -fsSL '{INSTALL_SH_URL}' | sh"))
                    .env("APIM_INSTALL_DIR", &dir)
                    .status();
                match status {
                    Ok(s) if s.success() => {}
                    Ok(s) => bail!(
                        "install.sh 更新失败（退出码 {}）；目录不可写时用：\n  \
                         sudo APIM_INSTALL_DIR={} sh -c \"curl -fsSL '{INSTALL_SH_URL}' | sh\"",
                        s.code().unwrap_or(-1),
                        dir.display()
                    ),
                    Err(err) => bail!(
                        "跑不了 sh/curl（{err}）；手动执行：{}",
                        Channel::Binary.command_hint()
                    ),
                }
            }
        }
    }

    if !json {
        println!("\n更新完成。重开一个 shell 后确认：apim --version");
    }
    Ok(())
}

/// 取最新 Release 的 tag：跟随 `releases/latest` 的 302，从最终 URL 里取 tag。
/// 走重定向而不是 GitHub API，这样没有匿名 API 的 60 次/小时限流。
async fn latest_tag() -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("apim/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let response = client
        .get(format!("https://github.com/{REPO}/releases/latest"))
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("HTTP {}", response.status().as_u16());
    }
    // 不读 body（HTML 很大），只要重定向后的 URL
    let tag = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .map(str::to_string)
        .unwrap_or_default();
    if tag.is_empty() || tag == "latest" {
        bail!("GitHub 没给出 tag（可能还没有 Release）");
    }
    Ok(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_npm_global_install() {
        assert_eq!(
            detect_channel(Path::new(
                "/opt/homebrew/lib/node_modules/apim-cli/node_modules/apim-cli-darwin-arm64/bin/apim"
            )),
            Some(Channel::Npm)
        );
        assert_eq!(
            detect_channel(Path::new(
                "C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\apim-cli-windows-x64\\bin\\apim.exe"
            )),
            Some(Channel::Npm)
        );
    }

    #[test]
    fn detects_homebrew() {
        assert_eq!(
            detect_channel(Path::new("/opt/homebrew/Cellar/apim/0.1.9/bin/apim")),
            Some(Channel::Homebrew)
        );
        // Linuxbrew
        assert_eq!(
            detect_channel(Path::new("/home/me/.linuxbrew/Cellar/apim/0.1.9/bin/apim")),
            Some(Channel::Homebrew)
        );
    }

    #[test]
    fn detects_install_sh_binary() {
        // install.sh 的默认落点：/usr/local/bin（可写）或 ~/.local/bin
        assert_eq!(
            detect_channel(Path::new("/usr/local/bin/apim")),
            Some(Channel::Binary)
        );
        assert_eq!(
            detect_channel(Path::new("/Users/me/.local/bin/apim")),
            Some(Channel::Binary)
        );
    }

    /// 本地开发构建不属于任何发布渠道 —— 绝不能拿 Release 覆盖掉它。
    #[test]
    fn dev_builds_are_not_a_channel() {
        assert_eq!(
            detect_channel(Path::new("/Users/me/apim-cli/target/debug/apim")),
            None
        );
        assert_eq!(
            detect_channel(Path::new(
                "/Users/me/Documents/VIBE/apim管理/target/release/apim"
            )),
            None
        );
    }

    /// 源码目录名里带 apim-cli 不能误判成 npm（必须同时有 node_modules）。
    #[test]
    fn repo_path_alone_is_not_npm() {
        assert_eq!(
            detect_channel(Path::new("/usr/local/bin/apim")),
            Some(Channel::Binary)
        );
    }
}
