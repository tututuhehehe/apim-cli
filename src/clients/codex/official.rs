//! 把 apim 的 OpenAI Codex OAuth 凭据导入 Codex 的**官方路**（与第三方 provider 并列的另一条路）。
//!
//! 机制来自 cc-switch（`codexProviderPresets.ts` 的「OpenAI Official」卡 + `codex_config.rs`）
//! 与本机 codex 0.161 实测，几条硬约束别凭感觉改：
//!
//! - **官方路 = `auth.json` 里的 ChatGPT 登录**：`auth_mode: "chatgpt"` + `tokens{id_token,
//!   access_token, refresh_token, account_id}` + `last_refresh`。config.toml **没有** `model_provider`
//!   （缺省即内置 `openai`）时 codex 才用它；有第三方 `model_provider` 时 auth.json 被晾在一边。
//! - **`refresh_token` 必须带上**：access token 过期后 codex 自己拿它去
//!   `auth.openai.com/oauth/token` 刷新（codex 源码 `request_chatgpt_token_refresh`），client id
//!   与 apim 用的是同一个 `app_EMoamEEZ73f0CkXaXp7hrann`、同样不发 `resource`，所以这份凭据
//!   codex 能自续。少了它，「裸跑 codex」在 access token 到期后会静默失效。
//! - `last_refresh` 只是兜底：codex 优先看 access token 的 `exp`（`should_refresh_proactively`，
//!   提前 5 分钟），解析不出来才看 `last_refresh`（8 天），写当前时间即可。
//! - **`cli_auth_credentials_store = keyring|ephemeral` 时 codex 根本不读 auth.json** → 直接拒绝，
//!   不写一份看不见的凭据（apim 不碰系统钥匙串）。
//! - 端到端校验用 **`codex login status`**：它离线读 auth.json（形状不对/未登录都 exit 1）并顺带
//!   解析 config.toml，比只看自己写的文件更硬。
//!
//! 与第三方导入（`import.rs`）的差别：这条**不动** `[model_providers.*]` 块（没有 `model_provider`
//! 指向就是死的），只摘掉 `model_provider` / `model` / apim 自己写的 `model_catalog_json` ——
//! 用户手写的 `notify` / `[projects]` / `[plugins]` … 一个字不碰（cc-switch 是整份清空，因为它有
//! provider 数据库兜底，apim 没有）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use toml_edit::DocumentMut;

use super::catalog::{self, CATALOG_FILE};
use super::import::remove_if_exists;
use super::{codex_home, config_file};
use crate::clients::file_io;
use crate::clients::lock::ImportLock;
use crate::openai_auth::Credential;
use crate::util::truncate;

/// codex 的 ChatGPT 登录凭据文件名（相对 `CODEX_HOME`）。
pub const AUTH_FILE: &str = "auth.json";

/// `codex login status` 报出 ChatGPT 登录的那句话（实测 0.161，写在 stderr）。
/// 用它而不是 `Logged in`：API Key 登录是 `Logged in using an API key - sk-***`，那条路不算数。
const LOGGED_IN_MARKER: &str = "Logged in using ChatGPT";

/// 一次官方路导入的结果（提示条只读这里）。
#[derive(Debug, Clone)]
pub struct OfficialReport {
    /// 从 config.toml 摘掉的顶层键（如实告诉用户改了什么）。
    pub removed: Vec<&'static str>,
    /// 写盘前的备份（auth.json / config.toml；原来没有那个文件就没有备份）。
    pub backups: Vec<PathBuf>,
}

/// Codex 现在走哪条路 —— **回读客户端现场**算出来的（与密钥行 ★ 同一个思路，不记台账）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexRoute {
    /// 官方路 + `auth.json` 里是 ChatGPT 登录（就是 apim 导入的这一路）
    OfficialOauth,
    /// 官方路但用的是 `auth.json` 的 `OPENAI_API_KEY`
    OfficialApiKey,
    /// config.toml 指着别的 provider（第三方中转站，或 `ollama` 这类内置 provider）
    Provider(String),
    /// 两条都没有：codex 没登录
    SignedOut,
}

