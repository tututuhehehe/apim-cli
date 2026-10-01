mod app;
mod browser;
mod cli;
mod clients;
mod clipboard;
mod config;
mod form;
mod probe;
mod recipe;
mod tui;
mod ui;
mod util;

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
            "--snapshot-import" => tui::run_snapshot_import().await,
            "--snapshot-import-models" => tui::run_snapshot_import_models().await,
            "--snapshot-import-default" => tui::run_snapshot_import_default().await,
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
        // 版本：给安装脚本 / 包管理器 / 自更新检测用
        Some("--version") | Some("-V") | Some("version") => {
            cli::print_version();
            Ok(())
        }
        Some(_) => cli::run(&args).await,
    }
}
