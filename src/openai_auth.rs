//! apim 自己管理的 OpenAI Codex OAuth：浏览器授权、私有存储、用量查询。
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use rand::{RngCore, rngs::OsRng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const AUTH_URL: &str = "https://auth.openai.com/api/accounts/authorize";
/// Two endpoints are in use across OpenAI's own docs and the clients built on this flow:
/// the documented open-source path (`/api/accounts/oauth/token`) and the one the Codex CLI
/// lineage uses (`/oauth/token`, see Pi and cc-switch). Both exist, so try in order.
const TOKEN_URLS: [&str; 2] = [
    "https://auth.openai.com/api/accounts/oauth/token",
    "https://auth.openai.com/oauth/token",
];
const JWKS_URL: &str = "https://auth.openai.com/.well-known/jwks.json";
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CLIENT_BOOTSTRAP: &str = "dynamic_agent_client";
/// Codex CLI 系客户端的公开 client id（Pi 与 cc-switch 都用它），**缺省档位**用它。
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_ORIGINATOR: &str = "codex_cli_rs";
const REQUIRED_SCOPE: &str = "chatgpt.tokens.use.direct";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub client_id: String,
    pub host_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub scopes: Vec<String>,
    pub account_id: String,
    /// 该凭据由哪个档位签发：`true` = Codex CLI 系（Pi / cc-switch 同款），
    /// 申请 `openid profile email offline_access`（无 `resource`、无 `nonce`），
    /// 但 token 带 `chatgpt_account_id` 且能读用量接口。
    #[serde(default)]
    pub codex_family: bool,
}
#[derive(Debug, Clone)]
pub struct Usage {
    pub lines: Vec<String>,
}
/// 浏览器流程用哪套客户端注册。
///
/// - `codex`（**缺省**，取值 `codex` 或未设）：复刻 Codex CLI 系的公开 client id 与其参数，
///   与 Pi / cc-switch 完全一致（`localhost` 回调、不发 nonce、不申请 resource 与
///   `chatgpt.tokens.use.direct`）。**只有这条路的 token 带 `chatgpt_account_id`，
///   也才能读 Codex 用量接口**，所以它是缺省。
/// - `apim`（`APIM_OAUTH_CLIENT=apim|dynamic`）：OpenAI 文档里的开源动态注册，首次登录由
///   OpenAI 签发 `oaiapp_…`。它的 token 能登录、能调 API，但读不了 Codex 用量
///   （见 AGENTS.md 约定 11）。
struct ClientProfile {
    name: &'static str,
    /// `Some` = 固定 client id（不需要也不采纳动态注册）
    client_id: Option<&'static str>,
    redirect: &'static str,
    scope: &'static str,
    codex_family: bool,
}

fn profile_for(choice: Option<&str>) -> ClientProfile {
    match choice {
        Some("apim") | Some("dynamic") => ClientProfile {
            name: "apim",
            client_id: None,
            redirect: "http://127.0.0.1:1455/auth/callback",
            scope: "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
            codex_family: false,
        },
        // 缺省与无法识别的取值都走 Codex CLI 档位；无法识别的那个由
        // `unknown_profile_choice` 挑出来写进诊断日志，不静默。
        Some("codex") | None | Some(_) => ClientProfile {
            name: "codex",
            client_id: Some(CODEX_CLIENT_ID),
            redirect: "http://localhost:1455/auth/callback",
            scope: "openid profile email offline_access",
            codex_family: true,
        },
    }
}

/// `APIM_OAUTH_CLIENT` 里写错的那个值（`None` = 取值合法或缺省）。
/// 档位影响能不能读用量，写错字（大小写、拼错）必须看得见，不能静默退回缺省。
fn unknown_profile_choice(choice: Option<&str>) -> Option<&str> {
    match choice {
        None | Some("codex") | Some("apim") | Some("dynamic") => None,
        Some(other) => Some(other),
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join("openai-oauth.json")
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?)
}

pub fn load(dir: &Path) -> Result<Option<Credential>> {
    let p = path(dir);
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(
        serde_json::from_slice(&fs::read(&p).with_context(|| format!("读取 {}", p.display()))?)
            .context("解析 OpenAI OAuth 凭据")?,
    ))
}
fn save(dir: &Path, cred: &Credential) -> Result<()> {
    fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(cred)?;
    let _guard = write_lock();
    crate::config::write_private(&path(dir), &text)
}

/// 凭据/诊断日志的**进程内**写锁。
///
/// `config::write_private` 的 tmp 名只带进程号（跨进程唯一），所以同一进程里的两个写者
/// 会共用同一个 tmp 路径。这个功能引入了第二个写者（登录任务与探测/刷新任务都会写
/// `openai-oauth.json`），不加锁就可能交错写入、rename 出半截 JSON。
/// 锁**只包住写盘本身，绝不跨越 await**。
static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn write_lock() -> std::sync::MutexGuard<'static, ()> {
    // 写盘失败已经会让调用方报错；锁被毒化时再 panic 只会把错误变成崩溃
    WRITE_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}
fn random_b64(n: usize) -> String {
    let mut b = vec![0; n];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
fn host_id(dir: &Path) -> Result<String> {
    let p = dir.join("openai-oauth-host-id");
    if let Ok(s) = fs::read_to_string(&p) {
        return Ok(s.trim().to_owned());
    }
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let id = format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    );
    fs::create_dir_all(dir)?;
    crate::config::write_private(&p, &id)?;
    Ok(id)
}
fn scopes(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}

#[derive(Deserialize)]
struct IdClaims {
    iss: String,
    aud: serde_json::Value,
    sub: String,
    exp: usize,
    nonce: String,
    email: Option<String>,
}
/// Codex CLI 档位的 ID token 身份。
///
/// 它的授权请求**不发 nonce**（Pi / cc-switch 同），所以 token 里没有 `nonce` claim，
/// 用上面的 `IdClaims` 解它永远失败 —— 那会把 `sub`/`email` 静默丢掉，
/// 于 `apim auth openai status` 永远只能显示“账户已验证”、重新登录也发不出 `login_hint`。
/// 这里只取身份、不校验（该档位本来就同 Pi/cc-switch 不校验 ID token）。
#[derive(Deserialize)]
struct IdTokenIdentity {
    sub: String,
    #[serde(default)]
    email: Option<String>,
}
#[derive(Deserialize)]
struct AccessClaims {
    #[serde(default)]
    sub: Option<String>,
    /// access token 的到期时间（秒）。回写要用它判断「Codex 现场那份是不是更新」。
    #[serde(default)]
    exp: Option<u64>,
    #[serde(rename = "https://api.openai.com/auth")]
    auth: Option<AuthClaim>,
}
#[derive(Deserialize)]
struct AuthClaim {
    chatgpt_account_id: Option<String>,
}
#[derive(Deserialize)]
struct Jwks {
    keys: Vec<jsonwebtoken::jwk::Jwk>,
}

