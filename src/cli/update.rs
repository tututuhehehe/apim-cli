//! `apim update`：先认出「当前这个 apim 是从哪条渠道装的」，再走同一条渠道更新。
//!
//! 只有三条发布渠道（`docs/RELEASING.md` 的「各渠道速查」）：
//!
//! | 渠道 | 二进制长在哪 | 更新方式 |
//! |---|---|---|
//! | npm | `node_modules/apim-cli-<平台>/bin/apim` | `npm install -g apim-cli@latest` |
//! | Homebrew | `.../Cellar/apim/<版本>/bin/apim` | `brew upgrade apim` |
//! | install.sh | `/usr/local/bin` 或 `~/.local/bin` 的裸二进制 | 下载**该 tag 的**官方 install.sh、校验 sha256 后执行 |
//!
//! `target/` 下的开发构建与 `~/.cargo/bin` 里的副本都不算渠道（`detect_channel` 返回 None），
//! 一律不碰 —— 前者会被 Release 覆盖掉开发二进制，后者是 `cargo install` 留下的多余副本。
//!
//! ## install.sh 渠道为什么不是 `curl … | sh`
//!
//! 换成「下载到临时文件 → 校验 → 再执行」：
//! - URL 钉到本次要更新到的 tag（不再用可变的 `main` 分支），内容可复现；
//! - 不再边下边执行，下载被中途截断/返回错误页时不会执行半截脚本；
//! - 先做形状校验（是 shell 脚本、是本仓库的安装器、含 sha256 校验步骤），
//!   再按 Release 里发布的 `install.sh.sha256` 校验摘要（**没有摘要就拒绝执行**）；
//! - 顺带不再依赖 `curl`（用进程内 reqwest 下载），只依赖 `shasum`/`sha256sum`。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

/// 仓库与安装脚本：与 Cargo.toml 的 `repository` / `install.sh` 保持一致。
const REPO: &str = "tututuhehehe/apim-cli";
/// 单个响应体的上限：脚本/摘要都是几 KB，超过就是被塞了别的东西。
const MAX_BODY_BYTES: usize = 64 * 1024;

/// 当前 apim 的安装渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channel {
    Npm,
    Homebrew,
    /// install.sh 装的裸二进制（`/usr/local/bin/apim`、`~/.local/bin/apim` 等）。
    Binary,
}

impl Channel {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Channel::Npm => "npm",
            Channel::Homebrew => "Homebrew",
            Channel::Binary => "install.sh（裸二进制）",
        }
    }

    /// 这条渠道的更新命令（给用户看的原样命令）。
    fn command_hint(self) -> &'static str {
        match self {
            Channel::Npm => "npm install -g apim-cli@latest",
            Channel::Homebrew => "brew upgrade apim",
            Channel::Binary => {
                "curl -fsSL https://raw.githubusercontent.com/tututuhehehe/apim-cli/main/install.sh | sh"
            }
        }
    }
}

/// 认渠道。`None` = 不是这三条发布渠道装的，一律不动：
/// - `target/` 下是 `cargo run` / `cargo build` 的产物，覆盖掉就把开发二进制换成 Release 版；
/// - `~/.cargo/bin` 是 `cargo install` 留下的副本（项目明确不推荐这条路，本机 PATH 里还有一份
///   被它遮挡的 npm 版），install.sh 往那儿装只会多一份版本不一致的二进制。
///
/// 同时看原路径与 `canonicalize()` 之后的路径：Homebrew 会经过 `/opt/homebrew/bin/apim`
/// 这样的软链，只看原路径认不出来。
pub(crate) fn detect_channel(exe: &Path) -> Option<Channel> {
    let canonical = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let raw = exe.to_string_lossy().replace('\\', "/");
    let real = canonical.to_string_lossy().replace('\\', "/");
    let has = |needle: &str| raw.contains(needle) || real.contains(needle);

    if has("/target/debug/") || has("/target/release/") || has("/.cargo/bin/") {
        return None;
    }
    // npm 全局包：二进制在 node_modules/apim-cli-<平台>/bin/ 下
    if has("node_modules") && has("apim-cli") {
        return Some(Channel::Npm);
    }
    // Homebrew（含 Linuxbrew）：brew 把真身放在 Cellar/apim/<版本>/
    if has("Cellar/apim") {
        return Some(Channel::Homebrew);
    }
    Some(Channel::Binary)
}

