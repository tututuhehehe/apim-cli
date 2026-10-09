//! `~/.pi/agent/models.json` 的读写（`auth.json` 只读、`settings.json` 完全不碰）。
//!
//! 为什么 `auth.json` 一个字都不写、只动哪些键、备份与权限见 `docs/clients/pi.md` 的「怎么写」
//! 与「只动我们认识的键」—— **改这个文件之前先读它**。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::clients::file_io;

/// 每模型清单（我们写的那份）。
pub const MODELS_FILE: &str = "models.json";
/// Pi 的设置文件。**apim 不读也不写它**（一键导入不动用户的默认 provider / 默认模型设定；
/// ★ 的判定只扫凭据，见 `active`），所以这个常量只给测试用：断言导入没碰过 settings.json。
#[cfg(test)]
pub const SETTINGS_FILE: &str = "settings.json";
/// Pi 自己的凭据库（`/login` 写的，**apim 只读不写**，见 `active` 与 AGENTS.md 约定 14）。
pub const AUTH_FILE: &str = "auth.json";

pub fn models_path(dir: &Path) -> PathBuf {
    dir.join(MODELS_FILE)
}

#[cfg(test)]
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
            Err(err) => Err(format!(
                "解析 {} 失败（未改动）：{err}；若文件里有 `//` 注释（pi 自己能读，apim 不读），先删掉注释再试",
                path.display()
            )),
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
