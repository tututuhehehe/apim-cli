//! `apim uninstall`：按安装渠道把 apim 从机器上摘掉（与 `apim update` 共用同一套渠道识别）。
//!
//! | 渠道 | 怎么卸 |
//! |---|---|
//! | npm | `npm uninstall -g apim-cli` |
//! | Homebrew | `brew uninstall apim` |
//! | install.sh | 删掉那个裸二进制（是软链就把链一起删，免得留下断链） |
//!
//! 默认**只删程序、留数据**；`--purge` 才连 `~/.config/apim`（config.toml / secrets.toml /
//! recipes / scripts，里面有密钥）一起删。`--yes` 跳过确认（给脚本/非交互用），
//! `--dry-run` 只报告会做什么。
//!
//! 三件故意不做的事：
//! - `target/` 下的开发构建与 `~/.cargo/bin` 里的 cargo 副本一律不删（跟 update 一致）；
//! - 不碰 `~/.codex/`：一键导入写进去的 provider 块和用户手写的配置混在同一个文件里，
//!   只能提示不能代删（约定 11：只切换激活项，旧内容一律保留）；
//! - 不碰 PATH 与 shell rc（install.sh 只打印 PATH 提示、从不写 rc）。
//!
//! 拆成三块：本文件是流程（认渠道 → 校验 → 确认 → 动手 → 报告），怎么删在 `cleanup`，
//! 输出在 `report`。

mod cleanup;
mod report;
#[cfg(test)]
mod tests;

use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Args;
use super::update::{Channel, RunError, detect_channel, run_command};
use report::Report;

/// `apim uninstall [--yes] [--purge] [--dry-run] [--json]`
pub(crate) fn run(args: &Args) -> Result<()> {
    let json = args.has("json");
    let purge = args.has("purge");
    let dry_run = args.has("dry-run");
    let assume_yes = args.has("yes");

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("apim"));
    let config_dir = crate::config::config_dir();

    // 不属于三条发布渠道：开发构建 / cargo 副本，跟 update 一样不碰
    let Some(channel) = detect_channel(&exe) else {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "channel": serde_json::Value::Null,
                    "exe": exe.display().to_string(),
                    "removed": [],
                    "purged": serde_json::Value::Null,
                })
            );
        } else {
            println!(
                "当前：apim {}（本地开发构建 / cargo 副本，不在发布渠道里）",
                env!("CARGO_PKG_VERSION")
            );
            println!("  {}", exe.display());
            println!("\napim uninstall 只卸三条发布渠道（npm / Homebrew / install.sh）装的 apim；");
            println!(
                "`cargo run` / `cargo build` 的产物和 ~/.cargo/bin 里的副本不删（与 apim update 一致）。"
            );
            if purge {
                println!(
                    "\n--purge 只对发布渠道生效；要清配置目录：rm -rf {}",
                    config_dir.display()
                );
            }
        }
        return Ok(());
    };

    // 先校验再动手：--purge 目标不合法就别先卸掉程序（免得留下半截状态）
    if purge {
        cleanup::check_purge_target(&config_dir)?;
    }

    // 动作之前算：删完之后 canonicalize 就解析不出来了（这是「实际删了哪些」的依据）
    let targets = cleanup::binary_targets(&exe);
    let mut report = Report {
        channel,
        exe: &exe,
        command: uninstall_command(channel, &exe),
        config_dir: &config_dir,
        purge,
        dry_run,
        removed: Vec::new(),
        purged: None,
        leftovers: cleanup::codex_leftovers(),
    };

    if !json {
        report.print_plan();
    }
    if dry_run {
        if json {
            report.print_json();
        } else {
            println!("\n--dry-run：什么都没删。");
        }
        return Ok(());
    }

    if !assume_yes {
        if json {
            bail!("--json 是非交互模式，卸载请加 --yes");
        }
        if !confirm("确认卸载？")? {
            println!("\n已取消，什么都没删。");
            return Ok(());
        }
    }

    // 1) 程序本体：按渠道走（npm / brew 由工具自己删，裸二进制 apim 直接删）
    uninstall_program(channel, &exe)?;
    // 2) 数据：只有 --purge 才动，且必须在程序卸掉之后（失败就停在上一步，不误删密钥）
    if purge {
        report.purged = cleanup::purge_config_dir(&config_dir)?;
    }

    // 实际删掉了哪些路径：动作之后再看文件在不在，比「假定删成功」诚实
    for path in &targets {
        let text = path.display().to_string();
        if fs::symlink_metadata(path).is_err() && !report.removed.contains(&text) {
            report.removed.push(text);
        }
    }

    if json {
        report.print_json();
    } else {
        report.print_result();
    }
    Ok(())
}

/// 这条渠道的卸载命令（给用户看的原样命令，也是 `--dry-run` 的结论）。
fn uninstall_command(channel: Channel, exe: &Path) -> String {
    match channel {
        Channel::Npm => "npm uninstall -g apim-cli".to_string(),
        Channel::Homebrew => "brew uninstall apim".to_string(),
        Channel::Binary => format!("rm {}", exe.display()),
    }
}

/// 按渠道卸掉程序本体。
fn uninstall_program(channel: Channel, exe: &Path) -> Result<()> {
    match channel {
        Channel::Npm => run_npm_uninstall(),
        Channel::Homebrew => run_tool("brew", &["uninstall", "apim"]),
        Channel::Binary => cleanup::remove_binary(exe),
    }
}

/// npm 渠道：`npm uninstall -g apim-cli` 会连 .bin 里的 shim 和 node_modules 里的
/// 平台子包一起删（npm 自己知道删哪些文件，不用 apim 猜路径）。
#[cfg(not(windows))]
fn run_npm_uninstall() -> Result<()> {
    run_tool("npm", &["uninstall", "-g", "apim-cli"])
}

/// Windows 上 apim.exe 被本进程占着，npm 删它多半会 EBUSY —— 失败时补一句「换个 shell 重跑」。
#[cfg(windows)]
fn run_npm_uninstall() -> Result<()> {
    run_tool("npm", &["uninstall", "-g", "apim-cli"]).map_err(|err| {
        anyhow::anyhow!(
            "{err}\n  Windows 上正在运行的 apim.exe 删不掉自己：关掉本进程，再到另一个 shell 里重跑这条命令。"
        )
    })
}

/// 跑一个外部工具（npm / brew），失败时给出手动命令。
fn run_tool(tool: &str, args: &[&str]) -> Result<()> {
    match run_command(tool, args, None, &[]) {
        Ok(_) => Ok(()),
        Err(RunError::Exited(code)) => bail!(
            "{tool} 卸载失败（退出码 {}）；手动执行：{} {}",
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

/// 卸载前的确认。stdin 不是终端（管道 / CI）时直接报错让脚本加 `--yes`，别默默走默认值。
fn confirm(prompt: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("stdin 不是终端，无法确认；非交互场景请加 --yes");
    }
    print!("{prompt} [y/N] ");
    std::io::stdout().flush().context("flush stdout")?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("read stdin")?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