/// `apim update [--check] [--force] [--json]`。
pub(crate) async fn run(args: &super::Args) -> Result<()> {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("apim"));
    // None = 本地开发构建 / cargo 副本，不属于任何发布渠道
    let channel = detect_channel(&exe);
    let current = env!("CARGO_PKG_VERSION");
    let json = args.has("json");
    let check_only = args.has("check");
    let force = args.has("force");

    let latest_tag = match latest_tag().await {
        Ok(tag) => Some(tag),
        Err(err) => {
            if !json {
                eprintln!("apim update: 查最新版本失败：{err}");
            }
            None
        }
    };
    let up_to_date = latest_tag
        .as_deref()
        .is_some_and(|tag| tag.trim_start_matches('v') == current);

    if json {
        println!(
            "{}",
            serde_json::json!({
                "current": current,
                "latest": latest_tag.as_deref().map(|tag| tag.trim_start_matches('v')),
                "channel": channel.map(Channel::label),
                "exe": exe.display().to_string(),
                "up_to_date": up_to_date,
                "command": channel.map(Channel::command_hint),
            })
        );
    } else {
        match channel {
            Some(channel) => println!("当前：apim {current}（渠道：{}）", channel.label()),
            None => println!("当前：apim {current}（本地开发构建 / cargo 副本，不在发布渠道里）"),
        }
        println!("  {}", exe.display());
        match &latest_tag {
            Some(tag) => println!("最新：{tag}"),
            None => println!("最新：查不到（网络问题？）"),
        }
    }

    if check_only {
        if !json && up_to_date {
            println!("已是最新。");
        }
        return Ok(());
    }

    // 不属于三条发布渠道：不覆盖
    let Some(channel) = channel else {
        if !json {
            println!(
                "\n这是 `cargo run` / `cargo build` 的产物，或是 cargo install 留下来的副本，apim 不覆盖它。"
            );
            println!(
                "要换成发布版：装 install.sh 版 / npm 版 / Homebrew 版（见 README 安装一节）。"
            );
        }
        return Ok(());
    };

    // 已经是最新且没 --force：不再重装一遍（没意义，还多一次权限风险）
    if up_to_date && !force {
        if !json {
            println!("已是最新，无需更新。想强制重装加 --force。");
        }
        return Ok(());
    }

    run_channel_update(channel, &exe, latest_tag.as_deref(), json).await
}

/// 按渠道执行更新。
async fn run_channel_update(
    channel: Channel,
    exe: &Path,
    latest_tag: Option<&str>,
    json: bool,
) -> Result<()> {
    match channel {
        Channel::Npm => {
            if !json {
                println!("\n→ npm 渠道：npm install -g apim-cli@latest");
            }
            run_tool("npm", &["install", "-g", "apim-cli@latest"], "npm")?;
        }
        Channel::Homebrew => {
            if !json {
                println!("\n→ Homebrew 渠道：brew upgrade apim");
            }
            run_tool("brew", &["upgrade", "apim"], "brew")?;
        }
        Channel::Binary => {
            #[cfg(windows)]
            {
                // 先判平台再打印：否则会先输出「下载 install.sh」再报「Windows 没有这条渠道」
                bail!(
                    "Windows 没有 install.sh 渠道；请从 Release 下载 .zip 手动替换，或改用 npm：\n  \
                     npm install -g apim-cli\n  https://github.com/{REPO}/releases"
                );
            }
            #[cfg(not(windows))]
            {
                update_via_install_sh(exe, latest_tag, json).await?;
            }
        }
    }

    if !json {
        println!("\n更新完成。重开一个 shell 后确认：apim --version");
    }
    Ok(())
}

