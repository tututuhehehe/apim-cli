//! 并发探活：health 请求 + 额度脚本一起跑。额度统一走脚本（script.rs）：
//! env 注入 key，stdout 逐行直显。

mod script;

use script::run_script_balance;

use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::{Client, Method, RequestBuilder, StatusCode};

use crate::config::KeyEntry;
use crate::recipe::{self, Auth, AuthKind, HttpCall, Recipe};
use crate::util::truncate;

#[derive(Debug, Clone)]
pub enum Health {
    Unknown,
    Checking,
    Live { ms: u64 },
    Down { ms: u64, message: String },
}

#[derive(Debug, Clone)]
pub struct BalanceSnapshot {
    /// 脚本 stdout（非空，首行为 headline）；None = 脚本失败（error 有值）。
    pub lines: Option<Vec<String>>,
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
            Some(spec) => Some(run_script_balance(recipe, spec, key).await),
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

/// 拉取厂商模型列表，复用 recipe 鉴权。超时由 client 自带（15s）。
/// 错误以 String 返回，直接进弹窗展示。
/// 端点候选（按序尝试，仅 404 换下一个，其余错误直接返回保留真实原因）：
/// recipe 显式 `models_url` → 探活路径（以 `/models` 结尾时）→ `{base_url}/models` → `{base_url}/v1/models`。
/// OpenAI 兼容约定是事实标准，GLM 这类例外用 models_url 配置。
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
        && health.url.ends_with("/models")
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
///
/// 只取名字，不看厂商声明的端点能力之类：`m` 键浏览用的就是这份列表，
/// 一键导入面板复用同一个函数，保证两边「看到的模型完全一致」。
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
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in entries {
        match entry {
            serde_json::Value::String(name) => {
                names.insert(name.clone());
            }
            serde_json::Value::Object(fields) => {
                if let Some(serde_json::Value::String(id)) = fields.get("id") {
                    names.insert(id.clone());
                }
            }
            _ => {}
        }
    }
    Ok(names.into_iter().collect())
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
        // 探活路径只是「以 models 结尾」但不是 /models 端点时不收进候选，
        // 免得它 200 返回非模型 JSON 就提前判死整个回退链
        assert_eq!(
            model_endpoint_candidates(
                &candidates_recipe(None, Some("{base_url}/freemodels")),
                "sk-x"
            ),
            vec!["https://p.example/models", "https://p.example/v1/models"]
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

    /// 厂商多给什么字段都不影响：只取名字（`supported_endpoint_types` 之类一律忽略）。
    #[test]
    fn parse_models_ignores_extra_capability_fields() {
        let body = r#"{"data":[
            {"id":"gpt-6-sol","supported_endpoint_types":["openai"]},
            {"id":"gpt-5.5","supported_endpoint_types":["openai","openai-response"]},
            {"id":"deepseek-flash","context_window":1048576,"input_modalities":["text","image"]}
        ]}"#;
        let models = parse_models(body).expect("多余字段应被忽略");
        assert_eq!(models, vec!["deepseek-flash", "gpt-5.5", "gpt-6-sol"]);
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
}
