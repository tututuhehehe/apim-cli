//! `apim update`：先认出「当前这个 apim 是从哪条渠道装的」，再走同一条渠道更新。
//!
//! 只有三条发布渠道（`docs/RELEASING.md` 的「各渠道速查」）：
//!
//! | 渠道 | 二进制长在哪 | 更新方式 |
//! |---|---|---|
//! | npm | `node_modules/apim-cli-<平台>/bin/apim` | `npm install -g apim-cli@latest` |
//! | Homebrew | `.../Cellar/apim/<版本>/bin/apim` | `brew upgrade apim` |
//! | install.sh | `/usr/local/bin` 或 `~/.local/bin` 的裸二进制 | 下载**该 tag 的**官方 install.sh、校验 sha256 后执行 |
//!
//! 本文件是入口与 npm / Homebrew 两条渠道；认渠道在 `channel`，install.sh 渠道在
//! `install_sh`（Windows 上没有这条渠道），联网取文本在 `http`。
//!
//! `target/` 下的开发构建与 `~/.cargo/bin` 里的副本都不算渠道（`detect_channel` 返回 None），
//! 一律不碰 —— 前者会被 Release 覆盖掉开发二进制，后者是 `cargo install` 留下的多余副本。

mod channel;
mod http;
#[cfg(not(windows))]
mod install_sh;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{Result, bail};

// `Channel` / `detect_channel` 也被 `apim uninstall` 复用（卸载同样只认这三条渠道），
// `run_command` / `RunError` 是两处共用的「跑外部命令」小工具。
pub(crate) use channel::{Channel, detect_channel};
use http::latest_tag;

/// 仓库与安装脚本：与 Cargo.toml 的 `repository` / `install.sh` 保持一致。
const REPO: &str = "tututuhehehe/apim-cli";

/// install.sh 的直链，`reference` 是 tag 或分支名。两条用法都从这里拼，别抄第二份：
/// 真正执行时钉 tag（内容可复现），提示里给用户手动跑的那条用 `main`。
fn install_sh_url(reference: &str) -> String {
    format!("https://raw.githubusercontent.com/{REPO}/{reference}/install.sh")
}

