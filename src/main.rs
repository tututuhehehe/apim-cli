mod app;
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
    let flag = std::env::args().find(|a| a.starts_with("--snapshot"));
    match flag.as_deref() {
        Some("--snapshot-form") => tui::run_snapshot_key_form().await,
        Some("--snapshot-provider-form") => tui::run_snapshot_provider_form().await,
        Some("--snapshot") => tui::run_snapshot().await,
        _ => tui::run_tui().await,
    }
}