fn decode_unverified<T: for<'de> Deserialize<'de>>(token: &str) -> Result<T> {
    let p = token.split('.').nth(1).context("无效 JWT")?;
    Ok(serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p)?)?)
}
async fn validate_id_token(
    http: &Client,
    token: &str,
    client_id: &str,
    nonce: &str,
) -> Result<IdClaims> {
    let header = decode_header(token).context("解析 ID token header")?;
    if header.alg != Algorithm::RS256 {
        bail!("OpenAI ID token 签名算法不受支持");
    }
    let kid = header.kid.context("ID token 缺少 kid")?;
    let jwks: Jwks = http
        .get(JWKS_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let jwk = jwks
        .keys
        .iter()
        .find(|j| j.common.key_id.as_deref() == Some(&kid))
        .context("OpenAI JWKS 中找不到签名密钥")?;
    let key = DecodingKey::from_jwk(jwk).context("解析 OpenAI 签名密钥")?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&["https://auth.openai.com"]);
    validation.set_audience(&[client_id]);
    let c = decode::<IdClaims>(token, &key, &validation)
        .context("验证 OpenAI ID token")?
        .claims;
    let audience_ok = c.aud.as_str() == Some(client_id)
        || c.aud
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(client_id)));
    if c.iss != "https://auth.openai.com"
        || !audience_ok
        || c.nonce != nonce
        || c.exp as u64 <= now()
        || c.sub.is_empty()
    {
        bail!("ID token issuer/audience/nonce/有效期校验失败");
    }
    Ok(c)
}

/// 绑定回调端口。被占是登录最常见的失败，单独成函数是为了统一错误文案
/// （也是唯一一处把“端口 1455”写进用户可见文案的地方）。
async fn bind_callback_port() -> Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", 1455))
        .await
        .context("OAuth 回调端口 1455 被占用")
}

async fn callback(listener: TcpListener) -> Result<(String, String, Option<String>, String)> {
    let (mut stream, _) = listener.accept().await?;
    let mut b = vec![0; 16384];
    let n = stream.read(&mut b).await?;
    let request = std::str::from_utf8(&b[..n]).context("OAuth 回调非 UTF-8")?;
    let target = request
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .context("无效 OAuth 回调")?;
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))?;
    if url.path() != "/auth/callback" {
        bail!("OAuth 回调路径不匹配");
    }
    let q = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    if let Some(err) = q.get("error") {
        stream.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nAuthorization was not completed.").await?;
        bail!("OpenAI OAuth 拒绝或失败：{err}");
    }
    let code = q.get("code").context("OAuth 回调缺少 code")?.to_string();
    let state = q.get("state").context("OAuth 回调缺少 state")?.to_string();
    let id = q.get("client_id").map(|s| s.to_string());
    let scope = q.get("scope").map(|s| s.to_string()).unwrap_or_default();
    // 这页是**登录成功**页，不是 OpenAI 的报错页：会被误读成“授权被拒”，
    // 所以把“成功 / 可以关掉这个标签页”写清楚（真正的拒绝是 OpenAI 的
    // invalid_client 页面，根本不会回到这里）。
    let page = "授权完成 ✅\n\napim 已收到 OpenAI 的授权码，正在后台换取凭据。\n回到终端即可：OpenAI 的 AUTH 额度会在几秒内出现。\n这个标签页可以关掉了。\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    stream.write_all(response.as_bytes()).await?;
    Ok((code, state, id, scope))
}

/// 浏览器授权流程。成功后把凭据写进配置目录。
///
/// 失败时返回**原始错误链**（不带日志路径），由调用方决定怎么展示：
/// TUI 把它拼成一行 toast、CLI 打多行。每一步的细节（状态码、遮罩后的响应体）
/// 另外落在 `log_path()` 指的诊断日志里。
pub async fn login(dir: &Path) -> Result<()> {
    let mut log = AttemptLog::new();
    let outcome = login_inner(dir, &mut log).await;
    if let Err(err) = &outcome {
        log.fail(err);
    }
    log.save(dir);
    outcome
}

/// 记下一步失败再原样返回。
///
/// 登录链路（开端口 → 开浏览器 → 回调 → 换 token）每一段都可能断，
/// 而 TUI 只能给一行 toast，所以每一段都要在诊断日志里留下痕迹。
fn logging<T>(log: &mut AttemptLog, result: Result<T>) -> Result<T> {
    if let Err(err) = &result {
        log.fail(err);
    }
    result
}

async fn login_inner(dir: &Path, log: &mut AttemptLog) -> Result<()> {
    // 先落一步再开端口：端口被占是最常见的失败，而它在
    // 任何后续 step 之前就返回，日志不能只剩一行标题。
    let choice = std::env::var("APIM_OAUTH_CLIENT").ok();
    let p = profile_for(choice.as_deref());
    log.step(format!(
        "开始登录：profile={} 配置目录 {}",
        p.name,
        dir.display()
    ));
    if let Some(unknown) = unknown_profile_choice(choice.as_deref()) {
        log.step(format!(
            "APIM_OAUTH_CLIENT={unknown} 无法识别，按缺省 {name} 档位登录（可选 apim）",
            name = p.name
        ));
    }
    let previous = load(dir)?;
    let host = match &previous {
        Some(c) => c.host_id.clone(),
        None => host_id(dir)?,
    };
    // Only a client id OpenAI actually issued may be echoed back for reauthorization.
    // A credential that was merely copied in (or otherwise lacks an ID token hint)
    // must start a fresh dynamic registration, or the authorize request fails with
    // `invalid_client` / "This app is unavailable".
    let previous = previous.filter(reusable_registration);
    let listener = logging(log, bind_callback_port().await)?;
    let redirect = p.redirect;
    let state = random_b64(24);
    let nonce = random_b64(24);
    let verifier = random_b64(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = url::Url::parse(AUTH_URL)?;
    let registered_id = p
        .client_id
        .map(str::to_owned)
        .or_else(|| previous.as_ref().map(|c| c.client_id.clone()));
    // `id_token_hint` / `login_hint` 只在同一客户端下才发：把动态注册的 id token 发给
    // Codex CLI 档位（aud 不同）会被授权端点当成不匹配的提示。
    let previous = previous.filter(|old| Some(old.client_id.as_str()) == registered_id.as_deref());
    let requested_id = registered_id.as_deref().unwrap_or(CLIENT_BOOTSTRAP);
    let mut params: Vec<(&str, &str)> = vec![
        ("client_id", requested_id),
        ("response_type", "code"),
        ("redirect_uri", redirect),
        ("scope", p.scope),
        ("state", state.as_str()),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge.as_str()),
    ];
    if p.codex_family {
        // Pi 与 cc-switch 都按 Codex CLI 的形状发这几个参数（Pi 不发 nonce）。
        params.extend([
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("originator", CODEX_ORIGINATOR),
        ]);
    } else {
        params.extend([
            ("ext_agent_host_id", host.as_str()),
            ("resource", "https://api.openai.com/v1"),
            ("nonce", nonce.as_str()),
        ]);
    }
    for (k, v) in params {
        url.query_pairs_mut().append_pair(k, v);
    }
    if let Some(old) = &previous {
        if !old.id_token.is_empty() {
            url.query_pairs_mut()
                .append_pair("id_token_hint", &old.id_token);
        }
        if let Some(email) = &old.email {
            url.query_pairs_mut().append_pair("login_hint", email);
        }
    } else if !p.codex_family {
        url.query_pairs_mut().append_pair("agent_name_hint", "apim");
    }
    log.step(format!(
        "authorize: profile={} client_id={requested_id} redirect={redirect} 新注册={} 带 id_token_hint={}",
        p.name,
        registered_id.is_none(),
        previous.is_some()
    ));
    logging(
        log,
        crate::browser::open(url.as_str()).context("打开 OpenAI OAuth 登录页"),
    )?;
    let received = logging(
        log,
        tokio::time::timeout(std::time::Duration::from_secs(600), callback(listener))
            .await
            .map_err(|_| anyhow::anyhow!("OAuth 登录超时（10 分钟内没有回到回调）")),
    )?;
    let (code, got_state, returned_id, callback_scope) = logging(log, received)?;
    log.step(format!(
        "callback: 收到 code（{} 字符）, state 匹配={}, 回调 client_id={}, scope={}",
        code.len(),
        got_state == state,
        returned_id.as_deref().unwrap_or("（无）"),
        if callback_scope.is_empty() {
            "（无）".into()
        } else {
            callback_scope.clone()
        }
    ));
    if got_state != state {
        bail!("OAuth state 不匹配，拒绝此回调");
    }
    let client_id = if let Some(fixed) = p.client_id {
        // 固定 client id 的档位不需要（也不该）采纳回调带回的 id
        match returned_id.as_deref() {
            Some(id) if id != fixed => bail!("回调 client_id 与所用固定客户端不匹配"),
            _ => fixed.to_owned(),
        }
    } else {
        match (&previous, returned_id) {
            (None, Some(id)) => id,
            (None, None) => bail!(
                "动态注册回调没有带回 OpenAI 签发的 client_id（已记到 {}）",
                log_path(dir).display()
            ),
            (Some(old), Some(id)) if id != old.client_id => {
                bail!("回调 client_id 与已保存注册不匹配")
            }
            (Some(old), _) => old.client_id.clone(),
        }
    };
    if client_id.is_empty() || client_id == CLIENT_BOOTSTRAP {
        bail!("OpenAI 没有返回可持久化的 client_id");
    }
    let http = client()?;
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "authorization_code"),
        ("client_id", &client_id),
        ("code", &code),
        ("code_verifier", &verifier),
        ("redirect_uri", redirect),
    ];
    if !p.codex_family {
        form.push(("resource", "https://api.openai.com/v1"));
    }
    let token: TokenResponse = token_grant(&http, &form, "code 换 token", log).await?;
    let granted = if token.scope.is_empty() {
        callback_scope
    } else {
        token.scope.clone()
    };
    let scope_list = scopes(&granted);
    // Codex CLI 档位不申请 chatgpt.tokens.use.direct，但它的 token 能读用量接口
    // （cc-switch 与 Pi 都这么做，本机验证过），所以只对动态注册档位强校验。
    if !p.codex_family && !scope_list.iter().any(|s| s == REQUIRED_SCOPE) {
        bail!("未获 Codex 用量权限，请在浏览器授权时同意该权限后重试");
    }
    let session = Session::from(token);
    let claims = session
        .identity(&http, &client_id, &nonce, p.codex_family, log)
        .await;
    let cred = Credential {
        client_id,
        host_id: host,
        subject: claims.subject,
        email: claims.email,
        id_token: session.id_token.unwrap_or_default(),
        access_token: session.access_token,
        refresh_token: session.refresh_token.unwrap_or_default(),
        expires_at: now() + session.expires_in,
        scopes: scope_list,
        account_id: claims.account_id,
        codex_family: p.codex_family,
    };
    save(dir, &cred)?;
    Ok(())
}

