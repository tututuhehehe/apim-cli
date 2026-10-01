//! 导入期间对 codex home 的排他锁（`.apim-import.lock`）。
//!
//! 锁覆盖「读 → 改 → 写 → 校验」整段：`config.toml` 是 read-modify-write，两个 apim
//! 同时导入时，后写者会把前者刚加的 `[model_providers.<key>]` 块整段抹掉，而两边都报成功
//! （tmp 名带 pid 只保证不写坏文件，不保证不丢内容）。

use std::path::{Path, PathBuf};

/// 导入期间占住的锁文件名（放在 codex home 里）。
const LOCK_FILE: &str = ".apim-import.lock";

/// 导入锁。
///
/// 进程崩溃留下的锁（里面记的 pid 已经不在）会被下一个实例认领，不会把用户永久锁在门外。
pub(super) struct HomeLock {
    path: PathBuf,
}

impl HomeLock {
    pub(super) fn acquire(home: &Path) -> Result<Self, String> {
        let path = home.join(LOCK_FILE);
        match create_lock_file(&path) {
            Ok(()) => Ok(Self { path }),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&path) {
                    let _ = std::fs::remove_file(&path);
                    if create_lock_file(&path).is_ok() {
                        return Ok(Self { path });
                    }
                }
                Err(format!(
                    "另一个 apim 正在改写 codex 配置（{}）；等它结束再试。\
                     若确认没有其它实例在跑，删掉这个文件即可",
                    path.display()
                ))
            }
            Err(err) => Err(format!("创建锁 {} 失败：{err}", path.display())),
        }
    }
}

impl Drop for HomeLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 独占创建锁文件并写入自己的 pid（`create_new` = O_EXCL，两个进程只有一个能成）。
fn create_lock_file(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    writeln!(file, "{}", std::process::id())
}

/// 锁是否属于一个已经死掉的进程（内容坏了也当陈旧，否则一次异常退出就永久占着）。
fn lock_is_stale(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return true;
    };
    !process_alive(pid)
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // `kill -0` 只探测「进程在不在 / 有没有权限」，不发信号；把它的输出丢掉，
    // 否则进程不存在时那句 `kill: 999999: No such process` 会打到用户终端里
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    // 非 unix 不做存活性探测：锁一律当「还活着」—— 宁可让人手动删，也不抢锁
    true
}
