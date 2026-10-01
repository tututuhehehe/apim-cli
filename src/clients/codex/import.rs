//! 一次一键导入的编排：定位本机 codex → 生成目录 → 改写 config.toml → 端到端校验 → 记下导入项。
//!
//! 写进 `config.toml` 的内容：
//! - `model_provider` = 厂商 id（保留 id 加 `apim-` 前缀，见 [`provider_key`]）
//! - `model` = 勾选的默认模型
//! - `[model_providers.<key>]`：name / base_url / `wire_api = "responses"` /
//!   `experimental_bearer_token`
//! - `model_catalog_json`：勾选模型生成的目录，让 codex 的 `/model` 能列出它们
//!
//! 顺序上有两条硬要求：**先纯内存改配置（失败时磁盘一点没动）**，**校验不过要把两处改动
//! 都还原**（否则用户看到「导入失败」，`~/.codex/config.toml` 其实已经切到新厂商 + 新目录）。

use std::path::Path;

use super::catalog::{self, CATALOG_FILE, DEFAULT_EFFORT};
use super::codex_home;
use super::config_file;
use crate::clients::lock::ImportLock;
use crate::clients::{ImportReport, ImportRequest};

/// 厂商 id → codex 的 API 根地址（两个客户端都要带 `/v1`，实现见 `clients::normalize_base_url`）。
pub use crate::clients::normalize_base_url;

/// Codex 保留的内置 provider id：用户自定义 provider 不能占用这些名字。
const RESERVED_PROVIDER_IDS: &[&str] = &[
    "openai",
    "ollama",
    "lmstudio",
    "amazon-bedrock",
    "amazon-bedrock-runtime",
];

/// 一键导入：定位本机 codex → 生成目录 → 改写 config.toml → 端到端校验。
pub fn import(request: &ImportRequest) -> Result<ImportReport, String> {
    let codex_bin = catalog::locate_codex();
    import_in(&codex_home(), request, codex_bin.as_deref())
}

/// 可注入 codex 目录（`CODEX_HOME` 由调用方给），测试用。
///
/// 模型目录内容不依赖 codex（条目是照官方文档手写的迷你条目），但**校验必须用 codex**：
/// 写完要真的让它解析一遍这份配置 + 目录，这才算“导入成功”。
pub fn import_in(
    home: &Path,
    request: &ImportRequest,
    codex_bin: Option<&Path>,
) -> Result<ImportReport, String> {
    let Some(bin) = codex_bin else {
        return Err(
            "未找到 codex 可执行文件；导入后要用它校验配置（安装 codex，或用 APIM_CODEX_BIN 指定路径）"
                .to_string(),
        );
    };
    let key = provider_key(&request.provider_id);
    let base_url = normalize_base_url(&request.base_url);
    // 思考强度不让人选：每个模型都声明全四档，默认档固定，想改就在 codex 里用 /model
    let effort = DEFAULT_EFFORT;
    let entries = catalog::build(&request.models, &request.default_model)
        .map_err(|err| format!("生成模型目录失败：{err}"))?;

    std::fs::create_dir_all(home).map_err(|err| format!("创建 {} 失败：{err}", home.display()))?;
    // 整段「读 → 改 → 写 → 校验」都在锁里：否则两个实例会互相抹掉对方的 provider 块
    let _lock = ImportLock::acquire(home, "codex")?;

    let config_path = home.join("config.toml");
    // 先把配置读进来、改好（纯内存）：这一步失败时磁盘还一点没动
    let mut doc = config_file::read(&config_path)?;
    config_file::apply(
        &mut doc,
        &config_file::ProviderWrite {
            key: &key,
            name: &request.provider_name,
            base_url: &base_url,
            api_key: &request.api_key,
            catalog_file: CATALOG_FILE,
            model: &request.default_model,
            reasoning_effort: effort,
        },
    )?;

    let catalog_path = home.join(CATALOG_FILE);
    // 留一份旧目录：校验不过时要能还原回去
    let previous_catalog = std::fs::read_to_string(&catalog_path).ok();
    catalog::write_catalog(&catalog_path, &entries)?;
    let backup_path = config_file::write(&config_path, &doc.to_string())?;

    // 端到端校验：让 codex 自己解析这份配置 + 目录，勾选的模型必须都在。
    // 失败就把两处改动都还原 —— 否则用户看到「导入失败」，而 ~/.codex/config.toml
    // 其实已经指向新厂商 + 新目录，codex 侧读不通。
    if let Err(err) = catalog::verify(bin, home, &request.models) {
        let rolled_back = rollback(
            &config_path,
            backup_path.as_deref(),
            &catalog_path,
            previous_catalog.as_deref(),
        );
        return Err(if rolled_back {
            format!("{err}（已还原到导入前的配置）")
        } else {
            format!(
                "{err}（自动还原失败，请手动把 {} 覆盖回 {}）",
                config_file::backup_path_of(&config_path).display(),
                config_path.display()
            )
        });
    }

    Ok(ImportReport {
        provider_key: key,
        model: request.default_model.clone(),
        models: request.models.clone(),
        detail: Some(format!("强度 {effort}")),
        backups: backup_path.into_iter().collect(),
    })
}

/// 校验失败时把这次导入写下去的两处改动还原：
/// config.toml 用备份回写（备份为 None = 原来没有这个文件 → 删掉刚写的），
/// 目录恢复成之前的内容（没有则删掉）。返回是否全部还原成功。
fn rollback(
    config_path: &Path,
    backup: Option<&Path>,
    catalog_path: &Path,
    previous_catalog: Option<&str>,
) -> bool {
    let config_ok = match backup {
        // fs::copy 会连权限一起复制（备份是 600），不会把配置摊成 644
        Some(backup) => std::fs::copy(backup, config_path).is_ok(),
        None => remove_if_exists(config_path),
    };
    let catalog_ok = match previous_catalog {
        Some(text) => std::fs::write(catalog_path, text).is_ok(),
        None => remove_if_exists(catalog_path),
    };
    config_ok && catalog_ok
}

/// 删文件；本来就不存在也算成功。
fn remove_if_exists(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(err) => err.kind() == std::io::ErrorKind::NotFound,
    }
}

/// 写进 `config.toml` 的 provider 表名：保留 id 会被 codex 拒绝，加前缀躲开。
pub fn provider_key(provider_id: &str) -> String {
    if RESERVED_PROVIDER_IDS.contains(&provider_id) {
        format!("apim-{provider_id}")
    } else {
        provider_id.to_string()
    }
}
