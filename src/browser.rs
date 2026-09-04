//! 用系统默认浏览器打开 URL（厂商控制面板主页）。

use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn open(url: &str) -> Result<()> {
    // recipe 可被手改，双保险：只放行 http(s)，防 open 被拿去执行别的 scheme。
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        bail!("主页 URL 必须以 http(s):// 开头：{url}");
    }
    #[cfg(unix)]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(program)
            .arg(url)
            .spawn()
            .with_context(|| format!("spawn {program}"))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        bail!("仅支持 macOS/Linux");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_rejects_non_http_scheme() {
        assert!(open("ftp://x.io").is_err());
        assert!(open("file:///etc").is_err());
        assert!(open("https://x.io").is_ok()); // 只校验 scheme，不真开浏览器
    }
}
