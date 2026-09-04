use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

pub fn copy(text: &str) -> Result<()> {
    match copy_arboard(text) {
        Ok(()) => Ok(()),
        Err(_) => copy_pbcopy(text),
    }
}

fn copy_arboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_string())?;
    Ok(())
}

fn copy_pbcopy(text: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .context("spawn pbcopy")?;
    child
        .stdin
        .as_mut()
        .context("pbcopy stdin")?
        .write_all(text.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("pbcopy failed");
    }
    Ok(())
}