/// 一次 token 端点调用的结果。
///
/// `id_token` / `refresh_token` 在两种合法响应里都可能缺失（身份类客户端不发
/// refresh token，账号类客户端不发 id token），所以都按可选处理；真正必需的
/// 只有 `access_token` 与 `expires_in`。
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    scope: String,
}

struct Session {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: u64,
}

impl Session {
    fn from(token: TokenResponse) -> Self {
        Self {
            access_token: token.access_token,
            refresh_token: token.refresh_token.filter(|s| !s.is_empty()),
            id_token: token.id_token.filter(|s| !s.is_empty()),
            expires_in: token.expires_in,
        }
    }

    /// 尽力而为地校验 ID token（注册路径的文档要求，Codex CLI 系客户端则完全不校验），
    /// 失败只记日志、不中断：access token 已经过 TLS + PKCE 从 OpenAI 端点取回，
    /// account id 也来自它本身。缺 ID token 时退回 access token 里的 `sub`。
    async fn identity(
        &self,
        http: &Client,
        client_id: &str,
        nonce: &str,
        codex_family: bool,
        log: &mut AttemptLog,
    ) -> Identity {
        let mut identity = Identity::from_access_token(&self.access_token, log);
        match &self.id_token {
            None => log.step("id token：响应里没有，跳过校验".to_string()),
            Some(id_token) if codex_family => {
                // Codex CLI 档位不带 nonce（Pi/cc-switch 不校验 ID token），只取身份
                match decode_unverified::<IdTokenIdentity>(id_token) {
                    Ok(identity_claims) => {
                        log.step(
                            "id token：Codex CLI 档位不校验，只取身份（同 Pi/cc-switch）"
                                .to_string(),
                        );
                        identity.subject = identity_claims.sub;
                        identity.email = identity_claims.email;
                    }
                    Err(err) => log.step(format!("id token：无法解析身份，已忽略（{err}）")),
                }
            }
            Some(id_token) => match validate_id_token(http, id_token, client_id, nonce).await {
                Ok(claims) => {
                    log.step(format!("id token：校验通过（sub={}）", claims.sub));
                    identity.subject = claims.sub;
                    identity.email = claims.email;
                }
                Err(err) => log.step(format!("id token：校验未通过，已忽略（{err}）")),
            },
        }
        identity
    }
}

struct Identity {
    subject: String,
    email: Option<String>,
    account_id: String,
}

impl Identity {
    fn from_access_token(access_token: &str, log: &mut AttemptLog) -> Self {
        let claims: AccessClaims = decode_unverified(access_token).unwrap_or(AccessClaims {
            sub: None,
            exp: None,
            auth: None,
        });
        let account_id = claims
            .auth
            .and_then(|a| a.chatgpt_account_id)
            .unwrap_or_default();
        if account_id.is_empty() {
            log.step("access token：没有 chatgpt_account_id claim".to_string());
        }
        Self {
            subject: claims.sub.unwrap_or_default(),
            email: None,
            account_id,
        }
    }
}

/// 轮换规则：服务端给了新的 refresh token 就用它，没给就沿用旧的。
///
/// RFC 6749 §6 允许刷新时**不**轮换，`TokenResponse` 的注释也把 `refresh_token`
/// 当可缺字段——在这里硬要求“必须给新的”会让凭据到期后直接死掉（只能重新登录）。
fn next_refresh_token(current: &str, rotated: Option<String>) -> String {
    rotated
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| current.to_owned())
}

/// 刷新请求的表单。
///
/// 与登录同一口径：只有动态注册档位带 `resource`（见 `login_inner` 的 token form）。
/// Pi 只发 grant_type/refresh_token/client_id，cc-switch 再带一个 scope，都不带 resource。
/// 独立成纯函数是为了把「档位门」钉在测试里：少给或多给 `resource` 会有测试变红（TODO-13）。
fn refresh_form(c: &Credential) -> Vec<(&str, &str)> {
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type", "refresh_token"),
        ("client_id", c.client_id.as_str()),
        ("refresh_token", c.refresh_token.as_str()),
    ];
    if !c.codex_family {
        form.push(("resource", "https://api.openai.com/v1"));
    }
    form
}

/// 从刷新后的 access token 推导 account id；`None` = 沿用旧值（两种情况都写进诊断日志）。
fn derive_account_from_access(access: &str, log: &mut AttemptLog) -> Option<String> {
    match decode_unverified::<AccessClaims>(access) {
        Ok(access) => match access.auth.and_then(|a| a.chatgpt_account_id) {
            Some(account) => Some(account),
            None => {
                log.step("刷新后的 access token 没有 chatgpt_account_id，沿用旧值".to_string());
                None
            }
        },
        Err(err) => {
            log.step(format!(
                "刷新后的 access token 无法解析（{err}），沿用旧 account id"
            ));
            None
        }
    }
}