impl CodexRoute {
    /// 额度面板 AUTH 区那一行。
    pub fn label(&self) -> String {
        match self {
            CodexRoute::OfficialOauth => "Codex：OpenAI 官方 OAuth".into(),
            CodexRoute::OfficialApiKey => "Codex：OpenAI 官方 API Key".into(),
            CodexRoute::Provider(key) => format!("Codex：provider {key}"),
            CodexRoute::SignedOut => "Codex：未登录".into(),
        }
    }
}

/// 回读 `~/.codex` 现场：现在走官方路还是 provider 路。
///
/// 顺序就是 codex 自己的解析顺序：先看 `model_provider`（有第三方 provider 就用它，auth.json 作废），
/// 缺省/`openai` 才轮到 auth.json。读不到 / 解析失败都当「没登录」，不猜。
pub fn codex_route(home: Option<&Path>) -> CodexRoute {
    let home = home.map(Path::to_path_buf).unwrap_or_else(codex_home);
    let provider = config_file::read(&home.join("config.toml"))
        .ok()
        .and_then(|doc| {
            doc.get("model_provider")
                .and_then(|item| item.as_str())
                .map(str::to_string)
        })
        .filter(|key| !key.is_empty() && key != "openai");
    if let Some(key) = provider {
        return CodexRoute::Provider(key);
    }
    match read_auth(&home) {
        Some(auth) if auth_has_chatgpt_tokens(&auth) => CodexRoute::OfficialOauth,
        Some(auth) if auth_has_api_key(&auth) => CodexRoute::OfficialApiKey,
        _ => CodexRoute::SignedOut,
    }
}

/// 一次官方路导入：定位本机 codex → 写 auth.json → 摘掉 config.toml 的第三方路由 → 端到端校验。
pub fn import(home: &Path, cred: &Credential) -> Result<OfficialReport, String> {
    let codex_bin = catalog::locate_codex();
    import_in(home, cred, codex_bin.as_deref())
}

/// 可注入 codex 目录与可执行文件（测试用）。
///
/// 校验必须用 codex 自己（`codex login status`）—— 只看 apim 写的文件等于自己给自己判卷。
pub fn import_in(
    home: &Path,
    cred: &Credential,
    codex_bin: Option<&Path>,
) -> Result<OfficialReport, String> {
    let Some(bin) = codex_bin else {
        return Err(
            "未找到 codex 可执行文件；导入后要用它校验登录（安装 codex，或用 APIM_CODEX_BIN 指定路径）"
                .to_string(),
        );
    };
    check_credential(cred)?;

    std::fs::create_dir_all(home).map_err(|err| format!("创建 {} 失败：{err}", home.display()))?;
    // 整段「读 → 改 → 写 → 校验」都在锁里：与第三方导入共用同一把锁，两个实例不会互相抹掉
    let _lock = ImportLock::acquire(home, "codex")?;

    let config_path = home.join("config.toml");
    // 先纯内存改配置：这一步失败时磁盘还一点没动
    let mut doc = config_file::read(&config_path)?;
    check_auth_store(&doc)?;
    let removed = strip_third_party(&mut doc);
    let auth_text = auth_json_text(cred)?;

    // 先写 auth.json 再写 config.toml（与 cc-switch 同序）：第二步失败就把第一步回滚，
    // 否则用户会看到「导入失败」，而 codex 其实已经换成 ChatGPT 登录了
    let auth_path = home.join(AUTH_FILE);
    let auth_backup = file_io::write_with_backup(&auth_path, &auth_text)?;
    let config_backup = match config_file::write(&config_path, &doc.to_string()) {
        Ok(backup) => backup,
        Err(err) => {
            let restored = rollback(&auth_path, auth_backup.as_deref());
            return Err(if restored {
                format!("{err}（已还原 auth.json）")
            } else {
                format!(
                    "{err}（自动还原失败，请手动把 {} 覆盖回 {}）",
                    file_io::backup_path_of(&auth_path).display(),
                    auth_path.display()
                )
            });
        }
    };

    // 端到端校验：让 codex 自己读这份 auth.json + config.toml，必须认成「已登录」
    if let Err(err) = verify_login(bin, home) {
        // 两处都要试着还原：写成 `rollback(...) && match ...` 会在 auth.json 还原失败时短路掉
        // config.toml 的还原，留下「apim 说导入失败、codex 配置其实已经被摘掉第三方路由」的状态
        let auth_restored = rollback(&auth_path, auth_backup.as_deref());
        let config_restored = match &config_backup {
            // fs::copy 连权限一起复制（备份是 600），不会把配置摊成 644
            Some(backup) => fs::copy(backup, &config_path).is_ok(),
            None => remove_if_exists(&config_path),
        };
        return Err(if auth_restored && config_restored {
            format!("{err}（已还原到导入前的配置）")
        } else {
            format!(
                "{err}（自动还原失败，请手动把 {} / {} 覆盖回去）",
                file_io::backup_path_of(&auth_path).display(),
                config_file::backup_path_of(&config_path).display()
            )
        });
    }

    Ok(OfficialReport {
        removed,
        backups: auth_backup.into_iter().chain(config_backup).collect(),
    })
}

