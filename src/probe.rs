use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::{Client, Method, RequestBuilder, StatusCode};

use crate::config::KeyEntry;
use crate::recipe::{
    self, Auth, AuthKind, BalanceMode, BalanceSpec, BalanceView, HttpCall, Recipe, ScriptSpec,
};

#[derive(Debug, Clone)]
pub enum Health {
    Unknown,
    Checking,
    Live { ms: u64 },
    Down { ms: u64, message: String },
}

#[derive(Debug, Clone)]
pub struct BalanceSnapshot {
    pub view: Option<BalanceView>,
    pub endpoint: String,
    pub status: Option<u16>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub key_id: String,
    pub health: Health,
    pub balance: Option<BalanceSnapshot>,
}

pub fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("apim/0.1")
        .build()?)
}

pub async fn probe(client: &Client, recipe: &Recipe, key: &KeyEntry) -> ProbeResult {
    let health_fut = async {
        match &recipe.health {
            Some(call) => match hit_health(client, recipe, call, &key.token).await {
                Ok((status, ms)) if status.is_success() => Health::Live { ms },
                Ok((status, ms)) => Health::Down {
                    ms,
                    message: format!("HTTP {}", status.as_u16()),
                },
                Err(err) => Health::Down {
                    ms: 0,
                    message: compact_error(&err),
                },
            },
            None => Health::Unknown,
        }
    };

    let balance_fut = async {
        match &recipe.balance {
            Some(BalanceMode::Http(spec)) => {
                Some(hit_balance(client, recipe, spec, &key.token).await)
            }
            Some(BalanceMode::Script(spec)) => Some(run_script_balance(recipe, spec, key).await),
            None => None,
        }
    };

    let (health, balance) = tokio::join!(health_fut, balance_fut);
    ProbeResult {
        key_id: key.id(),
        health,
        balance,
    }
}

async fn hit_health(
    client: &Client,
    recipe: &Recipe,
    call: &HttpCall,
    token: &str,
) -> Result<(StatusCode, u64)> {
    let started = Instant::now();
    let response = send(client, recipe, call, token).await?;
    let status = response.status();
    let _ = response.bytes().await;
    Ok((status, elapsed_ms(started)))
}

async fn hit_balance(
    client: &Client,
    recipe: &Recipe,
    spec: &BalanceSpec,
    token: &str,
) -> BalanceSnapshot {
    let ctx = recipe::request_ctx(recipe, token);
    let endpoint = format!(
        "{} {}",
        spec.request.method.to_uppercase(),
        recipe::subst(&spec.request.url, &ctx)
    );
    let started = Instant::now();
    match fetch_balance(client, recipe, spec, token).await {
        Ok((status, view)) => BalanceSnapshot {
            view: Some(view),
            endpoint,
            status: Some(status),
            elapsed_ms: elapsed_ms(started),
            error: None,
        },
        Err(err) => BalanceSnapshot {
            view: None,
            endpoint,
            status: None,
            elapsed_ms: elapsed_ms(started),
            error: Some(compact_error(&err)),
        },
    }
}

async fn fetch_balance(
    client: &Client,
    recipe: &Recipe,
    spec: &BalanceSpec,
    token: &str,
) -> Result<(u16, BalanceView)> {
    let response = send(client, recipe, &spec.request, token).await?;
    let status = response.status();
    let body = response.text().await.context("read body")?;
    if !status.is_success() {
        anyhow::bail!("HTTP {} {}", status.as_u16(), truncate(&body, 180));
    }
    let json: serde_json::Value = serde_json::from_str(&body).context("parse JSON")?;
    let view = recipe::parse_balance(spec, &json)?;
    Ok((status.as_u16(), view))
}

/// 脚本形态：跑 spec 指定的脚本，stdout 逐行进额度面板。密钥只经 env 注入。
async fn run_script_balance(recipe: &Recipe, spec: &ScriptSpec, key: &KeyEntry) -> BalanceSnapshot {
    let started = Instant::now();
    let endpoint = format!("script {}", recipe.id);
    match exec_script_balance(recipe, spec, key).await {
        Ok(view) => BalanceSnapshot {
            view: Some(view),
            endpoint,
            status: None,
            elapsed_ms: elapsed_ms(started),
            error: None,
        },
        Err(err) => BalanceSnapshot {
            view: None,
            endpoint,
            status: None,
            elapsed_ms: elapsed_ms(started),
            error: Some(compact_error(&err)),
        },
    }
}

async fn exec_script_balance(
    recipe: &Recipe,
    spec: &ScriptSpec,
    key: &KeyEntry,
) -> Result<BalanceView> {
    let mut cmd = match (spec.command.as_deref(), spec.run.as_deref()) {
        (Some(path), None) => tokio::process::Command::new(expand_tilde(path)),
        (None, Some(script)) => {
            let mut cmd = tokio::process::Command::new(spec.shell.as_deref().unwrap_or("/bin/sh"));
            cmd.arg("-c").arg(script);
            cmd
        }
        _ => anyhow::bail!("balance.kind=script 需要 command 或 run 二选一"),
    };
    cmd.env("APIM_TOKEN", &key.token)
        .env("APIM_BASE_URL", &recipe.base_url)
        .env("APIM_ALIAS", &key.alias)
        .env("APIM_PROVIDER", &recipe.id)
        .envs(script_env_vars(recipe))
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let timeout = Duration::from_secs(spec.timeout_secs.unwrap_or(15));
    let output = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("脚本超时（{}s）", timeout.as_secs()))?
        .context("spawn script")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        let code = output.status.code().unwrap_or(-1);
        if stderr.is_empty() {
            anyhow::bail!("exit {code}");
        }
        anyhow::bail!("exit {code} · {}", truncate(stderr, 180));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    if lines.is_empty() {
        anyhow::bail!("脚本无输出");
    }
    Ok(BalanceView {
        available: None,
        headline: lines[0].clone(),
        items: Vec::new(),
        lines,
    })
}

