//! `apim status / copy / use`：跑真实探活+额度、复制、输出 shell 环境行。

use anyhow::{Result, bail};
use serde_json::json;

use super::{Args, Ctx};
use crate::config::KeyEntry;
use crate::probe::{self, Health, ProbeResult};
use crate::recipe::ProviderKind;

pub(crate) async fn status(ctx: &Ctx, argv: &[String]) -> Result<()> {
    let args = Args::parse(argv)?;
    let filter = args.pos(0);
    let recipes = ctx.load_recipes()?;
    // 拼错的厂商名直接报错，别误导成「还没有密钥」
    if let Some(p) = filter
        && !recipes.contains_key(p)
    {
        bail!("厂商 {p} 不存在");
    }
    let keys = ctx.load_keys(&recipes)?;
    let matched: Vec<&KeyEntry> = keys
        .iter()
        .filter(|k| filter.is_none_or(|p| k.provider == p))
        .collect();
    if matched.is_empty() {
        if filter.is_some() {
            println!("该厂商还没有密钥（apim key add <provider> <别名>，token 走 stdin）");
        } else {
            println!("还没有密钥（apim key add <provider> <别名>，token 走 stdin）");
        }
        return Ok(());
    }

    let client = probe::client()?;
    // 并发探活：串行 await 会被坏 key 的超时（15s/个）拖成分钟级
    let mut futs = Vec::new();
    for key in matched {
        let Some(recipe) = recipes.get(&key.provider).cloned() else {
            continue; // load_keys 已保证 provider 存在，防御一下
        };
        let key = key.clone();
        let id = key.id();
        // 非模型厂商没有探活：它的「能不能用」只能看额度脚本跑不跑得通
        let kind = recipe.kind;
        let client = client.clone();
        futs.push(async move { (id, kind, probe::probe(&client, &recipe, &key).await) });
    }
    let results: Vec<(String, ProviderKind, ProbeResult)> = futures::future::join_all(futs).await;

    if args.has("json") {
        let out: Vec<_> = results
            .iter()
            .map(|(id, kind, r)| {
                json!({
                    "id": id,
                    "kind": kind.as_str(),
                    "health": health_json(&r.health),
                    "balance": balance_json(&r.balance),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    for (id, kind, r) in &results {
        if kind.is_model() {
            match &r.health {
                Health::Live { ms } => println!("{id:<24} ● 可用 {ms}ms"),
                Health::Down { ms, message } => println!("{id:<24} ● 失败 {ms}ms  {message}"),
                Health::Unknown => println!("{id:<24} ● 未配置探活"),
                Health::Checking => println!("{id:<24} … 检查中"),
            }
        } else {
            // 非模型厂商不探活：额度脚本跑通 = 这把 key 现在真能用
            match &r.balance {
                Some(b) if b.error.is_none() => println!("{id:<24} ● 可用"),
                Some(_) => println!("{id:<24} ● 失败"),
                None => println!("{id:<24} ● 未绑定额度脚本"),
            }
        }
        if let Some(b) = &r.balance {
            if let Some(err) = &b.error {
                println!("    额度失败：{err}");
            } else if let Some(lines) = &b.lines {
                for line in lines {
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
    match (&b.error, &b.lines) {
        (Some(err), _) => serde_json::json!({
            "ok": false, "endpoint": b.endpoint,
            "elapsed_ms": b.elapsed_ms, "error": err,
        }),
        (None, Some(lines)) => serde_json::json!({
            "ok": true, "endpoint": b.endpoint,
            "elapsed_ms": b.elapsed_ms,
            "lines": lines,
        }),
        (None, None) => serde_json::Value::Null,
    }
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
    // base_url 和 token 一样过单引号转义：eval $(apim use x) 时防注入
    println!(
        "export OPENAI_API_KEY='{}'",
        key.token.replace('\'', "'\\''")
    );
    println!(
        "export OPENAI_BASE_URL='{}'",
        recipe.base_url.replace('\'', "'\\''")
    );
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