/// 凭据三件套缺一不可；`account_id` 是 Codex 用量/账号绑定的必需 claim。
fn check_credential(cred: &Credential) -> Result<(), String> {
    if cred.id_token.trim().is_empty()
        || cred.access_token.trim().is_empty()
        || cred.refresh_token.trim().is_empty()
    {
        return Err(
            "OAuth 凭据不完整（缺 id/access/refresh token），按 o 重新登录后再导入".to_string(),
        );
    }
    if cred.account_id.trim().is_empty() {
        return Err(
            "凭据里没有 ChatGPT account id（动态注册档位没有它），按 o 用 Codex CLI 档位重新登录"
                .to_string(),
        );
    }
    Ok(())
}

/// codex 只在 `cli_auth_credentials_store` 缺省 / `file` / `auto` 时读 auth.json。
/// `keyring` / `ephemeral` / 写错的值一律拒绝：写一份 codex 看不见的凭据比报错更糟
/// （`auto` 是「钥匙串优先、auth.json 兜底」，写 auth.json 仍然有效）。
fn check_auth_store(doc: &DocumentMut) -> Result<(), String> {
    match doc
        .get("cli_auth_credentials_store")
        .and_then(|item| item.as_str())
    {
        None | Some("file") | Some("auto") => Ok(()),
        Some(other) => Err(format!(
            "codex 配置里 cli_auth_credentials_store = \"{other}\"，凭据不走 auth.json \
             （apim 不写系统钥匙串）；先把它改成 \"file\" 或删掉这一行"
        )),
    }
}

/// 摘掉会让官方路跑错的顶层键，返回摘掉了哪些（提示条要如实说）：
/// - `model_provider`：留着就还指着第三方 provider（官方路 = 没有这个键）
/// - `model`：apim 写的是第三方模型名，官方端点没有它（codex 回自己的默认模型）
/// - `model_catalog_json`：**只在它指向 apim 自己生成的目录时**摘；手写的别的目录不动
fn strip_third_party(doc: &mut DocumentMut) -> Vec<&'static str> {
    let mut removed = Vec::new();
    if doc.remove("model_provider").is_some() {
        removed.push("model_provider");
    }
    if doc.remove("model").is_some() {
        removed.push("model");
    }
    let apim_catalog =
        doc.get("model_catalog_json").and_then(|item| item.as_str()) == Some(CATALOG_FILE);
    if apim_catalog && doc.remove("model_catalog_json").is_some() {
        removed.push("model_catalog_json");
    }
    removed
}

