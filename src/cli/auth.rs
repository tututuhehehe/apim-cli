//! OAuth credential management commands.
use crate::{cli::Ctx, openai_auth};
use anyhow::{Result, bail};

pub(crate) async fn run(ctx: &Ctx, argv: &[String]) -> Result<()> {
    match argv.first().map(String::as_str) {
        Some("openai") => match argv.get(1).map(String::as_str) {
            Some("login") => openai_auth::login(&ctx.config_dir).await,
            Some("logout") => {
                openai_auth::remove(&ctx.config_dir)?;
                println!("已移除 OpenAI Codex OAuth 凭据");
                Ok(())
            }
            Some("status") => match openai_auth::load(&ctx.config_dir)? {
                Some(c) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let expiry = if c.expires_at <= now {
                        "access token 已过期（查询时会尝试刷新）".to_string()
                    } else {
                        format!(
                            "access token 约 {} 分钟后过期",
                            (c.expires_at - now).div_ceil(60)
                        )
                    };
                    println!(
                        "OpenAI Codex OAuth 已配置 · {} · {expiry}",
                        c.email.as_deref().unwrap_or("账户已验证")
                    );
                    Ok(())
                }
                None => {
                    println!("OpenAI Codex OAuth 未配置");
                    Ok(())
                }
            },
            _ => {
                println!("用法：apim auth openai <login|status|logout>");
                Ok(())
            }
        },
        _ => bail!("用法：apim auth openai <login|status|logout>"),
    }
}
