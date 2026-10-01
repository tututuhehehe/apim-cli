//! install.sh 渠道：下载该 tag 的官方安装脚本 → 校验 → 执行。
//!
//! 为什么不是 `curl … | sh`：
//! - URL 钉到本次要更新到的 tag（不再用可变的 `main` 分支），内容可复现；
//! - 不再边下边执行，下载被中途截断/返回错误页时不会执行半截脚本；
//! - 先做形状校验（是 shell 脚本、是本仓库的安装器、含 sha256 校验步骤），
//!   再按 Release 里发布的 `install.sh.sha256` 校验摘要（**没有摘要就拒绝执行**）；
//! - 顺带不再依赖 `curl`（用进程内 reqwest 下载），只依赖 `shasum`/`sha256sum`。
//!
//! Windows 上没有这条渠道：`run_channel_update` 在那儿直接 `bail!`，整个模块不参与编译。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::http::http_text;
use super::{Channel, REPO, RunError, install_sh_url, run_command};

/// install.sh 渠道：下载该 tag 的脚本 + 发布的摘要 → 形状校验 + sha256 校验 → 执行。
pub(super) async fn update_via_install_sh(
    exe: &Path,
    latest_tag: Option<&str>,
    json: bool,
) -> Result<()> {
    let tag = latest_tag.ok_or_else(|| {
        anyhow!("查不到最新版本号，无法确定要校验哪个版本的 install.sh（网络问题？）")
    })?;
    let script_url = install_sh_url(tag);
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

    let status = run_command(
        "sh",
        [script_path.as_os_str()],
        None,
        &[("APIM_INSTALL_DIR", dir.as_os_str())],
    );
    let _ = std::fs::remove_dir_all(&work);
    match status {
        Ok(_) => Ok(()),
        Err(RunError::Exited(code)) => bail!(
            "install.sh 更新失败（退出码 {}）；目录不可写时手动执行：\n  \
             curl -fsSL '{script_url}' -o /tmp/apim-install.sh && less /tmp/apim-install.sh\n  \
             sudo APIM_INSTALL_DIR={} sh /tmp/apim-install.sh",
            code.unwrap_or(-1),
            dir.display()
        ),
        Err(RunError::Spawn(err)) => bail!(
            "跑不了 sh（{err}）；手动执行：{}\n  {script_url}",
            Channel::Binary.command_hint()
        ),
    }
}

/// 形状校验：确认拿到的确实是本仓库的 install.sh，而不是错误页 / HTML / 别的脚本。
/// （摘要校验更强，但多这一道闸能把「网络中间件返回 200 + 一坨 HTML」挡在摘要校验之前。）
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
fn verify_digest(work: &Path) -> Result<()> {
    for (tool, args) in [
        ("shasum", &["-a", "256", "-c", "install.sh.sha256"][..]),
        ("sha256sum", &["-c", "install.sh.sha256"][..]),
    ] {
        match run_command(tool, args, Some(work), &[]) {
            Ok(_) => return Ok(()),
            Err(RunError::Exited(_)) => {
                bail!("install.sh 的 sha256 校验失败，已中止（下载可能被篡改或损坏）")
            }
            Err(RunError::Spawn(err)) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(RunError::Spawn(err)) => return Err(anyhow!("执行 {tool} 失败：{err}")),
        }
    }
    bail!("找不到 shasum / sha256sum，无法校验 install.sh 的摘要")
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// 摘要校验真跑一遍（会调 shasum/sha256sum）：正确摘要放行、被改过的脚本拦住。
    /// 这是全仓唯一一处把校验交给系统工具的地方，必须有真测试兜着。
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