/// install.sh 渠道：下载该 tag 的脚本 + 发布的摘要 → 形状校验 + sha256 校验 → 执行。
#[cfg(not(windows))]
async fn update_via_install_sh(exe: &Path, latest_tag: Option<&str>, json: bool) -> Result<()> {
    let tag = latest_tag.ok_or_else(|| {
        anyhow!("查不到最新版本号，无法确定要校验哪个版本的 install.sh（网络问题？）")
    })?;
    let script_url = format!("https://raw.githubusercontent.com/{REPO}/{tag}/install.sh");
    let digest_url = format!("https://github.com/{REPO}/releases/download/{tag}/install.sh.sha256");

    // 安装目录钉在当前二进制的**真实**位置：若 PATH 里的 apim 是指向真实安装的符号链接，
    // 用未解析的路径会让 install.sh 把链接本身 mv 掉，真实安装点留旧版本。
    let dir = exe
        .canonicalize()
        .unwrap_or_else(|_| exe.to_path_buf())
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if !json {
        println!(
            "\n→ install.sh 渠道：下载并校验 {tag} 的官方安装脚本（装到 {}）",
            dir.display()
        );
        println!("  {script_url}");
    }

    let script = http_text(&script_url).await?;
    verify_install_script(&script)?;
    let digest = http_text(&digest_url).await.map_err(|err| {
        anyhow!(
            "{tag} 这个 Release 没有发布 install.sh.sha256（{err}）。\n\
             为安全起见不执行未校验的脚本；可手动更新：\n  \
             curl -fsSL '{script_url}' -o /tmp/apim-install.sh && less /tmp/apim-install.sh\n  \
             sh /tmp/apim-install.sh"
        )
    })?;

    // 脚本与摘要放进同一个临时目录，用 `shasum -c` 校验（摘要里记录的名字是 install.sh）
    let work = std::env::temp_dir().join(format!("apim-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).with_context(|| format!("创建 {}", work.display()))?;
    let script_path = work.join("install.sh");
    std::fs::write(&script_path, &script)
        .with_context(|| format!("写 {}", script_path.display()))?;
    std::fs::write(work.join("install.sh.sha256"), &digest)
        .with_context(|| format!("写 {}/install.sh.sha256", work.display()))?;
    if let Err(err) = verify_digest(&work) {
        let _ = std::fs::remove_dir_all(&work);
        return Err(err);
    }
    if !json {
        println!("  sha256 校验通过");
    }

    let status = Command::new("sh")
        .arg(&script_path)
        .env("APIM_INSTALL_DIR", &dir)
        .status();
    let _ = std::fs::remove_dir_all(&work);
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!(
            "install.sh 更新失败（退出码 {}）；目录不可写时手动执行：\n  \
             curl -fsSL '{script_url}' -o /tmp/apim-install.sh && less /tmp/apim-install.sh\n  \
             sudo APIM_INSTALL_DIR={} sh /tmp/apim-install.sh",
            status.code().unwrap_or(-1),
            dir.display()
        ),
        Err(err) => bail!(
            "跑不了 sh（{err}）；手动执行：{}\n  {script_url}",
            Channel::Binary.command_hint()
        ),
    }
}

/// 跑一个外部工具（npm / brew），失败时给出手动命令。
fn run_tool(tool: &str, args: &[&str], label: &str) -> Result<()> {
    let status = Command::new(tool).args(args).status();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!("{label} 更新失败（退出码 {}）", status.code().unwrap_or(-1)),
        Err(err) => bail!("跑不了 {tool}（{err}）；手动执行：{}", args.join(" ")),
    }
}

/// 形状校验：确认拿到的确实是本仓库的 install.sh，而不是错误页 / HTML / 别的脚本。
/// （摘要校验更强，但多这一道闸能把「网络中间件返回 200 + 一坨 HTML」挡在摘要校验之前。）
#[cfg(not(windows))]
fn verify_install_script(script: &str) -> Result<()> {
    if !script.starts_with("#!") {
        bail!("下载到的不是 shell 脚本（开头不是 #!）：请检查网络/代理");
    }
    if !script.contains(&format!("REPO=\"{REPO}\"")) {
        bail!("脚本里没有 REPO=\"{REPO}\"，不是本项目的安装脚本，拒绝执行");
    }
    if !script.contains("shasum") && !script.contains("sha256sum") {
        bail!("脚本里没有 sha256 校验步骤，拒绝执行");
    }
    Ok(())
}

/// 在 `work` 目录里按 `install.sh.sha256` 校验 `install.sh`。
/// 不自己算摘要：没有现成依赖，而系统工具正是 install.sh 自己依赖的那套。
#[cfg(not(windows))]
fn verify_digest(work: &Path) -> Result<()> {
    for (tool, args) in [
        ("shasum", vec!["-a", "256", "-c", "install.sh.sha256"]),
        ("sha256sum", vec!["-c", "install.sh.sha256"]),
    ] {
        match Command::new(tool).args(&args).current_dir(work).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(_) => bail!("install.sh 的 sha256 校验失败，已中止（下载可能被篡改或损坏）"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(anyhow!("执行 {tool} 失败：{err}")),
        }
    }
    bail!("找不到 shasum / sha256sum，无法校验 install.sh 的摘要")
}

