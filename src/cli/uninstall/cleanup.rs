//! 卸载时真正落在文件系统上的动作：删程序（含软链）、删配置目录（`--purge`）、
//! 顺带探一下 `~/.codex` 里的残留。
//!
//! 这里的函数都尽量「幂等 + 可解释」：文件已经不在算成功（重复卸载不报错），
//! 没权限则给 sudo 的出路，目标长得不像 apim 的东西一律拒绝删。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// 卸载要动的路径：程序本体，调用路径本身是软链时连真身一起
/// （只删真身会留下断链 —— `~/.local/bin/apim` → 别处这种布局）。
/// 必须在删之前调用：删完 `canonicalize` 就解析不出来了。
pub(super) fn binary_targets(exe: &Path) -> Vec<PathBuf> {
    let mut targets = vec![exe.to_path_buf()];
    let is_link = fs::symlink_metadata(exe)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);
    if is_link {
        if let Ok(real) = exe.canonicalize() {
            if real != exe {
                targets.push(real);
            }
        }
    }
    targets
}

/// install.sh 渠道：删裸二进制。
pub(super) fn remove_binary(exe: &Path) -> Result<()> {
    if !is_expected_name(exe) {
        bail!(
            "拒绝删除 {}：文件名不是 apim（怕误删别的程序）。真的要删就自己动手：rm {}",
            exe.display(),
            exe.display()
        );
    }
    #[cfg(windows)]
    {
        // 文件占用是 Windows 的硬限制，只能让用户关掉进程后手动删
        bail!(
            "Windows 上正在运行的 exe 删不掉自己；请关掉本进程后手动删除：{}",
            exe.display()
        );
    }
    #[cfg(not(windows))]
    {
        for path in binary_targets(exe) {
            remove_file_quietly(&path)?;
        }
        Ok(())
    }
}

/// 文件名必须是 apim（Windows 下 apim.exe）—— 再兜一层，免得把用户目录里同名的别的东西删了。
fn is_expected_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    name == "apim" || name == "apim.exe"
}

/// 删除单个文件：已经不在（重复卸载、npm 已清）算成功；没权限时给 sudo 的出路。
fn remove_file_quietly(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => bail!(
            "没有权限删除 {}：{err}\n  用 sudo 重跑 `apim uninstall --yes`，或手动：sudo rm {}",
            path.display(),
            path.display()
        ),
        Err(err) => Err(err).with_context(|| format!("删除 {}", path.display())),
    }
}

/// 校验 `--purge` 的目标：目录名必须是 `apim`。`APIM_CONFIG_DIR` 指到别处时宁可拒绝，
/// 也不冒着 `rm -rf` 删错目录的风险。
pub(super) fn check_purge_target(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name != "apim" {
        bail!(
            "拒绝删除 {}：目录名不是 apim（怕误删）。确实要删就自己动手：rm -rf {}",
            dir.display(),
            dir.display()
        );
    }
    Ok(())
}

/// 删配置目录（`--purge`）：config.toml / secrets.toml / recipes / scripts 全在里面。
pub(super) fn purge_config_dir(dir: &Path) -> Result<Option<String>> {
    check_purge_target(dir)?;
    if !dir.exists() {
        return Ok(None);
    }
    fs::remove_dir_all(dir).with_context(|| format!("删除配置目录 {}", dir.display()))?;
    Ok(Some(dir.display().to_string()))
}

/// `~/.codex` 里 apim 一键导入留下的文件（只报告、不删）。
pub(super) fn codex_leftovers() -> Vec<String> {
    codex_leftovers_in(&crate::clients::codex::codex_home())
}

pub(super) fn codex_leftovers_in(home: &Path) -> Vec<String> {
    let catalog = home.join("apim-models.json");
    let config = home.join("config.toml");
    let mut out = Vec::new();
    if catalog.exists() {
        out.push(catalog.display().to_string());
    }
    // config.toml 里混着用户手写的配置，只能靠指针字符串判断 apim 有没有写过它
    if let Ok(text) = fs::read_to_string(&config) {
        if text.contains("apim-models.json") {
            out.push(format!(
                "{} 里的 [model_providers.apim-*] / model_provider / model_catalog_json",
                config.display()
            ));
        }
    }
    out
}
