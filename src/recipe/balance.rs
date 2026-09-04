//! 额度响应的解析与展示：JSON -> BalanceView。

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::Value;

use super::{BalanceSpec, subst};

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

pub(crate) fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
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

pub(crate) fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn flatten_object(v: &Value) -> HashMap<String, String> {
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

    if let Some(divisor) = spec.parse.divisor.filter(|d| *d != 0.0) {
        for ctx in &mut items_ctx {
            for value in ctx.values_mut() {
                if let Ok(n) = value.parse::<f64>() {
                    *value = format_scaled(n / divisor);
                }
            }
        }
    }
    if let Some(currency) = &spec.parse.currency {
        for ctx in &mut items_ctx {
            ctx.insert("currency".into(), currency.clone());
        }
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

/// 保留两位小数并去掉尾随零：17.52883 -> "17.53"，4.0 -> "4"。
fn format_scaled(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::super::{BalanceSpec, ParseSpec, RenderField, RenderSpec, money, parse_balance};
    use super::*;

    fn generic_spec() -> BalanceSpec {
        BalanceSpec {
            request: crate::recipe::HttpCall::get("{base_url}/billing"),
            parse: ParseSpec {
                available: None,
                items: None,
                fields: HashMap::from([(
                    "total_balance".to_string(),
                    "hard_limit_usd".to_string(),
                )]),
                divisor: None,
                currency: None,
            },
            render: RenderSpec {
                headline: "{total_balance}".into(),
                fields: vec![RenderField {
                    label: "额度".into(),
                    value: "{total_balance}".into(),
                }],
            },
        }
    }

    #[test]
    fn generic_balance_parses_flat_response() {
        let spec = generic_spec();
        let json: Value = serde_json::from_str(r#"{"hard_limit_usd": 12.5}"#).unwrap();
        let view = parse_balance(&spec, &json).unwrap();
        assert_eq!(view.headline, "12.5");
        assert_eq!(view.items.len(), 1);
    }

    #[test]
    fn deepseek_recipe_parses_live_shape() {
        let recipe: crate::recipe::Recipe =
            serde_yaml::from_str(include_str!("../../recipes/deepseek.yaml")).unwrap();
        let spec = recipe.balance.clone().expect("deepseek has balance");
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
        let view = parse_balance(&spec, &json).unwrap();
        assert_eq!(view.available, Some(true));
        assert_eq!(view.headline, "4.22");
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].currency.as_deref(), Some("CNY"));
        assert_eq!(view.items[0].fields[0], ("总额".into(), "4.22".into()));
    }

    #[test]
    fn money_formats_currencies() {
        assert_eq!(money(Some("CNY"), "4.22"), "¥4.22");
        assert_eq!(money(Some("USD"), "1.5"), "$1.5");
        assert_eq!(money(None, "9"), "9");
        assert_eq!(money(None, ""), "—");
    }

    #[test]
    fn divisor_and_currency_transform_newapi_quota() {
        let spec = BalanceSpec {
            request: crate::recipe::HttpCall::get("{base_url}/api/user/self"),
            parse: ParseSpec {
                available: None,
                items: None,
                fields: HashMap::from([
                    ("total_balance".to_string(), "data.quota".to_string()),
                    ("used".to_string(), "data.used_quota".to_string()),
                ]),
                divisor: Some(500000.0),
                currency: Some("USD".into()),
            },
            render: RenderSpec {
                headline: "{total_balance}".into(),
                fields: vec![RenderField {
                    label: "剩余".into(),
                    value: "{total_balance}".into(),
                }],
            },
        };
        let json: Value =
            serde_json::from_str(r#"{"data":{"quota": 8764415, "used_quota": 1235585}}"#).unwrap();
        let view = parse_balance(&spec, &json).unwrap();
        assert_eq!(view.headline, "17.53");
        assert_eq!(
            view.items[0].ctx.get("used").map(String::as_str),
            Some("2.47")
        );
        assert_eq!(view.items[0].currency.as_deref(), Some("USD"));
    }
}
