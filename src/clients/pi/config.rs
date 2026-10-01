//! `~/.pi/agent/models.json` 与 `settings.json` 的读写。
//!
//! 两份都是 JSON，而且都可能被用户手写（`models.json` 里常有人写 `modelOverrides` /
//! `headers` / `compat`）：所以**只按我们认识的键改，其余字段原样保留**，
//! 写前备份成 `<原名>.apim.bak`，落盘走 `file_io`（跟随符号链接 + 原子 + 建文件即 600）。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::clients::file_io;

/// 每模型清单（我们写的那份）。
pub const MODELS_FILE: &str = "models.json";
/// Pi 的设置（我们只动 `defaultProvider` / `defaultModel` / `enabledModels`）。
pub const SETTINGS_FILE: &str = "settings.json";

pub fn models_path(dir: &Path) -> PathBuf {
    dir.join(MODELS_FILE)
}

pub fn settings_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE)
}

/// 读一个 JSON 对象；文件不存在 → 空对象（等价于「全新装的 pi」）。
/// 解析失败直接报错：绝不拿一个读不懂的文件去覆盖（它可能是用户手写的）。
pub fn read(path: &Path) -> Result<Map<String, Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err(format!("{} 的顶层不是 JSON 对象（未改动）", path.display())),
            Err(err) => Err(format!("解析 {} 失败（未改动）：{err}", path.display())),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(err) => Err(format!("读取 {} 失败：{err}", path.display())),
    }
}

/// 写回 JSON（2 空格缩进 + 末尾换行，跟 pi 自己写出来的样子一致），返回备份路径。
pub fn write(path: &Path, map: &Map<String, Value>) -> Result<Option<PathBuf>, String> {
    let text = serde_json::to_string_pretty(&Value::Object(map.clone()))
        .map_err(|err| format!("序列化 {} 失败：{err}", path.display()))?;
    file_io::write_with_backup(path, &format!("{text}\n"))
}
