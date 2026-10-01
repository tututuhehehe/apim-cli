//! `~/.codex/config.toml` 的读写：用 toml_edit 保注释、保顺序，写前备份，原子落盘 600。

use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value, value};

/// 写进 `model_catalog_json` 上方的一行注释：模型不在本文件里，直接告诉用户去哪找。
const CATALOG_HINT: &str = "勾选的模型写在这个文件里（codex 只认独立文件，不在 config.toml 内）";

/// 一次导入要写进 config.toml 的内容。
pub struct ProviderWrite<'a> {
    /// `[model_providers.<key>]` 的表名（保留 id 已加前缀）。
    pub key: &'a str,
    pub name: &'a str,
    pub base_url: &'a str,
    pub api_key: &'a str,
    /// 生成的模型目录文件名（相对 CODEX_HOME）。
    pub catalog_file: &'a str,
    pub model: &'a str,
    /// 默认思考强度，写进 `model_reasoning_effort`。
    pub reasoning_effort: &'a str,
}

/// 读配置；文件不存在 → 空文档（等价于「全新安装的 codex」）。
/// 解析失败直接报错：绝不拿一个读不懂的文件去覆盖。
pub fn read(path: &Path) -> Result<DocumentMut, String> {
    match fs::read_to_string(path) {
        Ok(text) => text
            .parse::<DocumentMut>()
            .map_err(|err| format!("解析 {} 失败（未改动）：{err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(err) => Err(format!("读取 {} 失败：{err}", path.display())),
    }
}

/// 把一次导入写进文档：顶层 `model` / `model_provider` / `model_catalog_json`，
/// 以及 `[model_providers.<key>]`。其它厂商块和用户自己的键一律原样保留。
pub fn apply(doc: &mut DocumentMut, write: &ProviderWrite) -> Result<(), String> {
    set_string(doc, "model", write.model);
    set_string(doc, "model_provider", write.key);
    set_string(doc, "model_reasoning_effort", write.reasoning_effort);
    set_string_with_hint(doc, "model_catalog_json", write.catalog_file, CATALOG_HINT);

    if doc.get("model_providers").is_none() {
        // 隐式表：自己不产生 `[model_providers]` 头，只带出下面的子表
        let mut table = Table::new();
        table.set_implicit(true);
        doc.insert("model_providers", Item::Table(table));
    }
    let providers = doc
        .get_mut("model_providers")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| {
            "~/.codex/config.toml 里的 model_providers 不是表，apim 不覆盖它".to_string()
        })?;
    providers.insert(write.key, Item::Table(provider_table(write)));
    Ok(())
}

/// provider 子表：只放 codex 认的字段。同一块里 `env_key` / `auth` 和
/// `experimental_bearer_token` 不能共存，新建表天然没有它们（旧块被整体替换）。
fn provider_table(write: &ProviderWrite) -> Table {
    let mut table = Table::new();
    table.insert("name", value(write.name));
    table.insert("base_url", value(write.base_url));
    // codex 0.134+ 只接受 responses，写死避免踩 `wire_api = "chat"` 的硬报错
    table.insert("wire_api", value("responses"));
    table.insert("experimental_bearer_token", value(write.api_key));
    table
}

/// 顶层字符串键：已存在则只换值（保留键上的注释与行尾注释），不存在则新建。
fn set_string(doc: &mut DocumentMut, key: &str, text: &str) {
    set_string_with_hint(doc, key, text, "");
}

/// 同上，但新建这个键时在它前面写一行注释（`hint` 为空则不写）。
/// 注释必须挂在「键」的 decor 上：挂到 value 的 decor 上会把值挤到下一行（非法 TOML）。
fn set_string_with_hint(doc: &mut DocumentMut, key: &str, text: &str, hint: &str) {
    if let Some(old) = doc.get(key).and_then(Item::as_value) {
        let mut new = Value::from(text);
        *new.decor_mut() = old.decor().clone();
        doc[key] = Item::Value(new);
        return;
    }
    doc[key] = Item::Value(Value::from(text));
    if !hint.is_empty()
        && let Some(mut entry) = doc.key_mut(key)
    {
        entry.leaf_decor_mut().set_prefix(format!("\n# {hint}\n"));
    }
}

/// `<config.toml>` 对应的备份路径（`write` 就是写到这里）。
pub fn backup_path_of(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml"),
        super::BACKUP_SUFFIX
    ))
}

/// 原子写入：先把现有内容备份成 `<config.toml>.apim.bak`，再原子替换。
/// 返回备份文件路径（原文件不存在时为 None）。
///
/// 备份与正文都走 `config::write_private`：这份配置里有 `experimental_bearer_token`，
/// 权限必须**建文件时**就是 600 —— `fs::write` 会按 umask 落成 0644，
/// 等于把密钥复制一份给全机可读（且旧 key 会长期留在备份里，轮换也没用）。
pub fn write(path: &Path, text: &str) -> Result<Option<PathBuf>, String> {
    let backup = match fs::read_to_string(path) {
        Ok(old) => {
            let backup = backup_path_of(path);
            crate::config::write_private(&backup, &old)
                .map_err(|err| format!("写备份 {} 失败：{err}", backup.display()))?;
            Some(backup)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("读取 {} 失败：{err}", path.display())),
    };

    crate::config::write_private(path, text)
        .map_err(|err| format!("写 {} 失败：{err}", path.display()))?;
    Ok(backup)
}
