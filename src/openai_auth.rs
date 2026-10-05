//! apim-owned OpenAI Codex OAuth dynamic registration, private storage and usage lookup.
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
/// Codex CLI 系客户端的公开 client id（Pi 与 cc-switch 都用它），仅 `APIM_OAUTH_CLIENT=codex` 档位使用。
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
    /// 只申请 `openid profile email`，但 token 带 `chatgpt_account_id` 且能读用量接口。
    #[serde(default)]
    pub codex_family: bool,
}
#[derive(Debug, Clone)]
pub struct Usage {
    pub lines: Vec<String>,
}
/// 浏览器流程用哪套客户端注册。
///
/// - `apim`（缺省）：OpenAI 文档里的开源动态注册，首次登录由 OpenAI 签发 `oaiapp_…`。
/// - `codex`（`APIM_OAUTH_CLIENT=codex`）：复刻 Codex CLI 系的公开 client id 与其参数，
///   与 Pi / cc-switch 完全一致（`localhost` 回调、不发 nonce、不申请 resource 与
///   `chatgpt.tokens.use.direct`）。动态注册在浏览器里被拒时用它兜底。
struct ClientProfile {
    name: &'static str,
    /// `Some` = 固定 client id（不需要也不采纳动态注册）
    client_id: Option<&'static str>,
    redirect: &'static str,
    scope: &'static str,
    codex_family: bool,
}

fn profile() -> ClientProfile {
    profile_for(std::env::var("APIM_OAUTH_CLIENT").ok().as_deref())
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
        _ => ClientProfile {
            name: "codex",
            client_id: Some(CODEX_CLIENT_ID),
            redirect: "http://localhost:1455/auth/callback",
            scope: "openid profile email offline_access",
            codex_family: true,
        },
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
    crate::config::write_private(&path(dir), &serde_json::to_string_pretty(cred)?)
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
#[derive(Deserialize)]
struct AccessClaims {
    #[serde(default)]
    sub: Option<String>,
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

/// OpenAI documented open-source OAuth flow (dynamic registration on first login).
pub async fn login(dir: &Path) -> Result<()> {
    let mut log = AttemptLog::new();
    let outcome = login_inner(dir, &mut log).await;
    log.save(dir);
    // 失败时把日志路径带上：TUI 只能看到一行 toast，细节都在日志里
    outcome.with_context(|| format!("登录详情见 {}", log_path(dir).display()))
}

async fn login_inner(dir: &Path, log: &mut AttemptLog) -> Result<()> {
    let p = profile();
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
    let listener = TcpListener::bind(("127.0.0.1", 1455))
        .await
        .context("OAuth 回调端口 1455 被占用")?;
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
    crate::browser::open(url.as_str()).context("打开 OpenAI OAuth 登录页")?;
    eprintln!("已打开浏览器，请完成 OpenAI/Codex 授权…");
    let (code, got_state, returned_id, callback_scope) =
        tokio::time::timeout(std::time::Duration::from_secs(600), callback(listener))
            .await
            .context("OAuth 登录超时")??;
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
    println!(
        "OpenAI Codex OAuth 已保存到 {}（诊断日志 {}）",
        path(dir).display(),
        log_path(dir).display()
    );
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
                log.step("id token：Codex CLI 档位不校验（同 Pi/cc-switch）".to_string());
                if let Ok(claims) = decode_unverified::<IdClaims>(id_token) {
                    identity.subject = claims.sub;
                    identity.email = claims.email;
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

async fn refresh(dir: &Path, mut c: Credential, http: &Client) -> Result<Credential> {
    if c.expires_at > now() + 60 {
        return Ok(c);
    }
    let mut log = AttemptLog::new();
    let r = token_grant(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &c.client_id),
            ("refresh_token", &c.refresh_token),
            ("resource", "https://api.openai.com/v1"),
        ],
        "刷新 token",
        &mut log,
    )
    .await
    .inspect_err(|_| log.save(dir))?;
    let refresh_token = r
        .refresh_token
        .clone()
        .filter(|s| !s.is_empty())
        .context("刷新响应没有新的 refresh_token")?;
    c.access_token = r.access_token;
    c.refresh_token = refresh_token;
    c.expires_at = now() + r.expires_in;
    if let Some(id) = r.id_token.filter(|s| !s.is_empty()) {
        c.id_token = id
    }
    if !r.scope.is_empty() {
        c.scopes = scopes(&r.scope)
    }
    let access: AccessClaims = decode_unverified(&c.access_token)?;
    c.account_id = access
        .auth
        .and_then(|a| a.chatgpt_account_id)
        .context("刷新 token 缺少 account id")?;
    save(dir, &c)?;
    Ok(c)
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
        log.step(format!(
            "{step}：{url} → HTTP {}，响应 {} 字符",
            status.as_u16(),
            body.len()
        ));
        if status.is_success() {
            return serde_json::from_str(&body)
                .with_context(|| format!("{step}：响应不是预期 JSON（{url}）"));
        }
        failure = format!(
            "{step}：{url} 返回 HTTP {}：{}",
            status.as_u16(),
            redact(&body)
        );
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

/// 一次登录/刷新尝试的步骤记录（含状态码与遮罩后的响应体）。
///
/// 失败时 TUI 只能看到一行 toast，而这条链路（浏览器 → 回调 → token 端点 →
/// ID token 校验）每一步都可能断，所以把步骤落到磁盘供事后诊断。
/// **只记状态码、长度、client_id/scope 这类不敏感内容，不含任何 token。**
pub struct AttemptLog {
    steps: Vec<String>,
}

impl AttemptLog {
    fn new() -> Self {
        Self { steps: Vec::new() }
    }

    fn step(&mut self, step: String) {
        eprintln!("  {step}");
        self.steps.push(step);
    }

    fn save(&mut self, dir: &Path) {
        let mut text = "apim OpenAI OAuth 登录诊断\n".to_string();
        for step in &self.steps {
            text.push_str(step);
            text.push('\n');
        }
        // 日志只是诊断辅助：写不进去也不影响登录本身
        let _ = crate::config::write_private(&log_path(dir), &text);
    }
}

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join("openai-oauth.log")
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
pub async fn fetch_usage(dir: &Path) -> Result<Option<Usage>> {
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
    /// 复制进来的凭据只能查用量：按 `o` 必须重新走动态注册，
    /// 否则会把未知 client_id 发给 OpenAI，报 `invalid_client`。
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
}
