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
//! 目录条目照 codex 官方字段**手写迷你条目**（GLM / DeepSeek 官方接入文档同款），
//! 不克隆内置的 GPT 条目，也不内嵌大段模板文本。

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

/// 导入期间占住的锁文件名（放在 codex home 里）。
const LOCK_FILE: &str = ".apim-import.lock";

/// 导入锁。
///
/// `import_in` 对 `config.toml` 是 read-modify-write：两个 apim 同时导入时，后写者会把
/// 前者刚加的 `[model_providers.<key>]` 块整段抹掉，而两边都报成功（tmp 名带 pid 只保证
/// 不写坏文件，不保证不丢内容）。锁覆盖「读 → 改 → 写 → 校验」整段。
/// 进程崩溃留下的锁（里面记的 pid 已经不在）会被下一个实例认领，不会把用户永久锁在门外。
struct HomeLock {
    path: PathBuf,
}

impl HomeLock {
    fn acquire(home: &Path) -> Result<Self, String> {
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

/// 一次自动重启的结果。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RestartReport {
    /// 成功杀掉的守护进程数。
    pub killed: usize,
    /// 命令行里同时出现 codex 与 app-server、但**不是标准形态**的进程数
    /// （例如 `sh -c "codex app-server"`）。不做自动重启，只在提示里让用户手动重启 ——
    /// 放宽匹配去杀它会把用户自己的会话一起带走。
    pub ambiguous: usize,
}

/// 重启 codex 的 app-server 守护进程。
///
/// **为什么必须重启**：codex 的模型目录（`model_catalog_json`）只在 app-server 启动那一刻
/// 读一次，之后一直缓存 —— 实测：导入后新开的 codex 会话仍然列出旧目录（`codex exec` 却能
/// 用新模型），因为 TUI/桌面端都挂在同一个常驻 daemon 上。cc-switch 遇到同一件事，也只是
/// 提示用户重启 Codex（v3.16.1 release notes）。
///
/// 这里直接把在跑的 `codex app-server` 杀掉（codex 下次启动会自动起新的）。
/// 返回成功杀掉的进程数；0 = 当时没有 daemon 在跑。不想让它动进程就设 `APIM_NO_RESTART_CODEX=1`。
pub fn restart_daemon() -> Result<RestartReport, String> {
    restart_daemon_with(std::env::var_os("APIM_NO_RESTART_CODEX").is_some())
}

/// 同上，但「是否禁用」由调用方给（环境变量在 edition 2024 里不能安全地改，测试走这个入口）。
pub(crate) fn restart_daemon_with(disabled: bool) -> Result<RestartReport, String> {
    if disabled {
        return Ok(RestartReport::default());
    }
    let (pids, ambiguous) = scan_codex_servers()?;
    // 只扫描一次、只杀「这次调用开始前就在跑」的那批：
    // 重扫再杀会连带杀掉刚被拉起、已经加载了新配置的 daemon，
    // 甚至杀掉用户此刻新开的会话 —— 既无必要又有害。
    let mut killed = 0;
    for pid in pids {
        let ok = std::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        // 只统计真的杀成功的，别把 EPERM 也算进「已重启 N 个」
        if ok {
            killed += 1;
        }
    }
    Ok(RestartReport { killed, ambiguous })
}

/// 扫一遍进程表：返回（可安全杀掉的守护进程 pid，像 daemon 但形态不标准的进程数）。
///
/// 严格匹配只认「可执行文件叫 codex + 子命令位是 `app-server`」，不碰用户自己的 codex 会话；
/// 不标准的形态（`sh -c "codex app-server"` 之类）只计数、不杀 —— 误杀用户的会话比不帮你重启更糟。
#[cfg(unix)]
fn scan_codex_servers() -> Result<(Vec<u32>, usize), String> {
    let output = std::process::Command::new("ps")
        .args(["-eo", "pid=,command="])
        .output()
        .map_err(|err| format!("执行 ps 失败：{err}"))?;
    let me = std::process::id();
    let mut pids = Vec::new();
    let mut ambiguous = 0;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(pid) = line
            .split_whitespace()
            .next()
            .and_then(|token| token.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me {
            continue;
        }
        if is_codex_server(line) {
            pids.push(pid);
        } else if looks_like_wrapped_codex_server(line) {
            ambiguous += 1;
        }
    }
    Ok((pids, ambiguous))
}

#[cfg(not(unix))]
fn scan_codex_servers() -> Result<(Vec<u32>, usize), String> {
    // 不去动 Windows 上的用户进程；导入后会把「需重启 codex」写进提示
    Ok((Vec::new(), 0))
}

/// 一行 `ps -eo pid=,command=` 输出是不是 codex 的 app-server（守护）进程。
/// 可执行文件名必须正好是 `codex`，且**第一个参数**必须是 `app-server` 子命令。
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
    // 只认子命令位（argv[2]）：`codex app-server [--listen … | daemon pid-update-loop]`。
    // 不能扫「任意 token == app-server」—— 那会把 `codex --profile app-server`、
    // `codex exec "app-server"` 这类用户自己的会话也算进来，杀掉就是误杀。
    matches!(tokens.next(), Some("app-server"))
}

/// 「像 codex 的 daemon，但命令行是包装形态」——例如 `sh -c "codex app-server"`、
/// `env -i /path/codex app-server`。**只用于提示，绝不据此杀进程。**
#[cfg(unix)]
pub(crate) fn looks_like_wrapped_codex_server(line: &str) -> bool {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    // tokens[0] 是 pid；标准形态（tokens[1] 就是 codex）由 is_codex_server 负责
    let Some(wrapper) = tokens.get(1) else {
        return false;
    };
    let name = wrapper.rsplit('/').next().unwrap_or(wrapper);
    if !["sh", "bash", "zsh", "dash", "env", "node"].contains(&name) {
        return false;
    }
    // 引号在 ps 里是字面字符：比较前先剥掉，别让 `"codex` / `app-server"` 漏掉
    let strip = |token: &str| token.trim_matches(['"', '\'']).to_string();
    let mut seen_codex = false;
    for token in &tokens[2..] {
        let token = strip(token);
        let base = token.rsplit('/').next().unwrap_or(&token).to_string();
        if base == "codex" {
            seen_codex = true;
        } else if seen_codex && token == "app-server" {
            return true;
        }
    }
    false
}

/// 面板上显示的配置文件位置（尊重 `CODEX_HOME`，别写死 `~/.codex/...`）。
pub fn config_hint() -> String {
    let text = codex_home().join("config.toml").display().to_string();
    // 在 HOME 下就缩成 `~/…`：面板里铺一整条绝对路径太吵。
    // 设了 `CODEX_HOME`（不在 HOME 下）时仍如实显示那条路径，绝不写成 `~/.codex`。
    match std::env::var_os("HOME").and_then(|home| home.to_str().map(str::to_string)) {
        Some(home) if !home.is_empty() && text.starts_with(&home) => text.replacen(&home, "~", 1),
        _ => text,
    }
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
