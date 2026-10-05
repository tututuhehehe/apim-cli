//! OAuth 凭据管理命令（用户可见输出都在这里，`openai_auth` 不往终端写）。
use crate::{cli::Ctx, openai_auth};
use anyhow::{Result, bail};

pub(crate) async fn run(ctx: &Ctx, argv: &[String]) -> Result<()> {
    match argv.first().map(String::as_str) {
        Some("openai") => match argv.get(1).map(String::as_str) {
            Some("login") => {
                let log = openai_auth::log_path(&ctx.config_dir);
                eprintln!("正在打开浏览器完成 OpenAI/Codex 授权（最多等 10 分钟）…");
                openai_auth::login(&ctx.config_dir)
                    .await
                    .map_err(|err| anyhow::anyhow!("{err:#}\n登录详情见 {}", log.display()))?;
                println!(
                    "OpenAI Codex OAuth 已保存到 {}（诊断日志 {}）",
                    openai_auth::credential_path(&ctx.config_dir).display(),
                    log.display()
                );
                Ok(())
            }
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
            _ => bail!("未知子命令。用法：apim auth openai <login|status|logout>"),
        },
        _ => bail!("用法：apim auth openai <login|status|logout>"),
    }
}
