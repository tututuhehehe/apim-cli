//! 联网取文本：最新 Release 的 tag，以及 install.sh / 摘要这类纯文本资源。
//!
//! 取最新 tag 走 `releases/latest` 的 302 重定向而不是 GitHub API，这样没有匿名 API 的
//! 60 次/小时限流。

use anyhow::{Context, Result, anyhow, bail};

use super::REPO;

/// 单个响应体的上限：脚本/摘要都是几 KB，超过就是被塞了别的东西。
const MAX_BODY_BYTES: usize = 64 * 1024;

/// GET 一个文本资源，带长度上限。
pub(super) async fn http_text(url: &str) -> Result<String> {
    let response = http_client()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("请求 {url} 失败"))?;
    if !response.status().is_success() {
        bail!("{url} 返回 HTTP {}", response.status().as_u16());
    }
    let body = response
        .text()
        .await
        .with_context(|| format!("读取 {url} 失败"))?;
    if body.len() > MAX_BODY_BYTES {
        bail!("{url} 的响应过大（{} 字节），拒绝处理", body.len());
    }
    Ok(body)
}

fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("apim/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// 取最新 Release 的 tag：跟随 `releases/latest` 的 302，从最终 URL 里取 tag。
pub(super) async fn latest_tag() -> Result<String> {
    let response = http_client()?
        .get(format!("https://github.com/{REPO}/releases/latest"))
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("HTTP {}", response.status().as_u16());
    }
    // 不读 body（HTML 很大），只要重定向后的 URL
    tag_from_url(response.url().as_str())
        .ok_or_else(|| anyhow!("GitHub 没给出 tag（可能还没有 Release）"))
}

/// 从 `…/releases/tag/vX.Y.Z` 这类 URL 里取 tag。
/// 取不到、或最终还停在 `…/releases/latest`（说明没有 Release）时返回 None。
pub(super) fn tag_from_url(url: &str) -> Option<String> {
    let tail = url.trim_end_matches('/').rsplit('/').next()?;
    if tail.is_empty() || tail == "latest" {
        return None;
    }
    Some(tail.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_from_release_url() {
        assert_eq!(
            tag_from_url("https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.1"),
            Some("v0.1.1".to_string())
        );
        assert_eq!(
            tag_from_url("https://github.com/tututuhehehe/apim-cli/releases/tag/v0.1.1/"),
            Some("v0.1.1".to_string())
        );
        // 没有 Release 时重定向会停在 …/releases/latest
        assert_eq!(
            tag_from_url("https://github.com/tututuhehehe/apim-cli/releases/latest"),
            None
        );
        assert_eq!(tag_from_url(""), None);
    }

    /// 联网检查 `latest_tag` 能拿到 tag（默认忽略：CI/离线环境不跑）。
    /// 手动：`cargo test -- --ignored latest_tag_live`
    #[tokio::test]
    #[ignore = "需要联网；用 cargo test -- --ignored 手动跑"]
    async fn latest_tag_live() {
        let tag = latest_tag()
            .await
            .expect("应能从 releases/latest 的重定向拿到 tag");
        assert!(tag.starts_with('v'), "tag 形如 vX.Y.Z，实际 {tag}");
    }
}
