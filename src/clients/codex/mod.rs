//! 把 apim 的密钥 / 厂商 / 模型一键写进 Codex 配置（`~/.codex/config.toml`）。
//!
//! Codex 允许在 `config.toml` 里同时定义多个 `[model_providers.*]`，但同一时刻只有
//! 顶层 `model_provider` 指向的那一个是激活的。所以这里只切换「激活项」：已导入的
//! 其它厂商配置块原样保留（用户手写的注释、`[projects]`、`[tui]` 也一律不动）。
//!
//! 写入内容：
//! - `model_provider` = 厂商 id（保留 id 加 `apim-` 前缀，见 [`provider_key`]）
//! - `model` = 勾选的默认模型
//! - `[model_providers.<key>]`：name / base_url / `wire_api = "responses"` /
//!   `experimental_bearer_token`
//! - `model_catalog_json`：勾选模型生成的目录，让 codex 的 `/model` 能列出它们
//!
//! 两点来自 codex 源码的硬约束（0.159.2 实测）：
//! 1. `wire_api = "chat"` 已被移除，只接受 `"responses"`；
//! 2. `model_catalog_json` 的条目必须有 `base_instructions` 或
//!    `model_messages.instructions_template`，两样都缺会解析报错 —— 见 `catalog`。
//!
//! 目录条目一律从**本机安装的 codex**（`codex debug models --bundled`）克隆，不内嵌
//! 任何模板文本，因此永远跟 codex 版本一致。

mod catalog;
mod config_file;
mod store;

pub use catalog::DEFAULT_EFFORT;
pub use store::CodexState;

use std::path::{Path, PathBuf};

/// Codex 保留的内置 provider id：用户自定义 provider 不能占用这些名字。
const RESERVED_PROVIDER_IDS: &[&str] = &[
    "openai",
    "ollama",
    "lmstudio",
    "amazon-bedrock",
    "amazon-bedrock-runtime",
];

/// 生成的模型目录文件名。写进 `model_catalog_json` 时用相对路径
/// （codex 按 `CODEX_HOME` 解析相对路径，源码 `load_model_catalog` 已确认）。
pub const CATALOG_FILE: &str = "apim-models.json";

/// 每次导入前把现有 `config.toml` 备份到 `<config.toml>.apim.bak`，作为回滚锚点。
const BACKUP_SUFFIX: &str = "apim.bak";

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
    let catalog_path = home.join(CATALOG_FILE);
    catalog::write_catalog(&catalog_path, &entries)?;

    let config_path = home.join("config.toml");
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
    let backup_path = config_file::write(&config_path, &doc.to_string())?;

    // 端到端校验：让 codex 自己解析这份配置 + 目录，勾选的模型必须都在。
    catalog::verify(bin, home, &request.models)?;

    Ok(ImportReport {
        provider_key: key,
        model: request.default_model.clone(),
        models: request.models.clone(),
        reasoning_effort: effort.to_string(),
        backup_path,
    })
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

/// 重启 codex 的 app-server 守护进程。
///
/// **为什么必须重启**：codex 的模型目录（`model_catalog_json`）只在 app-server 启动那一刻
/// 读一次，之后一直缓存 —— 实测：导入后新开的 codex 会话仍然列出旧目录（`codex exec` 却能
/// 用新模型），因为 TUI/桌面端都挂在同一个常驻 daemon 上。cc-switch 遇到同一件事，也只是
/// 提示用户重启 Codex（v3.16.1 release notes）。
///
/// 这里直接把在跑的 `codex app-server` 杀掉（codex 下次启动会自动起新的）。
/// 返回杀掉的进程数；0 = 当时没有 daemon 在跑。不想让它动进程就设 `APIM_NO_RESTART_CODEX=1`。
/// 重启 codex 的 app-server 守护进程。
pub fn restart_daemon() -> Result<usize, String> {
    if std::env::var_os("APIM_NO_RESTART_CODEX").is_some() {
        return Ok(0);
    }
    let mut killed = 0;
    // 杀两轮：`daemon pid-update-loop` 会把主 daemon 拉回来，第一轮之后可能有新的
    for _ in 0..2 {
        let pids = codex_server_pids()?;
        if pids.is_empty() {
            break;
        }
        for pid in pids {
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .status();
            killed += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    Ok(killed)
}

/// 找在跑的 codex app-server 相关进程（含 `daemon pid-update-loop`）。
/// 只认「可执行文件叫 codex + 带 `app-server` 子命令」，不碰用户自己的 codex 会话，
/// 也不会误匹配命令行里恰好含这两个词的无关进程（如 `ps | grep codex`）。
#[cfg(unix)]
fn codex_server_pids() -> Result<Vec<u32>, String> {
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid=,command="])
        .output()
        .map_err(|err| format!("执行 ps 失败：{err}"))?;
    let me = std::process::id();
    let pids = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| is_codex_server(line))
        .filter_map(|line| line.split_whitespace().next()?.parse::<u32>().ok())
        .filter(|pid| *pid != me)
        .collect();
    Ok(pids)
}

#[cfg(not(unix))]
fn codex_server_pids() -> Result<Vec<u32>, String> {
    // 不去动 Windows 上的用户进程；导入后会把「需重启 codex」写进提示
    Ok(Vec::new())
}

/// 一行 `ps -eo pid=,command=` 输出是不是 codex 的 app-server（守护）进程。
/// 可执行文件名必须正好是 `codex`，且参数里要有 `app-server` 子命令。
#[cfg(unix)]
pub(crate) fn is_codex_server(line: &str) -> bool {
    let mut tokens = line.split_whitespace();
    let _pid = tokens.next();
    let Some(binary) = tokens.next() else {
        return false;
    };
    let name = binary.rsplit('/').next().unwrap_or(binary);
    if name != "codex" && name != "codex.exe" {
        return false;
    }
    tokens.any(|token| token == "app-server")
}

/// `~/.codex`（尊重 `CODEX_HOME`）。
pub fn codex_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".codex")
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

#[cfg(test)]
mod tests;
