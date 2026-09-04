//! `apim status / copy / use`：跑真实探活+额度、复制、输出 shell 环境行。

use anyhow::{Result, bail};
use serde_json::json;

use super::{Args, Ctx};
use crate::config::KeyEntry;
use crate::probe::{self, Health, ProbeResult};
use crate::recipe::BalanceView;

pub(crate) async fn status(ctx: &Ctx, argv: &[String]) -> Result<()> {
    let args = Args::parse(argv)?;
    let filter = args.pos(0);
    let recipes = ctx.load_recipes()?;
    let keys = ctx.load_keys(&recipes)?;
    let matched: Vec<&KeyEntry> = keys
        .iter()
        .filter(|k| filter.is_none_or(|p| k.provider == p))
        .collect();
    if matched.is_empty() {
        println!("还没有密钥（apim key add <provider> <别名>，token 走 stdin）");
        return Ok(());
    }

    let client = probe::client()?;
    let mut results: Vec<(String, ProbeResult)> = Vec::new();
    for key in matched {
        let Some(recipe) = recipes.get(&key.provider) else {
            continue; // load_keys 已保证 provider 存在，防御一下
        };
        let result = probe::probe(&client, recipe, key).await;
        results.push((key.id(), result));
    }

    if args.has("json") {
        let out: Vec<_> = results
            .iter()
            .map(|(id, r)| {
                json!({
                    "id": id,
                    "health": health_json(&r.health),
                    "balance": balance_json(&r.balance),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    for (id, r) in &results {
        match &r.health {
            Health::Live { ms } => println!("{id:<24} ● 可用 {ms}ms"),
            Health::Down { ms, message } => println!("{id:<24} ● 失败 {ms}ms  {message}"),
            Health::Unknown => println!("{id:<24} ● 未配置探活"),
            Health::Checking => println!("{id:<24} … 检查中"),
        }
        if let Some(b) = &r.balance {
            if let Some(err) = &b.error {
                println!("    额度失败：{err}");
            } else if let Some(view) = &b.view {
                for line in view_lines(view) {
                    println!("    {line}");
                }
                println!("    {} · {}ms", b.endpoint, b.elapsed_ms);
            }
        }
    }
    Ok(())
}

fn health_json(h: &Health) -> serde_json::Value {
    match h {
        Health::Live { ms } => serde_json::json!({"ok": true, "ms": ms}),
        Health::Down { ms, message } => {
            serde_json::json!({"ok": false, "ms": ms, "error": message})
        }
        Health::Unknown | Health::Checking => serde_json::Value::Null,
    }
}

fn balance_json(b: &Option<probe::BalanceSnapshot>) -> serde_json::Value {
    let Some(b) = b else {
        return serde_json::Value::Null;
    };
    match (&b.error, &b.view) {
        (Some(err), _) => serde_json::json!({
            "ok": false, "endpoint": b.endpoint,
            "elapsed_ms": b.elapsed_ms, "error": err,
        }),
        (None, Some(view)) => serde_json::json!({
            "ok": true, "endpoint": b.endpoint,
            "elapsed_ms": b.elapsed_ms,
            "lines": view_lines(view),
        }),
        (None, None) => serde_json::Value::Null,
    }
}

/// 统一成「若干行文本」：脚本形态即原始输出；http 形态由 BalanceView 合成。
fn view_lines(view: &BalanceView) -> Vec<String> {
    if !view.lines.is_empty() {
        return view.lines.clone();
    }
    let mut out = Vec::new();
    for item in &view.items {
        let amount = item
            .ctx
            .get("total_balance")
            .cloned()
            .unwrap_or_else(|| view.headline.clone());
        out.push(crate::recipe::money(item.currency.as_deref(), &amount));
        let fields: Vec<String> = item
            .fields
            .iter()
            .map(|(label, value)| format!("{label} {value}"))
            .collect();
        if !fields.is_empty() {
            out.push(fields.join("    "));
        }
    }
    out
}

pub(crate) fn copy(ctx: &Ctx, argv: &[String]) -> Result<()> {
    let args = Args::parse(argv)?;
    let Some(id) = args.pos(0) else {
        bail!("用法：apim copy <厂商.别名> [--base-url]");
    };
    let (key, recipe) = find(ctx, id)?;
    let what = if args.has("base-url") {
        recipe.base_url.clone()
    } else {
        key.token.clone()
    };
    crate::clipboard::copy(&what)?;
    if args.has("base-url") {
        println!("已复制 Base URL：{what}");
    } else {
        println!("已复制 {id} 的密钥");
    }
    Ok(())
}

pub(crate) fn use_env(ctx: &Ctx, argv: &[String]) -> Result<()> {
    let args = Args::parse(argv)?;
    let Some(id) = args.pos(0) else {
        bail!("用法：apim use <厂商.别名>");
    };
    let (key, recipe) = find(ctx, id)?;
    println!(
        "export OPENAI_API_KEY='{}'",
        key.token.replace('\'', "'\\''")
    );
    println!("export OPENAI_BASE_URL='{}'", recipe.base_url);
    Ok(())
}

fn find(ctx: &Ctx, id: &str) -> Result<(KeyEntry, crate::recipe::Recipe)> {
    let Some((provider, alias)) = id.split_once('.') else {
        bail!("密钥 id 形如 厂商.别名，得到：{id}");
    };
    let recipes = ctx.load_recipes()?;
    let keys = ctx.load_keys(&recipes)?;
    let key = keys
        .iter()
        .find(|k| k.provider == provider && k.alias == alias)
        .cloned();
    let Some(key) = key else {
        bail!("密钥 {id} 不存在");
    };
    let recipe = recipes
        .get(&key.provider)
        .cloned()
        .expect("load_keys 已保证 provider 存在");
    Ok((key, recipe))
}