/// recipe.vars → APIM_VAR_<大写名>，脚本里能取到自定义变量（如访问令牌）。
fn script_env_vars(recipe: &Recipe) -> HashMap<String, String> {
    recipe
        .vars
        .iter()
        .map(|(k, v)| {
            let name = format!("APIM_VAR_{}", k.to_uppercase().replace('-', "_"));
            (name, v.clone())
        })
        .collect()
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home)
            .join(rest)
            .to_string_lossy()
            .into_owned();
    }
    path.to_string()
}

async fn send(
    client: &Client,
    recipe: &Recipe,
    call: &HttpCall,
    token: &str,
) -> Result<reqwest::Response> {
    let ctx = recipe::request_ctx(recipe, token);
    let method = Method::from_bytes(call.method.as_bytes()).unwrap_or(Method::GET);
    let url = recipe::subst(&call.url, &ctx);
    let mut rb = client.request(method, url);
    // 调用自带鉴权头时（如 new-api 用访问令牌查 /api/user/self），不再附加密钥默认鉴权。
    if !has_explicit_auth(call, &recipe.auth) {
        rb = apply_auth(rb, &recipe.auth, token);
    }
    for (k, v) in &call.headers {
        rb = rb.header(k, recipe::subst(v, &ctx));
    }
    if let Some(body) = &call.body {
        rb = rb.json(&recipe::subst_json(body, &ctx));
    }
    rb.send().await.context("request failed")
}

/// call.headers 是否已显式写死鉴权头（大小写不敏感）。
fn has_explicit_auth(call: &HttpCall, auth: &Auth) -> bool {
    if matches!(auth.kind, AuthKind::Query) {
        let param = auth.query_param.as_deref().unwrap_or("api_key");
        return call.url.contains(&format!("{param}="));
    }
    let header = auth.header.as_deref().unwrap_or("Authorization");
    call.headers.keys().any(|k| k.eq_ignore_ascii_case(header))
}

fn apply_auth(rb: RequestBuilder, auth: &Auth, token: &str) -> RequestBuilder {
    match auth.kind {
        AuthKind::Bearer => {
            let header = auth.header.as_deref().unwrap_or("Authorization");
            let prefix = auth.prefix.as_deref().unwrap_or("Bearer ");
            rb.header(header, format!("{prefix}{token}"))
        }
        AuthKind::Header => {
            let header = auth.header.as_deref().unwrap_or("Authorization");
            rb.header(header, token)
        }
        AuthKind::Query => {
            let param = auth.query_param.as_deref().unwrap_or("api_key");
            rb.query(&[(param, token)])
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn compact_error(err: &anyhow::Error) -> String {
    truncate(&err.to_string(), 200)
}

fn truncate(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out.replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script_recipe(run: &str, timeout_secs: Option<u64>) -> Recipe {
        Recipe {
            id: "demo".into(),
            name: "Demo".into(),
            base_url: "https://example.com".into(),
            supports_groups: false,
            vars: HashMap::from([("access_token".to_string(), "at-123".to_string())]),
            auth: Default::default(),
            health: None,
            balance: Some(BalanceMode::Script(ScriptSpec {
                command: None,
                run: Some(run.to_string()),
                shell: None,
                timeout_secs,
            })),
            origin: None,
        }
    }

    fn demo_key() -> KeyEntry {
        KeyEntry {
            provider: "demo".into(),
            alias: "main".into(),
            group: None,
            token: "sk-test".into(),
        }
    }

    fn script_spec(recipe: &Recipe) -> &ScriptSpec {
        recipe.balance.as_ref().unwrap().script().unwrap()
    }

    #[tokio::test]
    async fn script_output_becomes_lines_with_env_injected() {
        let recipe = script_recipe(
            "echo \"token=$APIM_TOKEN var=$APIM_VAR_ACCESS_TOKEN\"; echo '周 3/4'",
            None,
        );
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert!(snapshot.error.is_none(), "{:?}", snapshot.error);
        let view = snapshot.view.unwrap();
        assert_eq!(view.headline, "token=sk-test var=at-123");
        assert_eq!(view.lines, vec!["token=sk-test var=at-123", "周 3/4"]);
        assert_eq!(snapshot.endpoint, "script demo");
    }

    #[tokio::test]
    async fn script_nonzero_exit_reports_stderr() {
        let recipe = script_recipe("echo 配置坏了 >&2; exit 3", None);
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert!(snapshot.view.is_none());
        assert_eq!(snapshot.error.as_deref(), Some("exit 3 · 配置坏了"));
    }

    #[tokio::test]
    async fn script_timeout_kills_process() {
        let recipe = script_recipe("sleep 30", Some(1));
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert!(snapshot.view.is_none());
        assert!(snapshot.error.unwrap().contains("脚本超时"));
    }

    #[tokio::test]
    async fn script_empty_output_is_an_error() {
        let recipe = script_recipe("true", None);
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert_eq!(snapshot.error.as_deref(), Some("脚本无输出"));
    }
}