/// 刷新拿到新 token 之后的落盘编排：**先把新 token 落盘**，再推导 account id（可能再写一次）。
///
/// POST 成功后服务端可能已经旋转了 refresh token，所以两步之间任何失败都不能让磁盘留着
/// 被顶替的那一份（否则下次刷新是 `invalid_grant`）。
/// `derive` 是注入点：生产路径用 `derive_account_from_access`，测试用它观察两次写盘之间的
/// 磁盘状态 —— 「先落盘再推导」这个顺序只有这样才钉得住（只断言最终结果区分不出两种顺序）。
fn persist_refreshed(
    dir: &Path,
    c: &mut Credential,
    log: &mut AttemptLog,
    derive: impl Fn(&str, &mut AttemptLog) -> Option<String>,
) -> Result<()> {
    save(dir, c)?;
    if let Some(account) = derive(&c.access_token, log) {
        c.account_id = account;
        save(dir, c)?;
    }
    Ok(())
}

/// POST **之后**的合并与落盘（注入点是响应而不是传输：测试不需要真实的 token 端点）。
fn refresh_from_response(
    dir: &Path,
    mut c: Credential,
    r: TokenResponse,
    log: &mut AttemptLog,
) -> Result<Credential> {
    c.access_token = r.access_token;
    c.expires_at = now() + r.expires_in;
    let rotated = r.refresh_token.filter(|s| !s.is_empty());
    if rotated.is_none() {
        log.step("刷新响应没有新的 refresh_token，沿用旧的".to_string());
    }
    c.refresh_token = next_refresh_token(&c.refresh_token, rotated);
    if let Some(id) = r.id_token.filter(|s| !s.is_empty()) {
        c.id_token = id
    }
    if !r.scope.is_empty() {
        c.scopes = scopes(&r.scope)
    }
    persist_refreshed(dir, &mut c, log, derive_account_from_access)?;
    Ok(c)
}

async fn refresh(dir: &Path, c: Credential, http: &Client) -> Result<Credential> {
    if c.expires_at > now() + 60 {
        return Ok(c);
    }
    let mut log = AttemptLog::new();
    let r = token_grant(http, &refresh_form(&c), "刷新 token", &mut log)
        .await
        .inspect_err(|_| log.save(dir))?;
    let c = refresh_from_response(dir, c, r, &mut log)?;
    log.save(dir);
    Ok(c)
}

/// 从 Codex 现场读回的 token 快照（由 `clients/codex` 解析 `auth.json` 得到）。
///
/// 单独立一个类型是刻意的：凭据模块**不认** codex 的 JSON 形状（那是客户端适配层的事），
/// 两边只经这个结构交接（分工见 `ADR-0002`）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // 接线在票 04；在那之前只有测试在用
pub struct ExternTokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
}

/// 回写判定（`TODO-2`）。**判不了就拒绝**（fail closed）：最坏的情况是「这次没同步」，
/// 绝不能是「把别人的凭据抄进 apim」。
#[derive(Debug)]
#[allow(dead_code)] // 接线在票 04；在那之前只有测试在用
pub enum AdoptDecision {
    /// 不是 apim 写进去的那份 / 凭据不全 / 读不出有效期：一个字节都不写
    Reject(&'static str),
    /// 是自己那份，但没有更新的东西：不写盘（幂等，不会来回抖）
    Skip(&'static str),
    /// 采纳：这是回写之后的凭据
    Adopt(Box<Credential>),
}

/// 采纳判定（**纯函数**，不做任何 IO）。
///
/// - **ownership 是硬门**：`theirs.refresh_token` 必须与 `ours` 逐字符相同。用户自己
///   `codex login` 的另一个账号刷新过的 token 绝不能抄进 apim。
/// - **只在确实更新时才采纳**：`theirs_exp`（Codex 现场那份 access token 的有效期）必须
///   晚于 `ours.expires_at`，否则不写盘。
/// - **只采纳 token 与有效期**：`client_id` / `host_id` / `subject` / `email` / `scopes` /
///   `codex_family` 是 apim 自己的注册身份，一律保留；`account_id` 按既有刷新口径从新
///   access token 推导，推导不出就沿用旧值。
#[allow(dead_code)] // 接线在票 04；在那之前只有测试在用
pub fn adopt_plan(
    ours: &Credential,
    theirs: &ExternTokens,
    theirs_exp: Option<u64>,
) -> AdoptDecision {
    if theirs.access_token.trim().is_empty()
        || theirs.refresh_token.trim().is_empty()
        || theirs.id_token.trim().is_empty()
    {
        return AdoptDecision::Reject("Codex 现场那份凭据不完整");
    }
    if theirs.refresh_token != ours.refresh_token {
        return AdoptDecision::Reject("refresh token 不吻合：那不是 apim 写进去的那份凭据");
    }
    let Some(exp) = theirs_exp else {
        return AdoptDecision::Reject("Codex 现场那份 access token 读不出有效期");
    };
    if exp <= ours.expires_at {
        return AdoptDecision::Skip("Codex 现场那份 token 不比 apim 的新");
    }
    let mut next = ours.clone();
    next.access_token = theirs.access_token.clone();
    next.refresh_token = theirs.refresh_token.clone();
    next.id_token = theirs.id_token.clone();
    next.expires_at = exp;
    if let Ok(access) = decode_unverified::<AccessClaims>(&next.access_token)
        && let Some(account) = access.auth.and_then(|a| a.chatgpt_account_id)
    {
        next.account_id = account;
    }
    AdoptDecision::Adopt(Box::new(next))
}

/// `adopt_plan` 的正式入口：有效期由 Codex 现场那份 access token 自己给出。
/// 拆成两个函数是为了让「有效期读不出 → 拒绝」这一行能在判定表里单独钉住。
#[allow(dead_code)] // 接线在票 04；在那之前只有测试在用
pub fn plan_adoption(ours: &Credential, theirs: &ExternTokens) -> AdoptDecision {
    adopt_plan(ours, theirs, access_token_exp(&theirs.access_token))
}

/// access token 里的 `exp`（不是 JWT / 没有该 claim → `None`，调用方据此拒绝）。
#[allow(dead_code)] // 接线在票 04；在那之前只有测试在用
pub fn access_token_exp(access: &str) -> Option<u64> {
    decode_unverified::<AccessClaims>(access).ok()?.exp
}

/// POST 一次 token 端点请求，并按顺序试两个已知端点（见 `TOKEN_URLS`）。
/// 失败时把**状态码与响应体片段**（截断、剔掉 token 字段）带进错误信息，
/// 否则这类未公开接口出错时只能看到一个没头没尾的 `invalid_client`。
async fn token_grant(
    http: &Client,
    form: &[(&str, &str)],
    step: &str,
    log: &mut AttemptLog,
) -> Result<TokenResponse> {
    let mut failure = String::new();
    for url in TOKEN_URLS {
        let response = match http.post(url).form(form).send().await {
            Ok(response) => response,
            Err(err) => {
                failure = format!("{step}：请求 {url} 失败（{err}）");
                log.step(failure.clone());
                continue;
            }
        };
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            log.step(format!(
                "{step}：{url} → HTTP {}，响应 {} 字符",
                status.as_u16(),
                body.len()
            ));
            return serde_json::from_str(&body)
                .with_context(|| format!("{step}：响应不是预期 JSON（{url}）"));
        }
        // 失败的那条要把遮罩后的响应体也写进日志：文档（AGENTS.md / README）承诺的
        // 就是“状态码 + 遮罩后的响应体”，只写个状态码在排障时等于没说。
        failure = format!(
            "{step}：{url} 返回 HTTP {}：{}",
            status.as_u16(),
            redact(&body)
        );
        log.step(failure.clone());
        if !matches!(status.as_u16(), 400 | 401 | 403 | 404) {
            break;
        }
    }
    bail!("{failure}")
}

/// 响应体进错误信息/日志前先截断，并遮掉长得像凭据的串。
///
/// 不按 JSON 字段名遮：错误响应未必是 JSON，token / authorization code 总是
/// 长串（JWT、`sk-…`、随机串），按「长 alphanumeric 连续段」遮就够了。
fn redact(body: &str) -> String {
    const SECRET_LEN: usize = 40;
    let mut out = String::with_capacity(body.len().min(400));
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() >= SECRET_LEN {
            out.push_str("[已隐藏]");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for ch in body.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            run.push(ch);
        } else {
            flush(&mut run, &mut out);
            out.push(ch);
        }
    }
    flush(&mut run, &mut out);
    out.chars().take(300).collect()
}