/// `apim update [--check] [--force] [--json]`。
pub(crate) async fn run(args: &super::Args) -> Result<()> {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("apim"));
    // None = 本地开发构建 / cargo 副本，不属于任何发布渠道
    let channel = detect_channel(&exe);
    let current = env!("CARGO_PKG_VERSION");
    let json = args.has("json");
    let check_only = args.has("check");
    let force = args.has("force");

    let latest_tag = match latest_tag().await {
        Ok(tag) => Some(tag),
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
            None => println!("当前：apim {current}（本地开发构建 / cargo 副本，不在发布渠道里）"),
        }
        println!("  {}", exe.display());
        match &latest_tag {
            Some(tag) => println!("最新：{tag}"),
            None => println!("最新：查不到（网络问题？）"),
        }
        if channel == Some(Channel::Npm)
            && let Some(published) = npm_published_version()
        {
            println!("npm 上可装：apim-cli {published}");
        }
    }

    if check_only {
        if !json && up_to_date {
            println!("已是最新。");
        }
        return Ok(());
    }

    // 不属于三条发布渠道：不覆盖
    let Some(channel) = channel else {
        if !json {
            println!(
                "\n这是 `cargo run` / `cargo build` 的产物，或是 cargo install 留下来的副本，apim 不覆盖它。"
            );
            println!(
                "要换成发布版：装 install.sh 版 / npm 版 / Homebrew 版（见 README 安装一节）。"
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

    run_channel_update(channel, &exe, latest_tag.as_deref(), json, force).await
}

/// 按渠道执行更新。
async fn run_channel_update(
    channel: Channel,
    exe: &Path,
    latest_tag: Option<&str>,
    json: bool,
    force: bool,
) -> Result<()> {
    match channel {
        Channel::Npm => {
            if !json {
                println!("\n→ npm 渠道：npm install -g apim-cli@latest");
            }
            // GitHub 的 Release 与 npm 是两条腿：Release 刚转正时 npm 可能还没跟上，
            // 这时 `npm install -g apim-cli@latest` 只会把**旧版本重装一遍**，
            // 用户看到「更新完成」会以为更新了（v0.1.4 真机踩过）。所以先问 npm 现在能装到哪个版本。
            let target = latest_tag.map(|tag| tag.trim_start_matches('v'));
            if let (Some(target), Some(published)) = (target, npm_published_version())
                && published != target
                && !force
            {
                bail!(npm_lag_message(target, &published));
            }
            run_tool("npm", &["install", "-g", "apim-cli@latest"], "更新")?;
        }
        Channel::Homebrew => {
            if !json {
                println!("\n→ Homebrew 渠道：brew upgrade apim");
            }
            run_tool("brew", &["upgrade", "apim"], "更新")?;
        }
        Channel::Binary => {
            #[cfg(windows)]
            {
                // 先判平台再打印：否则会先输出「下载 install.sh」再报「Windows 没有这条渠道」
                bail!(
                    "Windows 没有 install.sh 渠道；请从 Release 下载 .zip 手动替换，或改用 npm：\n  \
                     npm install -g apim-cli\n  https://github.com/{REPO}/releases"
                );
            }
            #[cfg(not(windows))]
            {
                install_sh::update_via_install_sh(exe, latest_tag, json).await?;
            }
        }
    }

    if !json {
        println!("\n更新完成。重开一个 shell 后确认：apim --version");
    }
    Ok(())
}

/// 跑一个外部工具（npm / brew），失败时给出手动命令。`action` 是「更新」/「卸载」这类动词 ——
/// `apim uninstall` 复用这一个实现（两种场景失败后的做法完全一样：报退出码 + 给一条手动命令）。
pub(crate) fn run_tool(tool: &str, args: &[&str], action: &str) -> Result<()> {
    match run_command(tool, args, None, &[]) {
        Ok(_) => Ok(()),
        Err(RunError::Exited(code)) => bail!(
            "{tool} {action}失败（退出码 {}）；手动执行：{} {}",
            code.unwrap_or(-1),
            tool,
            args.join(" ")
        ),
        Err(RunError::Spawn(err)) => {
            bail!(
                "跑不了 {tool}（{err}）；手动执行：{} {}",
                tool,
                args.join(" ")
            )
        }
    }
}

/// npm 上 `apim-cli@latest` 现在解析到的版本（查不到 = `None`：离线 / 私有 registry / 没装 npm）。
///
/// 不打印 npm 自己的输出：这里只用来判断「npm 跟上 GitHub 了没有」。
fn npm_published_version() -> Option<String> {
    let output = Command::new("npm")
        .args(["view", "apim-cli", "version"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// npm 还没跟上 GitHub 时的提示（`published` 是 npm 现在能装到的版本）。
fn npm_lag_message(target: &str, published: &str) -> String {
    format!(
        "npm 上还没有 apim-cli {target}（现在能装到的是 {published}）。\n\
         Release 刚发布时 npm 可能还在发：等几分钟再 `apim update`，或去 Actions 看 \"Publish to npm\" 那次运行。\n\
         （没有这一步的话，`npm install -g apim-cli@latest` 只会把旧版本重装一遍，看起来像更新成功了。）"
    )
}

/// 外部命令的两种失败：程序根本没启动起来（没装 / 不在 PATH 里），与启动了但退出码非 0。
/// 分开返回，让三处调用（npm/brew、install.sh 的 `sh`、校验摘要的 `shasum`）各自给出对用户有用的做法。
pub(crate) enum RunError {
    /// 跑起来了，但退出码非 0（被信号杀掉时没有退出码）。
    Exited(Option<i32>),
    /// 连启动都没成功。
    Spawn(std::io::Error),
}

/// 跑一个外部命令：只管启动、取退出码与分辨上面两种失败。`cwd` / `envs` 允许为空
/// （校验摘要在工作目录里跑，install.sh 要注入 `APIM_INSTALL_DIR`）。
pub(crate) fn run_command<A>(
    program: &str,
    args: A,
    cwd: Option<&Path>,
    envs: &[(&str, &OsStr)],
) -> std::result::Result<ExitStatus, RunError>
where
    A: IntoIterator,
    A::Item: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    match command.status() {
        Ok(status) if status.success() => Ok(status),
        Ok(status) => Err(RunError::Exited(status.code())),
        Err(err) => Err(RunError::Spawn(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// npm 落后于 GitHub Release 时的提示要同时给出两个版本号与两条出路（等一会儿 / 去 Actions）。
    #[test]
    fn npm_lag_message_says_both_versions_and_the_way_out() {
        let message = npm_lag_message("0.1.4", "0.1.3");
        assert!(message.contains("0.1.4"), "{message}");
        assert!(message.contains("0.1.3"), "{message}");
        assert!(message.contains("Publish to npm"), "{message}");
        assert!(message.contains("apim update"), "{message}");
    }
}
