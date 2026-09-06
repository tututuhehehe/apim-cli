//! 并发探活：health + 额度两个请求一起发。额度分两路——声明式 http（本文件）、
//! 脚本逃生舱（script.rs）。

mod script;

pub(crate) use script::expand_tilde;
use script::run_script_balance;

use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::{Client, Method, RequestBuilder, StatusCode};

use crate::config::KeyEntry;
use crate::recipe::{
    self, Auth, AuthKind, BalanceMode, BalanceSpec, BalanceView, HttpCall, Recipe,
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

/// 拉取厂商模型列表：GET `{base_url}/models`（探活同款路径），复用 recipe 鉴权。
/// 超时由 client 自带（15s）。错误以 String 返回，直接进弹窗展示。
/// 模型列表端点候选（按序尝试，404 换下一个）：
/// recipe 显式 `models_url` → 探活路径（以 models 结尾时）→ `{base_url}/models` → `/v1/models`。
/// OpenAI 兼容的 /models|/v1/models 是事实标准，GLM 这类例外用 models_url 配置。
fn model_endpoint_candidates(recipe: &Recipe, token: &str) -> Vec<String> {
    let ctx = recipe::request_ctx(recipe, token);
    let mut urls: Vec<String> = Vec::new();
    let mut push = |tpl: Option<&str>| {
        if let Some(tpl) = tpl {
            let url = recipe::subst(tpl, &ctx);
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
    };
    push(recipe.models_url.as_deref());
    if let Some(health) = &recipe.health
        && health.url.ends_with("models")
    {
        push(Some(&health.url));
    }
    push(Some("{base_url}/models"));
    push(Some("{base_url}/v1/models"));
    urls
}

pub async fn fetch_models(
    client: &Client,
    recipe: &Recipe,
    token: &str,
) -> std::result::Result<Vec<String>, String> {
    let mut last_err = "没有可用的模型端点".to_string();
    for url in model_endpoint_candidates(recipe, token) {
        let call = HttpCall::get(url);
        let response = send(client, recipe, &call, token)
            .await
            .map_err(|err| compact_error(&err))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| truncate(&err.to_string(), 200))?;
        if status == reqwest::StatusCode::NOT_FOUND {
            // 该厂商不用这个约定路径，试下一个候选
            last_err = format!("HTTP 404 {}", truncate(&body, 120));
            continue;
        }
        if !status.is_success() {
            return Err(format!("HTTP {} {}", status.as_u16(), truncate(&body, 180)));
        }
        return parse_models(&body);
    }
    Err(last_err)
}

/// 纯函数：模型列表 JSON → 排序去重后的模型名。认 OpenAI 风格
/// `{"data":[{"id":"..."}]}`，兼容裸数组（字符串或 `{"id":...}`）。
fn parse_models(body: &str) -> std::result::Result<Vec<String>, String> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("parse JSON: {err}"))?;
    let entries: Vec<&serde_json::Value> = match &json {
        serde_json::Value::Array(items) => items.iter().collect(),
        serde_json::Value::Object(map) => match map.get("data") {
            Some(serde_json::Value::Array(items)) => items.iter().collect(),
            _ => return Err("响应缺少 data 数组，无法解析模型列表".into()),
        },
        _ => return Err("响应结构异常，无法解析模型列表".into()),
    };
    let mut names: Vec<String> = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            serde_json::Value::String(s) => names.push(s.clone()),
            serde_json::Value::Object(fields) => {
                if let Some(serde_json::Value::String(id)) = fields.get("id") {
                    names.push(id.clone());
                }
            }
            _ => {}
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
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

/// endpoint 会显示在额度面板：Query 鉴权配方的 URL 带 ?api_key=<token>，
/// 展示前把 token 原文抹掉，明文密钥不上屏。
fn display_endpoint(method: &str, url: &str, token: &str) -> String {
    if token.is_empty() {
        return format!("{} {}", method.to_uppercase(), url);
    }
    format!("{} {}", method.to_uppercase(), url.replace(token, "…"))
}

