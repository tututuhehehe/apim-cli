mod app;
mod browser;
mod cli;
mod clipboard;
mod config;
mod form;
mod probe;
mod recipe;
mod tui;
mod ui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // --snapshot* 是给测试用的渲染快照，保持原样
        Some(flag) if flag.starts_with("--snapshot") => match flag {
            "--snapshot-form" => tui::run_snapshot_key_form().await,
            "--snapshot-provider-form" => tui::run_snapshot_provider_form().await,
            "--snapshot-inspector" => tui::run_snapshot_inspector().await,
            "--snapshot" => tui::run_snapshot().await,
            other => {
                eprintln!("未知参数 {other}");
                Ok(())
            }
        },
        // 无参数 = 进 TUI；help 打印用法；其余交给 CLI 子命令
        None | Some("tui") => tui::run_tui().await,
        Some("help") | Some("--help") | Some("-h") => {
            cli::print_help();
            Ok(())
        }
        Some(_) => cli::run(&args).await,
    }
}
