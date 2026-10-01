//! 客户端配置文件的公共落盘件：跟随符号链接写入、写前备份、原子 600 落盘。
//!
//! 这里的两条规则都来自真机踩坑（codex 那边先踩的，pi 这边一样适用）：
//!
//! 1. **写目标要跟随符号链接**：dotfiles 常把 `~/.codex` / `~/.pi` 管成链接，直接在链接
//!    路径上 tmp + rename 会把**链接本身**换成普通文件，与仓库里的版本分叉；但**备份不跟随**，
//!    始终放在传入路径旁边，免得含 token 的备份落进用户的 dotfiles 仓库里。
//! 2. **权限在建文件时就是 600**：`fs::write` 会按 umask 摊成 0644，而这些文件里有 API key。

use std::fs;
use std::path::{Path, PathBuf};

/// 每次导入前把现有文件备份成 `<原名>.apim.bak`。
pub(crate) const BACKUP_SUFFIX: &str = "apim.bak";

/// `<path>` 对应的备份路径。
pub(crate) fn backup_path_of(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("config");
    path.with_file_name(format!("{name}.{BACKUP_SUFFIX}"))
}

/// 真正要写进去的那个文件（符号链接 → 指向的真实文件）。
///
/// 链接指向不存在的位置（悬空链接）时 canonicalize 会失败，那就就地问写。
pub(crate) fn resolve_write_target(path: &Path) -> PathBuf {
    let is_symlink = fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);
    if is_symlink {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// 原子写文本：先把现有内容备份成 `<原名>.apim.bak`，再 tmp(带 pid) + rename。
/// 返回备份文件路径（原文件不存在时为 None）。
pub(crate) fn write_with_backup(path: &Path, text: &str) -> Result<Option<PathBuf>, String> {
    let target = resolve_write_target(path);
    let backup = match fs::read_to_string(&target) {
        Ok(old) => {
            let backup = backup_path_of(path);
            crate::config::write_private(&backup, &old)
                .map_err(|err| format!("写备份 {} 失败：{err}", backup.display()))?;
            Some(backup)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("读取 {} 失败：{err}", target.display())),
    };

    crate::config::write_private(&target, text)
        .map_err(|err| format!("写 {} 失败：{err}", target.display()))?;
    Ok(backup)
}