async fn hit_balance(
    client: &Client,
    recipe: &Recipe,
    spec: &BalanceSpec,
    token: &str,
) -> BalanceSnapshot {
    let ctx = recipe::request_ctx(recipe, token);
    let endpoint = display_endpoint(
        &spec.request.method,
        &recipe::subst(&spec.request.url, &ctx),
        token,
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
    use crate::recipe::Recipe;
    use std::collections::HashMap;

    fn candidates_recipe(models_url: Option<&str>, health_url: Option<&str>) -> Recipe {
        Recipe {
            id: "p".into(),
            name: "P".into(),
            base_url: "https://p.example".into(),
            homepage: None,
            models_url: models_url.map(String::from),
            supports_groups: false,
            vars: HashMap::new(),
            auth: Default::default(),
            health: health_url.map(HttpCall::get),
            balance: None,
            origin: None,
        }
    }

    #[test]
    fn model_endpoint_candidates_follow_resolution_order() {
        // 缺省：/models 与 /v1/models 两个候选，去重后有序
        assert_eq!(
            model_endpoint_candidates(&candidates_recipe(None, None), "sk-x"),
            vec!["https://p.example/models", "https://p.example/v1/models"]
        );
        // 探活路径以 models 结尾时优先复用（与默认 /v1/models 去重合并）
        assert_eq!(
            model_endpoint_candidates(
                &candidates_recipe(None, Some("{base_url}/v1/models")),
                "sk-x"
            ),
            vec!["https://p.example/v1/models", "https://p.example/models"]
        );
        // 显式 models_url 最优先（GLM 这类非标厂商），token 不进 URL
        assert_eq!(
            model_endpoint_candidates(
                &candidates_recipe(Some("{base_url}/api/paas/v4/models"), None),
                "sk-secret"
            ),
            vec![
                "https://p.example/api/paas/v4/models",
                "https://p.example/models",
                "https://p.example/v1/models"
            ]
        );
    }

    #[test]
    fn parse_models_openai_data_shape() {
        let body = r#"{"object":"list","data":[
            {"id":"deepseek-reasoner","object":"model"},
            {"id":"deepseek-chat","object":"model"}
        ]}"#;
        let models = parse_models(body).expect("OpenAI 形态应解析成功");
        assert_eq!(models, vec!["deepseek-chat", "deepseek-reasoner"]);
    }

    #[test]
    fn parse_models_bare_array_sorts_and_dedupes() {
        let body = r#"["b-model", {"id":"a-model"}, "b-model", {"object":"model"}]"#;
        let models = parse_models(body).expect("裸数组应解析成功");
        // 排序去重；没有 id 的对象跳过
        assert_eq!(models, vec!["a-model", "b-model"]);
    }

    #[test]
    fn parse_models_bad_json_errors() {
        let err = parse_models("<html>502</html>").expect_err("坏 JSON 应报错");
        assert!(err.contains("parse JSON"), "实际错误: {err}");
    }

    #[test]
    fn parse_models_object_without_data_errors() {
        let err = parse_models(r#"{"error":{"message":"nope"}}"#).expect_err("缺 data 应报错");
        assert!(err.contains("data"), "实际错误: {err}");
    }

    #[test]
    fn display_endpoint_redacts_token_in_query_urls() {
        // Query 鉴权配方：token 在 URL 里，展示前必须抹掉
        assert_eq!(
            display_endpoint(
                "GET",
                "https://x.io/api/usage?api_key=sk-secret-123&span=month",
                "sk-secret-123"
            ),
            "GET https://x.io/api/usage?api_key=…&span=month"
        );
        // Bearer 配方 URL 不含 token，原样展示
        assert_eq!(
            display_endpoint("GET", "https://api.deepseek.com/user/balance", "sk-other"),
            "GET https://api.deepseek.com/user/balance"
        );
    }
}
