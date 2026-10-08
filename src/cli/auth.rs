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
            // 把这份 OAuth 凭据导入 Codex 的**官方路**（写 auth.json + 摘掉 config.toml
            // 的第三方路由），TUI 里是 AUTH 行按 `x`；细节见 clients::codex::official
            Some("import-codex") => {
                let cred = openai_auth::load(&ctx.config_dir)?.ok_or_else(|| {
                    anyhow::anyhow!("还没有 OAuth 凭据，先运行 apim auth openai login")
                })?;
                let home = crate::clients::codex::codex_home();
                let report = crate::clients::codex::import_official(&home, &cred)
                    .map_err(anyhow::Error::msg)?;
                // 与 TUI 同一口径：写成功才重启（codex 的 daemon 只在启动时读一次配置）
                let restart = crate::clients::codex::restart_daemon().unwrap_or_default();
                println!(
                    "已把 OpenAI Codex OAuth 导入 Codex 官方路（{}）",
                    home.display()
                );
                if !report.removed.is_empty() {
                    println!("  已从 config.toml 摘掉 {}", report.removed.join(" / "));
                }
                for backup in &report.backups {
                    println!("  旧配置备份为 {}", backup.display());
                }
                if restart.killed > 0 {
                    println!(
                        "  已重启 Codex 守护进程 {} 个，重开它即可用官方账号",
                        restart.killed
                    );
                } else {
                    println!("  重开 Codex 生效");
                }
                Ok(())
            }
            _ => bail!("未知子命令。用法：apim auth openai <login|status|logout|import-codex>"),
        },
        _ => bail!("用法：apim auth openai <login|status|logout|import-codex>"),
    }
}
