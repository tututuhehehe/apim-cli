//! 用系统默认浏览器打开 URL（厂商控制面板主页）。

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use anyhow::Context;
use anyhow::{Result, bail};

/// scheme 校验：只放行 http(s)（大小写不敏感，RFC 3986），
/// 防 open 被拿去执行别的 scheme 或当 flag 注入（URL 必以字母开头）。
/// 独立成纯函数以便单测覆盖，不触发真实打开。
pub(crate) fn validate(url: &str) -> Result<()> {
    let scheme_ok = url
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
        || url
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("http://"));
    if !scheme_ok {
        bail!("主页 URL 必须以 http(s):// 开头：{url}");
    }
    Ok(())
}

pub fn open(url: &str) -> Result<()> {
    // recipe 可被手改，双保险：open 前再校验一次
    validate(url)?;
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
    fn validate_rejects_non_http_scheme() {
        // 只测纯校验函数，绝不触发真实的 open（cargo test 不应弹浏览器）
        assert!(validate("ftp://x.io").is_err());
        assert!(validate("file:///etc").is_err());
        assert!(validate("https:/single-slash").is_err());
        assert!(validate("https://x.io").is_ok());
        assert!(validate("HTTPS://X.io").is_ok()); // scheme 大小写不敏感（RFC 3986）
    }

    #[test]
    fn open_rejects_before_spawning() {
        // 拒绝路径在校验处即返回，不会 spawn open 进程
        assert!(open("ftp://x.io").is_err());
    }
}
