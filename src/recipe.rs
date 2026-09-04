use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BUILTIN_DEEPSEEK: &str = include_str!("../recipes/deepseek.yaml");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub supports_groups: bool,
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HttpCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<BalanceSpec>,
    /// 本地 YAML 路径；None = 内置（编译进二进制），不可删除。
    #[serde(skip)]
    pub origin: Option<PathBuf>,
}

impl Recipe {
    pub fn normalize(&mut self) {
        while self.base_url.ends_with('/') {
            self.base_url.pop();
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Auth {
    #[serde(default)]
    pub kind: AuthKind,
    pub header: Option<String>,
    pub prefix: Option<String>,
    pub query_param: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    #[default]
    Bearer,
    Header,
    Query,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpCall {
    #[serde(default = "default_get")]
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

impl HttpCall {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: default_get(),
            url: url.into(),
            headers: HashMap::new(),
            body: None,
        }
    }
}

fn default_get() -> String {
    "GET".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BalanceSpec {
    pub request: HttpCall,
    #[serde(default)]
    pub parse: ParseSpec,
    pub render: RenderSpec,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ParseSpec {
    /// Dotted path to a boolean "account usable" flag.
    pub available: Option<String>,
    /// Dotted path to an array of objects (e.g. per-currency balances).
    pub items: Option<String>,
    /// When `items` is absent, build one virtual item from these root paths.
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderSpec {
    pub headline: String,
    #[serde(default)]
    pub fields: Vec<RenderField>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct BalanceView {
    pub available: Option<bool>,
    pub headline: String,
    pub items: Vec<BalanceItem>,
}

#[derive(Debug, Clone)]
pub struct BalanceItem {
    pub currency: Option<String>,
    pub fields: Vec<(String, String)>,
    pub ctx: HashMap<String, String>,
}

pub fn load_recipes() -> Result<HashMap<String, Recipe>> {
    let mut map = HashMap::new();
    insert_yaml(&mut map, BUILTIN_DEEPSEEK, "builtin:deepseek", None)?;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("recipes");
    load_dir(&manifest_dir, &mut map)?;
    load_dir(&user_recipes_dir(), &mut map)?;
    Ok(map)
}

pub fn user_recipes_dir() -> PathBuf {
    crate::config::config_dir().join("recipes")
}

/// 写入用户 recipe 目录，返回文件路径。
pub fn save_user_recipe(recipe: &Recipe) -> Result<PathBuf> {
    save_user_recipe_to(&user_recipes_dir(), recipe)
}

pub fn delete_user_recipe(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("delete {}", path.display()))
}

pub fn save_user_recipe_to(dir: &Path, recipe: &Recipe) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
    let path = dir.join(format!("{}.yaml", recipe.id));
    let yaml = serde_yaml::to_string(recipe).context("serialize recipe")?;
    fs::write(&path, yaml).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn load_dir(dir: &Path, map: &mut HashMap<String, Recipe>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut files: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("yaml") | Some("yml")
            )
        })
        .collect();
    files.sort();
    for path in files {
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        insert_yaml(map, &raw, &path.display().to_string(), Some(path.clone()))?;
    }
    Ok(())
}

fn insert_yaml(
    map: &mut HashMap<String, Recipe>,
    raw: &str,
    origin: &str,
    path: Option<PathBuf>,
) -> Result<()> {
    let mut recipe: Recipe =
        serde_yaml::from_str(raw).with_context(|| format!("parse {origin}"))?;
    recipe.normalize();
    recipe.origin = path;
    map.insert(recipe.id.clone(), recipe);
    Ok(())
}

pub fn request_ctx(recipe: &Recipe, token: &str) -> HashMap<String, String> {
    HashMap::from([
        ("token".into(), token.to_string()),
        ("base_url".into(), recipe.base_url.clone()),
    ])
}

pub fn subst(input: &str, ctx: &HashMap<String, String>) -> String {
    let mut out = input.to_string();
    for (k, v) in ctx {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

pub fn subst_json(value: &Value, ctx: &HashMap<String, String>) -> Value {
    match value {
        Value::String(s) => Value::String(subst(s, ctx)),
        Value::Array(items) => Value::Array(items.iter().map(|v| subst_json(v, ctx)).collect()),
        Value::Object(map) => {
            let mut next = serde_json::Map::new();
            for (k, v) in map {
                next.insert(k.clone(), subst_json(v, ctx));
            }
            Value::Object(next)
        }
        other => other.clone(),
    }
}

pub fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(root);
    }
    let mut cur = root;
    for seg in path.split('.') {
        cur = match cur {
            Value::Object(map) => map.get(seg)?,
            Value::Array(arr) => arr.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn flatten_object(v: &Value) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Value::Object(map) = v {
        for (k, val) in map {
            out.insert(k.clone(), value_to_string(val));
        }
    }
    out
}

pub fn parse_balance(spec: &BalanceSpec, root: &Value) -> Result<BalanceView> {
    let available = spec.parse.available.as_ref().and_then(|path| {
        get_path(root, path).and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            Value::String(s) => s.parse().ok(),
            Value::Number(n) => Some(n.as_i64()? != 0),
            _ => None,
        })
    });

    let mut items_ctx: Vec<HashMap<String, String>> = Vec::new();
    if let Some(path) = &spec.parse.items {
        let node = get_path(root, path)
            .with_context(|| format!("response missing array path `{path}`"))?;
        match node {
            Value::Array(arr) => {
                for el in arr {
                    items_ctx.push(flatten_object(el));
                }
            }
            Value::Object(_) => items_ctx.push(flatten_object(node)),
            _ => anyhow::bail!("`{path}` is not an array/object"),
        }
    } else if !spec.parse.fields.is_empty() {
        let mut ctx = HashMap::new();
        for (name, path) in &spec.parse.fields {
            if let Some(v) = get_path(root, path) {
                ctx.insert(name.clone(), value_to_string(v));
            }
        }
        items_ctx.push(ctx);
    } else {
        items_ctx.push(flatten_object(root));
    }

    if items_ctx.is_empty() {
        items_ctx.push(HashMap::new());
    }

    let items: Vec<BalanceItem> = items_ctx
        .into_iter()
        .map(|ctx| {
            let fields = spec
                .render
                .fields
                .iter()
                .map(|f| (f.label.clone(), subst(&f.value, &ctx)))
                .collect();
            let currency = ctx.get("currency").cloned().filter(|s| !s.is_empty());
            BalanceItem {
                currency,
                fields,
                ctx,
            }
        })
        .collect();

    let headline = items
        .first()
        .map(|item| subst(&spec.render.headline, &item.ctx))
        .unwrap_or_default();

    Ok(BalanceView {
        available,
        headline,
        items,
    })
}

pub fn money(currency: Option<&str>, amount: &str) -> String {
    if amount.is_empty() {
        return "—".into();
    }
    match currency {
        Some("CNY") | Some("RMB") => format!("¥{amount}"),
        Some("USD") => format!("${amount}"),
        Some("EUR") => format!("€{amount}"),
        Some(other) => format!("{other} {amount}"),
        None => amount.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-recipe-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_user_recipe() -> Recipe {
        Recipe {
            id: "my-relay".into(),
            name: "我的中转站".into(),
            base_url: "https://relay.example.com/v1".into(),
            supports_groups: false,
            auth: Auth {
                kind: AuthKind::Bearer,
                header: None,
                prefix: None,
                query_param: None,
            },
            health: Some(HttpCall::get("{base_url}/models")),
            balance: Some(BalanceSpec {
                request: HttpCall::get("{base_url}/v1/dashboard/billing/subscription"),
                parse: ParseSpec {
                    available: None,
                    items: None,
                    fields: HashMap::from([(
                        "total_balance".to_string(),
                        "hard_limit_usd".to_string(),
                    )]),
                },
                render: RenderSpec {
                    headline: "{total_balance}".into(),
                    fields: vec![RenderField {
                        label: "额度".into(),
                        value: "{total_balance}".into(),
                    }],
                },
            }),
            origin: None,
        }
    }

    #[test]
    fn user_recipe_save_roundtrip() {
        let dir = temp_dir("roundtrip");
        let recipe = sample_user_recipe();
        let path = save_user_recipe_to(&dir, &recipe).unwrap();
        assert!(path.ends_with("my-relay.yaml"));

        let mut map = HashMap::new();
        load_dir(&dir, &mut map).unwrap();
        let loaded = map.get("my-relay").expect("recipe loaded");
        assert_eq!(loaded.name, "我的中转站");
        assert_eq!(loaded.base_url, "https://relay.example.com/v1");
        assert_eq!(loaded.origin.as_deref(), Some(path.as_path()));
        let balance = loaded.balance.as_ref().unwrap();
        assert_eq!(
            balance
                .parse
                .fields
                .get("total_balance")
                .map(String::as_str),
            Some("hard_limit_usd")
        );
        delete_user_recipe(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn generic_balance_parses_flat_response() {
        let recipe = sample_user_recipe();
        let spec = recipe.balance.as_ref().unwrap();
        let json: Value = serde_json::from_str(r#"{"hard_limit_usd": 12.5}"#).unwrap();
        let view = parse_balance(spec, &json).unwrap();
        assert_eq!(view.headline, "12.5");
        assert_eq!(view.items.len(), 1);
    }

    #[test]
    fn deepseek_recipe_parses_live_shape() {
        let recipe: Recipe = serde_yaml::from_str(BUILTIN_DEEPSEEK).unwrap();
        let spec = recipe.balance.as_ref().unwrap();
        let json: Value = serde_json::from_str(
            r#"{
                "is_available": true,
                "balance_infos": [{
                    "currency": "CNY",
                    "total_balance": "4.22",
                    "granted_balance": "0.00",
                    "topped_up_balance": "4.22"
                }]
            }"#,
        )
        .unwrap();
        let view = parse_balance(spec, &json).unwrap();
        assert_eq!(view.available, Some(true));
        assert_eq!(view.headline, "4.22");
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].currency.as_deref(), Some("CNY"));
        assert_eq!(view.items[0].fields[0], ("总额".into(), "4.22".into()));
    }
}