/// GET 一个文本资源，带长度上限。
async fn http_text(url: &str) -> Result<String> {
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
/// 走重定向而不是 GitHub API，这样没有匿名 API 的 60 次/小时限流。
async fn latest_tag() -> Result<String> {
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
fn tag_from_url(url: &str) -> Option<String> {
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
    fn detects_npm_global_install() {
        assert_eq!(
            detect_channel(Path::new(
                "/opt/homebrew/lib/node_modules/apim-cli/node_modules/apim-cli-darwin-arm64/bin/apim"
            )),
            Some(Channel::Npm)
        );
        assert_eq!(
            detect_channel(Path::new(
                "C:\\Users\\me\\AppData\\Roaming\\npm\\node_modules\\apim-cli-windows-x64\\bin\\apim.exe"
            )),
            Some(Channel::Npm)
        );
    }

    #[test]
    fn detects_homebrew() {
        assert_eq!(
            detect_channel(Path::new("/opt/homebrew/Cellar/apim/0.1.9/bin/apim")),
            Some(Channel::Homebrew)
        );
        // Linuxbrew
        assert_eq!(
            detect_channel(Path::new("/home/me/.linuxbrew/Cellar/apim/0.1.9/bin/apim")),
            Some(Channel::Homebrew)
        );
    }

    #[test]
    fn detects_install_sh_binary() {
        // install.sh 的默认落点：/usr/local/bin（可写）或 ~/.local/bin
        assert_eq!(
            detect_channel(Path::new("/usr/local/bin/apim")),
            Some(Channel::Binary)
        );
        assert_eq!(
            detect_channel(Path::new("/Users/me/.local/bin/apim")),
            Some(Channel::Binary)
        );
    }

    /// 本地开发构建不属于任何发布渠道 —— 绝不能拿 Release 覆盖掉它。
    #[test]
    fn dev_builds_are_not_a_channel() {
        assert_eq!(
            detect_channel(Path::new("/Users/me/apim-cli/target/debug/apim")),
            None
        );
        assert_eq!(
            detect_channel(Path::new(
                "/Users/me/Documents/VIBE/apim管理/target/release/apim"
            )),
            None
        );
    }

    /// 路径里带 apim-cli 但**不含 node_modules** 时不能误判成 npm（两者要同时满足）。
    #[test]
    fn repo_path_alone_is_not_npm() {
        assert_eq!(
            detect_channel(Path::new("/Users/me/apim-cli/bin/apim")),
            Some(Channel::Binary)
        );
    }

    /// `~/.cargo/bin` 是 cargo install 留下的副本（本机 PATH 里还有一份被它遮挡的
    /// npm 版），跟 target/ 一样不属于发布渠道。
    #[test]
    fn cargo_install_is_not_a_release_channel() {
        assert_eq!(detect_channel(Path::new("/Users/me/.cargo/bin/apim")), None);
    }

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

    /// 摘要校验真跑一遍（会调 shasum/sha256sum）：正确摘要放行、被改过的脚本拦住。
    /// 这是全仓唯一一处把校验交给系统工具的地方，必须有真测试兜着。
    #[cfg(not(windows))]
    #[test]
    fn digest_verification_accepts_and_rejects() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-digest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("install.sh");
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();

        // 与 CI 相同的方式生成摘要（`<hash>  install.sh`）
        let generated = Command::new("shasum")
            .args(["-a", "256", "install.sh"])
            .current_dir(&dir)
            .output();
        let Ok(generated) = generated else {
            let _ = std::fs::remove_dir_all(&dir);
            return; // 没有 shasum 就跳过（macOS / Linux 都有）
        };
        assert!(generated.status.success(), "shasum 生成摘要失败");
        std::fs::write(dir.join("install.sh.sha256"), &generated.stdout).unwrap();

        assert!(verify_digest(&dir).is_ok(), "正确摘要必须放行");

        // 篡改脚本 → 必须拒绝
        std::fs::write(&script, "#!/bin/sh\necho tampered\n").unwrap();
        assert!(verify_digest(&dir).is_err(), "被改过的脚本必须拦住");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 形状校验：错误页 / HTML / 别的仓库的脚本都要被挡住。
    #[cfg(not(windows))]
    #[test]
    fn install_script_shape_is_checked() {
        assert!(
            verify_install_script("<html>404</html>").is_err(),
            "HTML 要挡"
        );
        assert!(
            verify_install_script("#!/bin/sh\necho hi\n").is_err(),
            "缺 REPO 要挡"
        );
        assert!(
            verify_install_script("#!/bin/sh\nREPO=\"other/repo\"\nshasum -a 256 x\n").is_err(),
            "别的仓库要挡"
        );
        assert!(
            verify_install_script(&format!(
                "#!/bin/sh\nREPO=\"{REPO}\"\necho no-checksum-here\n"
            ))
            .is_err(),
            "没有校验步骤要挡"
        );
        // 真实 install.sh 的形状（含 REPO= 与 shasum 校验）必须通过
        assert!(
            verify_install_script(&format!(
                "#!/bin/sh\nset -eu\nREPO=\"{REPO}\"\nshasum -a 256 -c x\n"
            ))
            .is_ok()
        );
    }
}
