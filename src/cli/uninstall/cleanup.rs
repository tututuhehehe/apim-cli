//! 卸载时真正落在文件系统上的动作：删程序（含软链链）、删配置目录（`--purge`）、
//! 顺带探一下 `~/.codex` 里的残留。
//!
//! 这里的函数都尽量「幂等 + 可解释」：文件已经不在算成功（重复卸载不报错），
//! 没权限则给 sudo 的出路，长得不像 apim 的东西一律不删。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// 跟软链最多跟这么多跳就停（软链成环时不至于死循环）。
const MAX_LINK_HOPS: usize = 8;

/// 卸载要动的路径：调用路径本身 + 顺着软链一路跟下来的每一跳。
/// 只删真身会把链留成断链（`~/.local/bin/apim` → 别处这种布局），多级链同理。
/// 必须在删之前调用：删完 `read_link` 就解析不出来了。
pub(super) fn binary_targets(exe: &Path) -> Vec<PathBuf> {
    let mut targets = vec![exe.to_path_buf()];
    let mut current = exe.to_path_buf();
    for _ in 0..MAX_LINK_HOPS {
        if !is_symlink(&current) {
            break;
        }
        let Ok(next) = fs::read_link(&current) else {
            break;
        };
        // read_link 给的是原样目标：相对路径按「链接所在目录」解析
        let next = match current.parent() {
            Some(parent) if next.is_relative() => parent.join(next),
            _ => next,
        };
        if targets.contains(&next) {
            break; // 成环
        }
        targets.push(next.clone());
        current = next;
    }
    targets
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// install.sh 渠道：删裸二进制（链上叫 apim 的每一跳都删）。
/// 链指向别的名字的程序（用户把 apim 链到自己的脚本）时**不碰那个真身** ——
/// 那种情况只删链，剩下的留给用户自己处理（`run` 会把仍在磁盘上的路径报出来）。
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
            if is_expected_name(&path) {
                remove_file_quietly(&path)?;
            }
        }
        Ok(())
    }
}

/// 文件名必须是 apim（Windows 下 apim.exe）—— 再兜一层，免得把用户目录里同名的别的东西删了。
pub(super) fn is_expected_name(path: &Path) -> bool {
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
        // 只提示 `sudo rm 这一个文件`：`sudo apim uninstall` 会连 HOME 一起换掉，
        // --purge 就指到 /var/root 去了，反而给人「已经清干净」的错觉。
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => bail!(
            "没有权限删除 {}：{err}\n  用管理员身份单独删这个文件：sudo rm {}",
            path.display(),
            path.display()
        ),
        Err(err) => Err(err).with_context(|| format!("删除 {}", path.display())),
    }
}

/// 校验 `--purge` 的目标：目录名必须是 `apim`。`APIM_CONFIG_DIR` 指到别处时宁可拒绝，
/// 也不冒着 `rm -rf` 删错目录的风险。
pub(super) fn check_purge_target(dir: &Path) -> Result<()> {
    // 用 symlink_metadata：名字也要管到「断掉的软链」，不能因为链断了就当它不存在
    if fs::symlink_metadata(dir).is_err() {
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
    if fs::symlink_metadata(dir).is_err() {
        return Ok(None);
    }
    // 配置目录是软链（dotfiles 常这么管）时拒绝：`remove_dir_all` 对软链是删链还是跟进去删
    // 不值得赌 —— 前者会「报告已删密钥、真身还在」，后者会把用户的 dotfiles 仓库删了。
    if is_symlink(dir) {
        bail!(
            "{} 是软链（指向 {}），apim 不递归删软链。确认要删就自己动手：rm -rf {}",
            dir.display(),
            fs::read_link(dir)
                .map(|target| target.display().to_string())
                .unwrap_or_else(|_| "?".into()),
            dir.display()
        );
    }
    fs::remove_dir_all(dir).with_context(|| {
        format!(
            "删除配置目录 {}（别用 sudo：sudo 下 HOME 会变，可能指到别的目录）",
            dir.display()
        )
    })?;
    Ok(Some(dir.display().to_string()))
}

/// `~/.pi/agent` 里 apim 一键导入留下的文件（只报告、不删）。
pub(super) fn pi_leftovers() -> Vec<String> {
    pi_leftovers_in(&crate::clients::pi::agent_dir())
}

pub(super) fn pi_leftovers_in(dir: &Path) -> Vec<String> {
    let models = dir.join("models.json");
    let backup = crate::clients::file_io::backup_path_of(&models);
    let mut out = Vec::new();
    // models.json 里混着用户自己的 provider，只按 `apim-` 前缀的键判断 apim 有没有写过它
    if let Ok(text) = fs::read_to_string(&models)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(providers) = value.get("providers").and_then(|p| p.as_object())
        && providers.keys().any(|key| key.starts_with("apim-"))
    {
        out.push(format!(
            "{} 里的 providers.apim-* 条目（apiKey 是明文）",
            models.display()
        ));
    }
    // 同 codex：改写前的备份里也有 apiKey 明文，必须一起报
    if backup.exists() {
        out.push(format!(
            "{}（apim 改写 models.json 前留的备份，里面也有密钥）",
            backup.display()
        ));
    }
    out
}

/// `~/.codex` 里 apim 一键导入留下的文件（只报告、不删）。
pub(super) fn codex_leftovers() -> Vec<String> {
    codex_leftovers_in(&crate::clients::codex::codex_home())
}

pub(super) fn codex_leftovers_in(home: &Path) -> Vec<String> {
    let catalog = home.join("apim-models.json");
    let config = home.join("config.toml");
    let backup = home.join("config.toml.apim.bak");
    let mut out = Vec::new();
    if catalog.exists() {
        out.push(catalog.display().to_string());
    }
    // config.toml 里混着用户手写的配置，只能靠指针字符串判断 apim 有没有写过它
    if let Ok(text) = fs::read_to_string(&config)
        && text.contains("apim-models.json")
    {
        out.push(format!(
            "{} 里的 [model_providers.apim-*] / model_provider / model_catalog_json",
            config.display()
        ));
    }
    // 约定 11：改写前的备份里同样有 experimental_bearer_token（明文密钥），必须一起报
    if backup.exists() {
        out.push(format!(
            "{}（apim 改写 config.toml 前留的备份，里面也有密钥）",
            backup.display()
        ));
    }
    out
}
