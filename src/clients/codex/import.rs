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

use std::path::{Path, PathBuf};

use super::catalog::{self, CATALOG_FILE, DEFAULT_EFFORT};
use super::codex_home;
use super::config_file;
use super::lock::HomeLock;
use super::store::{self, CodexState};

/// Codex 保留的内置 provider id：用户自定义 provider 不能占用这些名字。
const RESERVED_PROVIDER_IDS: &[&str] = &[
    "openai",
    "ollama",
    "lmstudio",
    "amazon-bedrock",
    "amazon-bedrock-runtime",
];

/// 一次一键导入的输入。
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// apim 的厂商 id（recipe id）。
    pub provider_id: String,
    /// 厂商显示名，写进 `[model_providers.*].name`。
    pub provider_name: String,
    /// recipe 的 base_url；不带 `/v1` 时写入前补齐。
    pub base_url: String,
    /// 写进 `experimental_bearer_token` 的密钥。
    pub api_key: String,
    /// apim 的密钥别名，只进状态文件（用于 TUI 的 ★ 标记）。
    pub alias: String,
    /// 勾选导入的模型（至少 1 个）。
    pub models: Vec<String>,
    /// 默认模型，必须是 `models` 之一。
    pub default_model: String,
}

impl ImportRequest {
    /// 与 `KeyEntry::id()` 同构，用来把结果对应回某把密钥。
    pub fn key_id(&self) -> String {
        format!("{}.{}", self.provider_id, self.alias)
    }
}

/// 导入成功后的结果，用于 TUI 反馈。
#[derive(Debug, Clone)]
pub struct ImportReport {
    /// 实际写进 `[model_providers.<key>]` 的表名。
    pub provider_key: String,
    /// 默认模型（codex 顶层 `model`）。
    pub model: String,
    pub models: Vec<String>,
    /// 写进顶层 `model_reasoning_effort` 的思考强度（固定值，见 [`DEFAULT_EFFORT`]）。
    pub reasoning_effort: String,
    /// 写入前的备份文件；原本没有 config.toml 时为 None。
    pub backup_path: Option<PathBuf>,
}

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
    let _lock = HomeLock::acquire(home)?;

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
        reasoning_effort: effort.to_string(),
        backup_path,
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

/// 导入成功后记录 apim 侧的「当前导入项」（TUI 打 ★ / 面板提示用）。
/// 返回值第二项是状态文件写失败的说明（导入本身已成功）。
pub fn remember(
    config_dir: &Path,
    request: &ImportRequest,
    report: &ImportReport,
) -> (CodexState, Option<String>) {
    let state = CodexState {
        provider: request.provider_id.clone(),
        provider_name: request.provider_name.clone(),
        alias: request.alias.clone(),
        provider_key: report.provider_key.clone(),
        models: report.models.clone(),
        default_model: report.model.clone(),
        reasoning_effort: report.reasoning_effort.clone(),
    };
    let error = store::save(config_dir, &state).err();
    (state, error)
}

/// 读 apim 侧记录的「当前导入项」（面板/★ 标记用）。
pub fn current_state(config_dir: &Path) -> Option<CodexState> {
    store::load(config_dir)
}

/// 写进 `config.toml` 的 provider 表名：保留 id 会被 codex 拒绝，加前缀躲开。
pub fn provider_key(provider_id: &str) -> String {
    if RESERVED_PROVIDER_IDS.contains(&provider_id) {
        format!("apim-{provider_id}")
    } else {
        provider_id.to_string()
    }
}

/// codex 的 `base_url` 是 API 根地址（要带 `/v1`），recipe 的 base_url 不一定带，
/// 这里统一补齐（已经是 `/v1` 结尾的原样返回）。
pub fn normalize_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") || trimmed.ends_with("/v1beta") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}