/// `auth.json` 的正文（与 Codex CLI 自己浏览器登录落盘的形状对齐，字段顺序也一样）。
fn auth_json_text(cred: &Credential) -> Result<String, String> {
    let value = json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": {
            "id_token": cred.id_token,
            "access_token": cred.access_token,
            "refresh_token": cred.refresh_token,
            "account_id": cred.account_id,
        },
        "last_refresh": rfc3339_utc(now()),
    });
    serde_json::to_string_pretty(&value).map_err(|err| format!("序列化 auth.json 失败：{err}"))
}

/// 端到端校验：`codex login status` 真的读 auth.json（形状不对直接报错）并解析 config.toml。
///
/// **两个流都要看，而且必须认成 ChatGPT 登录**。本机 codex 0.161 实测的三种输出都写在
/// **stderr**：
/// - ChatGPT 登录：`Logged in using ChatGPT`，exit 0
/// - API Key 登录：`Logged in using an API key - sk-***`，exit 0 —— **不能算过**：那条路不走
///   auth.json 里的 ChatGPT 凭据，把成功说出来等于「官方 OAuth 已导入」是假话
/// - 未登录 / 配置非法：`Not logged in` / 报错，exit 1
///
/// 所以只匹配 `Logged in` 太松（`cli_auth_credentials_store = auto` 且钥匙串里另有一把 API Key
/// 时，codex 报的就是 API Key 那一条），必须匹配 [`LOGGED_IN_MARKER`]。stdout 也一起看：
/// 同一句话在不同版本可能写在另一个流。
fn verify_login(codex_bin: &Path, home: &Path) -> Result<(), String> {
    let output = Command::new(codex_bin)
        .args(["login", "status"])
        .env("CODEX_HOME", home)
        .output()
        .map_err(|err| format!("执行 {} 失败：{err}", codex_bin.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success()
        && (stdout.contains(LOGGED_IN_MARKER) || stderr.contains(LOGGED_IN_MARKER))
    {
        return Ok(());
    }
    // 报错优先给 stderr（codex 的提示在那里），空的话再退回 stdout
    let detail = if stderr.trim().is_empty() {
        stdout.into_owned()
    } else {
        stderr.into_owned()
    };
    Err(format!(
        "codex 不认这份登录凭据：{}",
        truncate(&detail, 200)
    ))
}

/// 回滚 auth.json：有备份就覆盖回去，原来没有就删掉刚写的那个。返回是否成功。
fn rollback(auth_path: &Path, backup: Option<&Path>) -> bool {
    match backup {
        Some(backup) => fs::copy(backup, auth_path).is_ok(),
        None => remove_if_exists(auth_path),
    }
}

fn read_auth(home: &Path) -> Option<Value> {
    let text = fs::read_to_string(home.join(AUTH_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// `tokens` 里有没有真的 ChatGPT 凭据（元数据 `account_id` 不算）。
fn auth_has_chatgpt_tokens(auth: &Value) -> bool {
    ["id_token", "access_token", "refresh_token"]
        .iter()
        .any(|field| {
            auth.get("tokens")
                .and_then(|tokens| tokens.get(*field))
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty())
        })
}

fn auth_has_api_key(auth: &Value) -> bool {
    auth.get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Unix 秒 → RFC3339 UTC（`2026-01-02T03:04:05Z`）。codex 用 chrono 的 RFC3339 解析它。
///
/// 只有 `last_refresh` 这一处用得上，所以不为它引 chrono：日期部分用 Howard Hinnant 的
/// `civil_from_days`（chrono / time 内部同一个算法），闰年边界由单测守着。
fn rfc3339_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// 自 1970-01-01 起的天数 → (年, 月, 日)。Howard Hinnant `civil_from_days`。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let doe = (shifted - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential() -> Credential {
        Credential {
            client_id: "app_EMoamEEZ73f0CkXaXp7hrann".into(),
            host_id: "urn:uuid:test".into(),
            subject: "auth0|test".into(),
            email: Some("user@example.invalid".into()),
            id_token: "id.token.sig".into(),
            access_token: "access.token.sig".into(),
            refresh_token: "refresh-token".into(),
            expires_at: 4_000_000_000,
            scopes: vec!["openid".into()],
            account_id: "acct-test".into(),
            codex_family: true,
        }
    }

    #[test]
    fn rfc3339_matches_known_instants() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_704_067_200), "2024-01-01T00:00:00Z");
        // 2024 是闰年：2/29 存在
        assert_eq!(rfc3339_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        // 2000 是闰年（百年例外里的四百年例外），1900 不是
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(4_102_444_800), "2100-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_709_251_199), "2024-02-29T23:59:59Z");
    }

    #[test]
    fn auth_json_has_the_native_shape() {
        let text = auth_json_text(&credential()).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["auth_mode"], "chatgpt");
        assert!(value["OPENAI_API_KEY"].is_null());
        // 四个 token 字段一个都不能少（codex 靠 refresh_token 自刷新，缺 id/access 也认不出账号）
        assert_eq!(value["tokens"]["id_token"], "id.token.sig");
        assert_eq!(value["tokens"]["access_token"], "access.token.sig");
        assert_eq!(value["tokens"]["refresh_token"], "refresh-token");
        assert_eq!(value["tokens"]["account_id"], "acct-test");
        // last_refresh 必须是 chrono 认的 RFC3339
        let last = value["last_refresh"].as_str().unwrap();
        assert!(last.ends_with('Z') && last.len() == 20, "{last}");
    }

    #[test]
    fn credential_needs_all_three_tokens_and_an_account() {
        assert!(check_credential(&credential()).is_ok());
        let mut missing_refresh = credential();
        missing_refresh.refresh_token = String::new();
        assert!(check_credential(&missing_refresh).is_err());
        let mut no_account = credential();
        no_account.account_id = String::new();
        let err = check_credential(&no_account).unwrap_err();
        assert!(err.contains("account id"), "{err}");
    }

    #[test]
    fn strip_only_touches_apims_own_keys() {
        let mut doc: DocumentMut = r#"model = "deepseek-flash"
model_provider = "deepseek"
model_reasoning_effort = "high"
model_catalog_json = "apim-models.json"
notify = ["/bin/echo"]

[model_providers.deepseek]
base_url = "https://api.deepseek.com/v1"

[tui]
screen_reader_detection_done = true
"#
        .parse()
        .unwrap();
        let removed = strip_third_party(&mut doc);
        assert_eq!(
            removed,
            vec!["model_provider", "model", "model_catalog_json"]
        );
        let text = doc.to_string();
        // 注意：`[model_providers.deepseek]` 里也含 "model_provider" 子串，所以要按键判
        assert!(doc.get("model_provider").is_none());
        assert!(doc.get("model").is_none());
        assert!(doc.get("model_catalog_json").is_none());
        assert!(text.contains("model_reasoning_effort"));
        assert!(text.contains("notify"));
        assert!(text.contains("[model_providers.deepseek]"));
        assert!(text.contains("[tui]"));
    }

    #[test]
    fn strip_keeps_a_hand_written_catalog() {
        let mut doc: DocumentMut = "model_catalog_json = \"my-models.json\"\n".parse().unwrap();
        assert!(strip_third_party(&mut doc).is_empty());
        assert!(doc.to_string().contains("my-models.json"));
    }

    #[test]
    fn auth_store_modes_that_ignore_the_file_are_rejected() {
        let ok: DocumentMut = "".parse().unwrap();
        assert!(check_auth_store(&ok).is_ok());
        let file: DocumentMut = "cli_auth_credentials_store = \"file\"\n".parse().unwrap();
        assert!(check_auth_store(&file).is_ok());
        let auto: DocumentMut = "cli_auth_credentials_store = \"auto\"\n".parse().unwrap();
        assert!(check_auth_store(&auto).is_ok());
        let keyring: DocumentMut = "cli_auth_credentials_store = \"keyring\"\n"
            .parse()
            .unwrap();
        let err = check_auth_store(&keyring).unwrap_err();
        assert!(err.contains("keyring"), "{err}");
        let ephemeral: DocumentMut = "cli_auth_credentials_store = \"ephemeral\"\n"
            .parse()
            .unwrap();
        assert!(check_auth_store(&ephemeral).is_err());
    }

    #[test]
    fn route_reads_the_live_config() {
        let home = super::super::tests::helpers::temp_dir("official-route");
        // 什么都没写 → 未登录
        assert_eq!(codex_route(Some(&home)), CodexRoute::SignedOut);

        // 第三方 provider 优先于 auth.json（auth.json 被晾在一边）
        fs::write(
            home.join("config.toml"),
            "model_provider = \"deepseek\"\n[model_providers.deepseek]\nbase_url = \"https://x.invalid/v1\"\n",
        )
        .unwrap();
        fs::write(
            home.join(AUTH_FILE),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"a.b.c"}}"#,
        )
        .unwrap();
        assert_eq!(
            codex_route(Some(&home)),
            CodexRoute::Provider("deepseek".into())
        );

        // 官方路：没有 model_provider（或它等于内置 openai）+ ChatGPT 凭据
        fs::write(home.join("config.toml"), "model = \"gpt-6\"\n").unwrap();
        assert_eq!(codex_route(Some(&home)), CodexRoute::OfficialOauth);
        fs::write(home.join("config.toml"), "model_provider = \"openai\"\n").unwrap();
        assert_eq!(codex_route(Some(&home)), CodexRoute::OfficialOauth);

        // 只有 API Key 的 auth.json → 官方 API Key 路
        fs::write(home.join(AUTH_FILE), r#"{"OPENAI_API_KEY":"sk-test"}"#).unwrap();
        assert_eq!(codex_route(Some(&home)), CodexRoute::OfficialApiKey);

        // 只有元数据的 auth.json 不算登录
        fs::write(
            home.join(AUTH_FILE),
            r#"{"auth_mode":"chatgpt","tokens":{"account_id":"acct"}}"#,
        )
        .unwrap();
        assert_eq!(codex_route(Some(&home)), CodexRoute::SignedOut);
    }

    /// 端到端：假 codex 只认 `login status` 且输出「已登录」时，导入必须成功并留下两处备份；
    /// 假 codex 说「没登录」时必须**两处都还原**（否则用户看到失败、配置其实已经变了）。
    #[cfg(unix)]
    #[test]
    fn import_verifies_with_codex_and_rolls_back_on_failure() {
        use std::os::unix::fs::PermissionsExt;

        let fake = |dir: &Path, body: &str| {
            let bin = dir.join("codex");
            fs::write(&bin, body).unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
            bin
        };
        // 真机 codex 0.161 的形状：「Logged in using ChatGPT」写在 **stderr**、exit 0
        let logged_in = "#!/bin/sh\necho 'Logged in using ChatGPT' >&2\nexit 0\n";
        let logged_out = "#!/bin/sh\necho 'Not logged in' >&2\nexit 1\n";

        // 成功：auth.json 写对、config.toml 摘掉第三方路由、其余键保留
        let home = super::super::tests::helpers::temp_dir("official-ok");
        fs::write(
            home.join("config.toml"),
            "model = \"deepseek-flash\"\nmodel_provider = \"deepseek\"\nnotify = [\"/bin/echo\"]\n\n[model_providers.deepseek]\nbase_url = \"https://api.deepseek.com/v1\"\n",
        )
        .unwrap();
        let bin = fake(&home, logged_in);
        let report = import_in(&home, &credential(), Some(&bin)).unwrap();
        assert_eq!(report.removed, vec!["model_provider", "model"]);
        // 原来只有 config.toml：auth.json 是新建的，所以只有一份备份
        assert_eq!(report.backups.len(), 1);
        assert!(report.backups[0].ends_with("config.toml.apim.bak"));
        let auth: Value =
            serde_json::from_str(&fs::read_to_string(home.join(AUTH_FILE)).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["account_id"], "acct-test");
        let config = fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(!config.contains("model_provider = "));
        assert!(config.contains("notify"));
        assert!(config.contains("[model_providers.deepseek]"));
        // 备份必须存在（否则失败回滚就无从谈起）
        for backup in &report.backups {
            assert!(backup.exists(), "{} 不存在", backup.display());
        }

        // 失败：codex 不认这份凭据 → auth.json 与 config.toml 都要回到导入前
        let home = super::super::tests::helpers::temp_dir("official-fail");
        let before_config = "model = \"deepseek-flash\"\nmodel_provider = \"deepseek\"\n";
        fs::write(home.join("config.toml"), before_config).unwrap();
        let bin = fake(&home, logged_out);
        let err = import_in(&home, &credential(), Some(&bin)).unwrap_err();
        assert!(err.contains("已还原"), "{err}");
        assert!(!home.join(AUTH_FILE).exists(), "auth.json 应被删掉");
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            before_config
        );
    }

    /// `verify_login` 认「登录成功」不看是哪个流（真机写 stderr，别的版本可能写 stdout），
    /// 但 exit 非 0 就是没登录 —— 不能因为输出里恰好有这几个字就放过。
    #[cfg(unix)]
    #[test]
    fn verify_accepts_logged_in_on_either_stream() {
        use std::os::unix::fs::PermissionsExt;

        let home = super::super::tests::helpers::temp_dir("official-verify");
        let write_fake = |body: &str| {
            let bin = home.join("codex");
            fs::write(&bin, body).unwrap();
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
            bin
        };

        let stderr_bin = write_fake("#!/bin/sh\necho 'Logged in using ChatGPT' >&2\nexit 0\n");
        assert!(verify_login(&stderr_bin, &home).is_ok());
        let stdout_bin = write_fake("#!/bin/sh\necho 'Logged in using ChatGPT'\nexit 0\n");
        assert!(verify_login(&stdout_bin, &home).is_ok());
        // API Key 登录（真机 0.161 的原话）不是 ChatGPT 登录：不能算过，否则
        // cli_auth_credentials_store = auto + 钥匙串里另有一把 API Key 时会把假话说成真的
        let api_key_bin = write_fake(
            "#!/bin/sh\necho 'Logged in using an API key - sk-fake-***l-key' >&2\nexit 0\n",
        );
        let err = verify_login(&api_key_bin, &home).unwrap_err();
        assert!(err.contains("API key"), "{err}");
        // exit 非 0：即使输出里带着「Logged in」也不认
        let failed_bin = write_fake("#!/bin/sh\necho 'Logged in using ChatGPT' >&2\nexit 1\n");
        assert!(verify_login(&failed_bin, &home).is_err());
        let not_logged_in = write_fake("#!/bin/sh\necho 'Not logged in' >&2\nexit 1\n");
        let err = verify_login(&not_logged_in, &home).unwrap_err();
        assert!(err.contains("Not logged in"), "{err}");
    }

    /// 没装 codex 时硬报错（与第三方导入同一口径：校验是硬前提）。
    #[test]
    fn import_without_codex_refuses() {
        let home = super::super::tests::helpers::temp_dir("official-no-codex");
        let err = import_in(&home, &credential(), None).unwrap_err();
        assert!(err.contains("codex"), "{err}");
    }

    /// keyring / ephemeral 档位：codex 不读 auth.json，必须拒绝而不是写一份看不见的凭据。
    #[test]
    fn import_refuses_when_codex_reads_the_keyring() {
        let home = super::super::tests::helpers::temp_dir("official-keyring");
        fs::write(
            home.join("config.toml"),
            "cli_auth_credentials_store = \"keyring\"\n",
        )
        .unwrap();
        let err = import_in(&home, &credential(), Some(Path::new("/bin/true"))).unwrap_err();
        assert!(err.contains("keyring"), "{err}");
        assert!(!home.join(AUTH_FILE).exists());
    }
}
