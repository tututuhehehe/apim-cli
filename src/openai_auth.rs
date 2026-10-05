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
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const JWKS_URL: &str = "https://auth.openai.com/.well-known/jwks.json";
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CLIENT_BOOTSTRAP: &str = "dynamic_agent_client";
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
}
#[derive(Debug, Clone)]
pub struct Usage {
    pub lines: Vec<String>,
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
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    id_token: String,
    expires_in: u64,
    #[serde(default)]
    scope: String,
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
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nOpenAI authorization received. Return to apim.").await?;
    Ok((code, state, id, scope))
}

/// OpenAI documented open-source OAuth flow (dynamic registration on first login).
pub async fn login(dir: &Path) -> Result<()> {
    let previous = load(dir)?;
    let host = match &previous {
        Some(c) => c.host_id.clone(),
        None => host_id(dir)?,
    };
    let listener = TcpListener::bind(("127.0.0.1", 1455))
        .await
        .context("OAuth 回调端口 1455 被占用")?;
    let redirect = "http://127.0.0.1:1455/auth/callback";
    let state = random_b64(24);
    let nonce = random_b64(24);
    let verifier = random_b64(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = url::Url::parse(AUTH_URL)?;
    let requested_id = previous
        .as_ref()
        .map(|c| c.client_id.as_str())
        .unwrap_or(CLIENT_BOOTSTRAP);
    for (k, v) in [
        ("client_id", requested_id),
        ("ext_agent_host_id", &host),
        ("response_type", "code"),
        ("redirect_uri", redirect),
        (
            "scope",
            "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
        ),
        ("resource", "https://api.openai.com/v1"),
        ("state", &state),
        ("nonce", &nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", &challenge),
    ] {
        url.query_pairs_mut().append_pair(k, v);
    }
    if let Some(old) = &previous {
        url.query_pairs_mut()
            .append_pair("id_token_hint", &old.id_token);
        if let Some(email) = &old.email {
            url.query_pairs_mut().append_pair("login_hint", email);
        }
    } else {
        url.query_pairs_mut().append_pair("agent_name_hint", "apim");
    }
    crate::browser::open(url.as_str()).context("打开 OpenAI OAuth 登录页")?;
    eprintln!("已打开浏览器，请完成 OpenAI/Codex 授权…");
    let (code, got_state, returned_id, callback_scope) =
        tokio::time::timeout(std::time::Duration::from_secs(600), callback(listener))
            .await
            .context("OAuth 登录超时")??;
    if got_state != state {
        bail!("OAuth state 不匹配，拒绝此回调");
    }
    let client_id = match (&previous, returned_id) {
        (None, Some(id)) => id,
        (None, None) => bail!("动态注册回调缺少 OpenAI 签发的 client_id"),
        (Some(old), Some(id)) if id != old.client_id => bail!("回调 client_id 与已保存注册不匹配"),
        (Some(old), _) => old.client_id.clone(),
    };
    if client_id.is_empty() || client_id == CLIENT_BOOTSTRAP {
        bail!("OpenAI 没有返回可持久化的动态 client_id");
    }
    let http = client()?;
    let token: TokenResponse = http
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", redirect),
            ("resource", "https://api.openai.com/v1"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .context("解析 OpenAI token 响应")?;
    let granted = if token.scope.is_empty() {
        callback_scope
    } else {
        token.scope.clone()
    };
    let scope_list = scopes(&granted);
    if !scope_list.iter().any(|s| s == REQUIRED_SCOPE) {
        bail!("未获 Codex 用量权限，请在浏览器授权时同意该权限后重试");
    }
    let claims = validate_id_token(&http, &token.id_token, &client_id, &nonce).await?;
    let access: AccessClaims = decode_unverified(&token.access_token)?;
    let account = access
        .auth
        .and_then(|a| a.chatgpt_account_id)
        .context("access token 缺少 ChatGPT account id")?;
    let cred = Credential {
        client_id,
        host_id: host,
        subject: claims.sub,
        email: claims.email,
        id_token: token.id_token,
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now() + token.expires_in,
        scopes: scope_list,
        account_id: account,
    };
    save(dir, &cred)?;
    println!("OpenAI Codex OAuth 已保存到 {}", path(dir).display());
    Ok(())
}

async fn refresh(dir: &Path, mut c: Credential, http: &Client) -> Result<Credential> {
    if c.expires_at > now() + 60 {
        return Ok(c);
    }
    #[derive(Deserialize)]
    struct R {
        access_token: String,
        refresh_token: String,
        expires_in: u64,
        #[serde(default)]
        id_token: Option<String>,
        #[serde(default)]
        scope: String,
    }
    let r: R = http
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", &c.client_id),
            ("refresh_token", &c.refresh_token),
            ("resource", "https://api.openai.com/v1"),
        ])
        .send()
        .await?
        .error_for_status()
        .context("OpenAI OAuth 刷新失败")?
        .json()
        .await?;
    c.access_token = r.access_token;
    c.refresh_token = r.refresh_token;
    c.expires_at = now() + r.expires_in;
    if let Some(id) = r.id_token {
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
    if !c.scopes.iter().any(|s| s == REQUIRED_SCOPE) {
        bail!("OAuth 凭据未获 Codex 用量权限，请重新登录")
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
        let label = match w.limit_window_seconds.unwrap_or_default() {
            0..=21600 => "5h",
            21601..=691200 => "1w",
            _ => "Quota",
        };
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
    Ok(Some(Usage { lines }))
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