/// 一次登录/刷新尝试的步骤记录（状态码 + 遮罩后的响应体）。
///
/// 失败时 TUI 只能给一行 toast，而这条链路（开端口 → 浏览器 → 回调 → token 端点 →
/// ID token 校验）每一步都可能断，所以把步骤落到磁盘供事后诊断。
/// **不含任何 token**：响应体统一过 `redact`（长串遮成 `[已隐藏]`），
/// 授权码只记长度。
pub struct AttemptLog {
    steps: Vec<String>,
    /// 已经记过失败了（避免站点与收尾处各写一遍）。
    failed: bool,
}

impl AttemptLog {
    fn new() -> Self {
        Self {
            steps: Vec::new(),
            failed: false,
        }
    }

    /// 只收集，不往终端写：这个方法会在 TUI 的 tokio 任务里被调用，
    /// 而那时 ratatui 正占着备用屏（往 stderr 写会乱屏或被 diff 掉）。
    fn step(&mut self, step: String) {
        self.steps.push(step);
    }

    /// 记一条失败（带完整错误链），每个尝试只记第一条。
    fn fail(&mut self, err: &anyhow::Error) {
        if self.failed {
            return;
        }
        self.failed = true;
        self.step(format!("失败：{err:#}"));
    }

    fn save(&mut self, dir: &Path) {
        let text = self.render();
        let _guard = write_lock();
        // 日志只是诊断辅助：写不进去也不影响登录本身
        let _ = crate::config::write_private(&log_path(dir), &text);
    }

    /// 日志正文（独立成函数：测试不用真去碰磁盘）。
    fn render(&self) -> String {
        let mut text = "apim OpenAI OAuth 登录诊断\n".to_string();
        for step in &self.steps {
            text.push_str(step);
            text.push('\n');
        }
        text
    }
}

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("openai-oauth.log")
}

/// 凭据文件路径（CLI 的成功提示用，避免再次手写文件名）。
pub fn credential_path(dir: &Path) -> PathBuf {
    path(dir)
}
#[derive(Deserialize)]
struct Window {
    used_percent: Option<f64>,
    limit_window_seconds: Option<u64>,
    reset_at: Option<u64>,
}
#[derive(Deserialize)]
struct Rate {
    primary_window: Option<Window>,
    secondary_window: Option<Window>,
}
#[derive(Deserialize)]
struct Credits {
    balance: Option<f64>,
    unlimited: Option<bool>,
}
#[derive(Deserialize)]
struct UsageJson {
    plan_type: Option<String>,
    rate_limit: Option<Rate>,
    credits: Option<Credits>,
}
/// 进程内序列化整个用量查询（含可能发生的 token 刷新）。
///
/// TUI 里两个调用者（定时探测与 `--snapshot` 的阻塞刷新）会同时打同一份凭据；
/// 若两边都落后于 60s 余量，它们会各自用同一份 refresh token 发刷新请求，
/// 后到的那个拿到 `invalid_grant`，还在诊断日志里留下一条看起来像真故障的记录。
/// 拿锁后再 `load`：后到的那个会读到刚刷新的凭据，直接跳过刷新。
static USAGE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn fetch_usage(dir: &Path) -> Result<Option<Usage>> {
    let _guard = USAGE_LOCK.lock().await;
    fetch_usage_locked(dir).await
}

async fn fetch_usage_locked(dir: &Path) -> Result<Option<Usage>> {
    let Some(c) = load(dir)? else { return Ok(None) };
    let http = client()?;
    let c = refresh(dir, c, &http).await?;
    if !c.codex_family && !c.scopes.iter().any(|s| s == REQUIRED_SCOPE) {
        bail!("OAuth 凭据未获 Codex 用量权限，请重新登录")
    }
    if c.account_id.is_empty() {
        bail!("该凭据里没有 ChatGPT account id（动态注册档位读不了 Codex 用量），按 o 重新登录")
    }
    let j: UsageJson = http
        .get(USAGE_URL)
        .bearer_auth(&c.access_token)
        .header("ChatGPT-Account-Id", &c.account_id)
        .header(reqwest::header::USER_AGENT, "codex-cli")
        .send()
        .await?
        .error_for_status()
        .context("Codex 用量请求失败（可能需要重新授权）")?
        .json()
        .await
        .context("解析 Codex 用量响应失败")?;
    Ok(Some(Usage {
        lines: usage_lines(j)?,
    }))
}

/// Turn a usage response into the lines the balance pane shows.
/// Every window the API returned gets a line; the count is whatever the plan has.
fn usage_lines(j: UsageJson) -> Result<Vec<String>> {
    let mut windows: Vec<Window> = j
        .rate_limit
        .into_iter()
        .flat_map(|r| [r.primary_window, r.secondary_window])
        .flatten()
        .filter(|w| w.used_percent.is_some())
        .collect();
    windows.sort_by_key(|w| w.reset_at.unwrap_or(u64::MAX));
    if windows.is_empty() {
        bail!("Codex 用量响应中没有限额窗口")
    }
    let mut lines = Vec::new();
    for w in windows {
        let used = w.used_percent.unwrap_or_default().clamp(0.0, 100.0);
        let label = window_label(w.limit_window_seconds);
        let reset = w
            .reset_at
            .map(|at| format!(" · {} 后重置", reset_countdown(at.saturating_sub(now()))))
            .unwrap_or_default();
        lines.push(format!(
            "Codex {label}：已用 {:.0}% · 剩余 {:.0}%{reset}",
            used,
            100.0 - used
        ));
    }
    if let Some(x) = j.credits {
        if x.unlimited == Some(true) {
            lines.push("Codex Credits：无限".into())
        } else if let Some(b) = x.balance {
            lines.push(format!("Codex Credits：{b}"))
        }
    }
    if let Some(p) = j.plan_type {
        lines.push(format!("套餐：{p}"))
    }
    Ok(lines)
}
/// Human label for a rate-limit window, derived from the length the API actually returned.
/// **Do not assume a 5h + 7d pair**: the endpoint returns whichever windows the plan has
/// (a `go` account, for example, reports a single 30-day window and `secondary_window: null`),
/// so label whatever comes back instead of inventing periods.
fn window_label(seconds: Option<u64>) -> String {
    let seconds = seconds.unwrap_or_default();
    if seconds == 0 {
        return "Quota".into();
    }
    if seconds <= 6 * 3_600 {
        "5h".into()
    } else if seconds <= 8 * 86_400 {
        "1w".into()
    } else if seconds <= 35 * 86_400 {
        "1mo".into()
    } else {
        format!("{}d", (seconds as f64 / 86_400.0).round() as u64)
    }
}

/// Whether a stored credential may be reused as the OAuth client for reauthorization.
/// OpenAI issues `oaiapp_…` client ids during dynamic registration and returns an ID token
/// we can send back as `id_token_hint`; neither exists for a credential that was only
/// copied in for usage display.
fn reusable_registration(c: &Credential) -> bool {
    c.client_id.starts_with("oaiapp_") && !c.id_token.is_empty()
}

fn reset_countdown(seconds: u64) -> String {
    if seconds == 0 {
        return "即将".into();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600).div_ceil(60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

pub fn remove(dir: &Path) -> Result<()> {
    let p = path(dir);
    if p.exists() {
        fs::remove_file(&p).with_context(|| format!("删除 {}", p.display()))?
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(name: &str) -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("oauth-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn credential() -> Credential {
        Credential {
            client_id: "oaiapp-test".into(),
            host_id: "urn:uuid:12345678-1234-4234-8234-123456789abc".into(),
            subject: "sub".into(),
            email: None,
            id_token: "id".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: 1_900_000_000,
            scopes: vec![REQUIRED_SCOPE.into()],
            account_id: "acct".into(),
            codex_family: true,
        }
    }
    #[test]
    fn credential_roundtrip_and_remove() {
        let p = dir("store");
        let c = credential();
        save(&p, &c).unwrap();
        assert_eq!(load(&p).unwrap().unwrap().refresh_token, c.refresh_token);
        remove(&p).unwrap();
        remove(&p).unwrap();
        assert!(load(&p).unwrap().is_none());
        let _ = fs::remove_dir_all(p);
    }
    #[cfg(unix)]
    #[test]
    fn credential_file_mode_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let p = dir("mode");
        save(&p, &credential()).unwrap();
        assert_eq!(
            fs::metadata(path(&p)).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(p);
    }
    #[tokio::test]
    async fn missing_credential_skips_network_usage_request() {
        let p = dir("missing");
        assert!(fetch_usage(&p).await.unwrap().is_none());
        let _ = fs::remove_dir_all(p);
    }
    /// 复制进来的凭据只能查用量：`client_id` 不是 OpenAI 签发过的，就不能再当注册用
    /// （按 `o` 会丢弃它重走所选档位；动态注册档位把它发出去会报 `invalid_client`）。
    #[test]
    fn only_a_real_openai_registration_is_reused_for_sign_in() {
        let mut c = credential();
        c.client_id = "oaiapp_real".into();
        assert!(reusable_registration(&c));
        c.client_id = "pi-imported-temporary".into();
        assert!(!reusable_registration(&c));
        c.client_id = "oaiapp_real".into();
        c.id_token.clear();
        assert!(
            !reusable_registration(&c),
            "没有 ID token 就没有可用的 id_token_hint"
        );
    }

    /// 真实响应形状：`go` 账号只回一个 30 天窗口、`secondary_window: null`，
    /// 这时面板就该只显示一条 `1mo`，而不是硬凑出 5h/7d。
    #[test]
    fn single_monthly_window_renders_one_labelled_line() {
        let j: UsageJson = serde_json::from_str(
            r#"{
                "plan_type": "go",
                "rate_limit": {
                    "primary_window": {"used_percent": 27, "limit_window_seconds": 2592000, "reset_at": 1793784696},
                    "secondary_window": null
                },
                "credits": {"has_credits": false, "unlimited": false, "balance": null}
            }"#,
        )
        .unwrap();
        let lines = usage_lines(j).unwrap();
        assert_eq!(lines.len(), 2, "一个窗口 + 套餐行：{lines:?}");
        assert!(
            lines[0].starts_with("Codex 1mo：已用 27% · 剩余 73%"),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("后重置"), "{}", lines[0]);
        assert_eq!(lines[1], "套餐：go");
    }

    /// 返回两个窗口时两条都显示（pro 这类账号的 5h + 1w）。
    #[test]
    fn two_windows_are_labelled_five_hours_and_one_week() {
        let j: UsageJson = serde_json::from_str(
            r#"{
                "plan_type": "pro",
                "rate_limit": {
                    "primary_window": {"used_percent": 15, "limit_window_seconds": 18000, "reset_at": 1793784696},
                    "secondary_window": {"used_percent": 5, "limit_window_seconds": 604800, "reset_at": 1794389496}
                }
            }"#,
        )
        .unwrap();
        let lines = usage_lines(j).unwrap();
        assert!(lines[0].starts_with("Codex 5h："), "{}", lines[0]);
        assert!(lines[1].starts_with("Codex 1w："), "{}", lines[1]);
    }

    /// 窗口标签按接口真正返回的长度算，不假设一定有 5h + 7d 两个窗口。
    #[test]
    fn window_label_follows_the_returned_length() {
        assert_eq!(window_label(Some(18_000)), "5h");
        assert_eq!(window_label(Some(604_800)), "1w");
        assert_eq!(window_label(Some(2_592_000)), "1mo");
        assert_eq!(window_label(Some(0)), "Quota");
        assert_eq!(window_label(None), "Quota");
    }

    /// 两个档位的参数差异就是这两个项目的实现差异：Pi / cc-switch 走 Codex CLI 的公开 client id。
    #[test]
    fn profiles_match_the_reference_clients() {
        // 缺省必须是与 Pi/cc-switch 一致的档位：只有它的 token 能读 Codex 用量
        let codex = profile_for(None);
        assert_eq!(codex.client_id, Some(CODEX_CLIENT_ID));
        assert_eq!(codex.redirect, "http://localhost:1455/auth/callback");
        assert_eq!(codex.scope, "openid profile email offline_access");
        assert!(codex.codex_family);

        let apim = profile_for(Some("apim"));
        assert!(apim.client_id.is_none(), "动态注册档位不需要固定 client id");
        assert!(
            apim.redirect.contains("127.0.0.1"),
            "文档要求不许用 localhost"
        );
        assert!(apim.scope.contains(REQUIRED_SCOPE));
        assert_eq!(profile_for(Some("dynamic")).client_id, None);
        assert_eq!(profile_for(Some("codex")).client_id, Some(CODEX_CLIENT_ID));

        // 写错字不能静默：仍按缺省档位走，但取值会被点名（登录时写进诊断日志）
        assert!(unknown_profile_choice(None).is_none());
        assert!(unknown_profile_choice(Some("codex")).is_none());
        assert!(unknown_profile_choice(Some("apim")).is_none());
        assert_eq!(unknown_profile_choice(Some("Codex")), Some("Codex"));
        assert!(profile_for(Some("apar")).codex_family);
    }

    /// 错误信息/日志里不能带 token，哪怕响应体贴了完整 token JSON。
    #[test]
    fn redact_strips_token_like_values_and_truncates() {
        let token = "a1b2c3d4e5".repeat(6); // 60 字符，典型 JWT / sk- 长度
        let body = format!(r#"{{"error":"invalid_grant","access_token":"{token}"}}"#);
        let masked = redact(&body);
        assert!(!masked.contains(&token), "{masked}");
        assert!(masked.contains("invalid_grant"), "{masked}");
        assert!(redact(&"x".repeat(5_000)).chars().count() <= 300);
        assert_eq!(redact("短消息不能被动"), "短消息不能被动");
    }

    #[test]
    fn reset_countdown_formats_hours_and_days() {
        assert_eq!(reset_countdown(3600 + 42 * 60), "1h 42m");
        assert_eq!(reset_countdown(3 * 86400 + 8 * 3600), "3d 8h");
        assert_eq!(reset_countdown(0), "即将");
    }

    /// 刷新时的轮换规则：没给新 token 就沿用旧的（RFC 6749 §6 允许不轮换），
    /// 不能把“响应缺 refresh_token”当成致命错误。
    #[test]
    fn refresh_keeps_the_old_token_when_the_server_does_not_rotate() {
        assert_eq!(next_refresh_token("old", None), "old");
        assert_eq!(next_refresh_token("old", Some(String::new())), "old");
        assert_eq!(next_refresh_token("old", Some("new".into())), "new");
    }

    /// 余量内不刷新：直接返回旧凭据，不发请求也不写盘。
    #[tokio::test]
    async fn refresh_returns_early_for_an_unexpired_credential() {
        let p = dir("unexpired");
        let mut c = credential();
        c.expires_at = now() + 3_600;
        let http = client().unwrap();
        let out = refresh(&p, c.clone(), &http).await.unwrap();
        assert_eq!(out.access_token, c.access_token);
        assert_eq!(out.refresh_token, c.refresh_token);
        assert!(
            load(&p).unwrap().is_none(),
            "提前返回不应写盘（目录里本没有凭据）"
        );
        let _ = fs::remove_dir_all(p);
    }

    /// 登录失败要先落进诊断日志（带错误链），且只记第一条：
    /// 用户看到的 toast 只有一行，日志是唯一能说清楚的地方。
    #[test]
    fn attempt_log_records_the_cause_once() {
        let mut log = AttemptLog::new();
        log.step("开始登录：profile=codex".to_string());
        log.fail(&anyhow::anyhow!("OAuth 回调端口 1455 被占用"));
        log.fail(&anyhow::anyhow!("外层又包装了一次"));
        let text = log.render();
        assert_eq!(
            text.lines().filter(|l| l.starts_with("失败：")).count(),
            1,
            "{text}"
        );
        assert!(text.contains("端口 1455 被占用"), "{text}");
        assert!(text.contains("开始登录"), "{text}");
    }

    /// Codex CLI 档位的 ID token 里没有 `nonce` claim（授权请求就没发）。
    /// 用要求 nonce 的 `IdClaims` 解它永远失败，`sub`/`email` 会被静默丢掉 ——
    /// `apim auth openai status` 因此永远显示“账户已验证”。
    #[test]
    fn codex_id_token_identity_is_read_without_a_nonce_claim() {
        let payload = serde_json::json!({"sub": "user-1", "email": "user@example.invalid"});
        let token = format!(
            "h.{}.s",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap())
        );
        let identity: IdTokenIdentity = decode_unverified(&token).unwrap();
        assert_eq!(identity.sub, "user-1");
        assert_eq!(identity.email.as_deref(), Some("user@example.invalid"));
        assert!(
            decode_unverified::<IdClaims>(&token).is_err(),
            "同一份 token 用要求 nonce 的结构就解不出来：这正是之前丢字段的原因"
        );
    }

    #[test]
    fn parses_account_id_from_access_jwt() {
        let body = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(
                &serde_json::json!({"https://api.openai.com/auth":{"chatgpt_account_id":"acct"}}),
            )
            .unwrap(),
        );
        let c: AccessClaims = decode_unverified(&format!("h.{body}.s")).unwrap();
        assert_eq!(c.auth.unwrap().chatgpt_account_id.as_deref(), Some("acct"));
    }

    /// 档位门（TODO-13）：只有动态注册档位带 `resource`。这条规则写在注释里很久了，
    /// 但改回去不会有测试变红 —— 现在会了。
    #[test]
    fn refresh_form_follows_the_profile() {
        // Codex CLI 档位（Pi / cc-switch 同款）：只有三个字段
        let mut codex = credential();
        codex.client_id = "app_EMoamEEZ73f0CkXaXp7hrann".into();
        codex.refresh_token = "rt-codex".into();
        codex.codex_family = true;
        assert_eq!(
            refresh_form(&codex),
            vec![
                ("grant_type", "refresh_token"),
                ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
                ("refresh_token", "rt-codex"),
            ]
        );

        // 动态注册档位：多一个 `resource`，一个不多一个不少
        let mut dynamic = credential();
        dynamic.client_id = "oaiapp-real".into();
        dynamic.refresh_token = "rt-dynamic".into();
        dynamic.codex_family = false;
        let form = refresh_form(&dynamic);
        assert_eq!(form.len(), 4, "{form:?}");
        assert_eq!(form[0], ("grant_type", "refresh_token"));
        assert_eq!(form[1], ("client_id", "oaiapp-real"));
        assert_eq!(form[2], ("refresh_token", "rt-dynamic"));
        assert_eq!(form[3], ("resource", "https://api.openai.com/v1"));
    }

    /// POST 之后：新 token 必须**先落盘**，account id 是第二步。
    /// 两种顺序的**最终**磁盘状态一模一样，所以只能在两次写盘之间观察 —— 这就是
    /// `persist_refreshed` 的 `derive` 会成为注入点的原因。
    #[test]
    fn refreshed_tokens_hit_the_disk_before_the_account_id_is_derived() {
        let p = dir("refresh-order");
        let mut c = credential();
        c.access_token = "new.access.token".into();
        c.refresh_token = "rotated-refresh".into();
        c.account_id = "acct-old".into();

        let between = std::cell::RefCell::new(None);
        let probe = p.clone();
        let mut log = AttemptLog::new();
        persist_refreshed(&p, &mut c, &mut log, |_, _| {
            *between.borrow_mut() = load(&probe).unwrap();
            Some("acct-new".into())
        })
        .unwrap();

        let between = between.into_inner().expect("第一步就必须已经写过一次盘");
        assert_eq!(
            between.access_token, "new.access.token",
            "新 access token 要在推导 account id 之前就落盘"
        );
        assert_eq!(between.refresh_token, "rotated-refresh");
        assert_eq!(
            between.account_id, "acct-old",
            "这一步还没有推导 account id"
        );

        let landed = load(&p).unwrap().unwrap();
        assert_eq!(landed.account_id, "acct-new", "第二步才补上推导结果");
        assert_eq!(landed.access_token, "new.access.token");
        let _ = fs::remove_dir_all(p);
    }

    /// 推导不出 account id 时只写一次盘：新 token 已经在盘上，account id 沿用旧值。
    /// （「下面任何失败都不能让磁盘留着被顶替的那一份」的另一半。）
    #[test]
    fn refreshed_tokens_stay_on_disk_when_the_account_id_cannot_be_derived() {
        let p = dir("refresh-no-account");
        let mut c = credential();
        c.access_token = "new.access.token".into();
        c.refresh_token = "rotated-refresh".into();
        c.account_id = "acct-old".into();

        let mut log = AttemptLog::new();
        persist_refreshed(&p, &mut c, &mut log, |_, log| {
            log.step("推导不出 account id".to_string());
            None
        })
        .unwrap();

        let on_disk = load(&p).unwrap().unwrap();
        assert_eq!(on_disk.access_token, "new.access.token");
        assert_eq!(on_disk.refresh_token, "rotated-refresh");
        assert_eq!(on_disk.account_id, "acct-old");
        assert_eq!(c.account_id, "acct-old", "内存里也不该被改");
        let _ = fs::remove_dir_all(p);
    }

    /// `refresh_from_response` 的合并规则：不轮换时沿用旧 refresh token / id_token / scope，
    /// 轮换与带新值时全部采纳；`expires_at` 按响应里的 `expires_in` 推算。
    #[test]
    fn refresh_response_merges_tokens_and_keeps_the_old_refresh_token_when_not_rotated() {
        let p = dir("refresh-merge");
        let mut c = credential();
        c.access_token = "old".into();
        c.refresh_token = "rt-old".into();
        c.id_token = "id-old".into();
        c.scopes = vec!["scope-old".into()];
        c.account_id = "acct-old".into();

        let mut log = AttemptLog::new();
        let out = refresh_from_response(
            &p,
            c,
            TokenResponse {
                access_token: "new".into(),
                expires_in: 3_600,
                refresh_token: None,
                id_token: None,
                scope: String::new(),
            },
            &mut log,
        )
        .unwrap();
        assert_eq!(out.access_token, "new");
        assert_eq!(out.refresh_token, "rt-old", "没轮换就沿用旧的");
        assert_eq!(out.id_token, "id-old");
        assert_eq!(out.scopes, vec!["scope-old".to_string()]);
        assert_eq!(out.account_id, "acct-old", "非 JWT 推导不出来，沿用旧值");
        assert!(
            out.expires_at > now() + 3_000,
            "expires_at 按 expires_in 推"
        );
        assert!(log.render().contains("沿用旧的"), "{}", log.render());
        assert_eq!(load(&p).unwrap().unwrap().access_token, "new");

        // 轮换 + 新 id_token + 新 scope：全部采纳
        let mut log = AttemptLog::new();
        let out = refresh_from_response(
            &p,
            out,
            TokenResponse {
                access_token: "newer".into(),
                expires_in: 60,
                refresh_token: Some("rt-new".into()),
                id_token: Some("id-new".into()),
                scope: "scope-a scope-b".into(),
            },
            &mut log,
        )
        .unwrap();
        assert_eq!(out.refresh_token, "rt-new");
        assert_eq!(out.id_token, "id-new");
        assert_eq!(
            out.scopes,
            vec!["scope-a".to_string(), "scope-b".to_string()]
        );
        assert_eq!(load(&p).unwrap().unwrap().refresh_token, "rt-new");
        let _ = fs::remove_dir_all(p);
    }

    /// 合成一个不带签名的 JWT（本仓既有写法：只关心 payload）。
    fn jwt_with(claims: serde_json::Value) -> String {
        format!(
            "h.{}.s",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    /// 现场快照的构造（ownership 用的 refresh token 与 access token 单独给）。
    fn extern_tokens(refresh_token: &str, access_token: &str) -> ExternTokens {
        ExternTokens {
            id_token: "id".into(),
            access_token: access_token.into(),
            refresh_token: refresh_token.into(),
        }
    }

    /// 回写判定表（`TODO-2`）：ownership 是硬门、只在确实更新时才采纳、判不了一律拒绝。
    #[test]
    fn adoption_requires_ownership_and_a_newer_token() {
        let ours = credential();
        let newer = ours.expires_at + 3_600;
        let access = jwt_with(serde_json::json!({
            "https://api.openai.com/auth": {"chatgpt_account_id": "acct-new"}
        }));

        // ① 另一个账号（用户自己 codex login 的那份）：refresh token 不吻合 → 拒绝
        let theirs = ExternTokens {
            refresh_token: "someone-elses".into(),
            ..extern_tokens("refresh", &access)
        };
        let rejected = adopt_plan(&ours, &theirs, Some(newer));
        assert!(
            matches!(&rejected, AdoptDecision::Reject(reason) if reason.contains("refresh token")),
            "{rejected:?}"
        );

        // ② 凭据不全 → 拒绝（三个 token 都得在）
        for theirs in [
            ExternTokens {
                id_token: String::new(),
                ..extern_tokens("refresh", "a.b.c")
            },
            ExternTokens {
                access_token: String::new(),
                ..extern_tokens("refresh", "a.b.c")
            },
            ExternTokens {
                refresh_token: String::new(),
                ..extern_tokens("refresh", "a.b.c")
            },
        ] {
            assert!(
                matches!(
                    adopt_plan(&ours, &theirs, Some(newer)),
                    AdoptDecision::Reject(_)
                ),
                "{theirs:?}"
            );
        }

        // ③ 读不出有效期 → 拒绝（绝不替它编一个）
        let theirs = extern_tokens("refresh", &access);
        assert!(
            matches!(adopt_plan(&ours, &theirs, None), AdoptDecision::Reject(reason) if reason.contains("有效期"))
        );

        // ④ 不比 apim 的新（相同或更早）→ 跳过，不写盘
        for exp in [ours.expires_at, ours.expires_at - 1] {
            assert!(
                matches!(
                    adopt_plan(&ours, &theirs, Some(exp)),
                    AdoptDecision::Skip(_)
                ),
                "exp={exp}"
            );
        }

        // ⑤ 同账号 + 确实更新 → 采纳；只动 token 与有效期
        let AdoptDecision::Adopt(next) = adopt_plan(&ours, &theirs, Some(newer)) else {
            panic!("同账号且更新时应当采纳");
        };
        assert_eq!(next.access_token, access);
        assert_eq!(next.expires_at, newer);
        assert_eq!(
            next.account_id, "acct-new",
            "account id 从新 access token 推导"
        );
        assert_eq!(next.refresh_token, "refresh");
        assert_eq!(next.id_token, "id");
        // 身份字段一律保留 apim 自己那份（它们是 apim 的注册身份，auth.json 里也没有）
        assert_eq!(next.client_id, ours.client_id);
        assert_eq!(next.host_id, ours.host_id);
        assert_eq!(next.subject, ours.subject);
        assert_eq!(next.email, ours.email);
        assert_eq!(next.scopes, ours.scopes);
        assert_eq!(next.codex_family, ours.codex_family);

        // ⑥ 是合法 JWT 但没有 account id claim → 采纳，沿用旧 account id
        let no_account = jwt_with(serde_json::json!({"exp": newer, "sub": "u"}));
        let theirs = extern_tokens("refresh", &no_account);
        let AdoptDecision::Adopt(next) = adopt_plan(&ours, &theirs, Some(newer)) else {
            panic!("token 本身有效就该采纳，account id 推导不出只影响那一个字段");
        };
        assert_eq!(next.account_id, ours.account_id);
        assert_eq!(next.access_token, no_account);
    }

    /// 正式入口：有效期由现场那份 access token 自己给出；读不出就拒绝。
    #[test]
    fn adoption_entry_point_reads_the_expiry_from_the_token_itself() {
        let ours = credential();
        let exp = ours.expires_at + 60;
        let access = jwt_with(serde_json::json!({ "exp": exp }));
        assert_eq!(access_token_exp(&access), Some(exp));
        assert_eq!(access_token_exp("not-a-jwt"), None);
        assert_eq!(
            access_token_exp(&jwt_with(serde_json::json!({"sub": "s"}))),
            None,
            "没有 exp claim"
        );

        let theirs = extern_tokens("refresh", &access);
        assert!(
            matches!(plan_adoption(&ours, &theirs), AdoptDecision::Adopt(_)),
            "有效期比 apim 新就该采纳"
        );
        let theirs = extern_tokens("refresh", "not-a-jwt");
        assert!(
            matches!(plan_adoption(&ours, &theirs), AdoptDecision::Reject(_)),
            "读不出有效期就拒绝"
        );
    }
}
