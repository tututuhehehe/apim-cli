//! `apim provider`：厂商级增删改查。脚本绑定逻辑见 README「余额查询脚本绑定」。

use std::collections::HashMap;

use anyhow::{Result, bail};
use serde_json::json;

use super::{Args, Ctx};
use crate::recipe::{BalanceMode, HttpCall, Recipe, ScriptSpec};

pub(crate) async fn run(ctx: &Ctx, argv: &[String]) -> Result<()> {
    let args = Args::parse(argv)?;
    match args.pos(0) {
        Some("ls") | Some("list") => ls(ctx, &args),
        Some("add") => add(ctx, &args),
        Some("set") => set(ctx, &args),
        Some("rm") | Some("remove") => rm(ctx, &args),
        Some(other) => bail!("未知子命令：provider {other}（可用 ls/add/set/rm）"),
        None => bail!("用法：apim provider <ls|add|set|rm> ..."),
    }
}

fn ls(ctx: &Ctx, args: &Args) -> Result<()> {
    let recipes = ctx.load_recipes()?;
    let keys = ctx.load_keys(&recipes)?;
    let mut ids: Vec<_> = recipes.keys().cloned().collect();
    ids.sort();
    if args.has("json") {
        let out: Vec<_> = ids
            .iter()
            .map(|id| {
                let r = &recipes[id];
                json!({
                    "id": r.id,
                    "name": r.name,
                    "base_url": r.base_url,
                    "homepage": r.homepage,
                    "health": r.health.as_ref().map(|h| h.url.clone()),
                    "balance": balance_json(&r.balance),
                    "origin": r.origin.as_ref().map(|p| p.display().to_string()),
                    "keys": keys.iter().filter(|k| &k.provider == id).map(|k| k.id()).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    for id in ids {
        let r = &recipes[&id];
        let n_keys = keys.iter().filter(|k| k.provider == id).count();
        let balance = match &r.balance {
            Some(BalanceMode::Script(s)) => {
                format!("script {}", s.command.as_deref().unwrap_or("(内联)"))
            }
            Some(BalanceMode::Http(_)) => "http".into(),
            None => "—".into(),
        };
        let origin = if r.origin.is_none() { " [内置]" } else { "" };
        println!("{id:<14} {:<18} {}{origin}", r.name, r.base_url);
        let homepage = r.homepage.as_deref().unwrap_or("—");
        println!(
            "{:<14} 额度 {balance} · 密钥 {n_keys} · 主页 {homepage}",
            ""
        );
    }
    Ok(())
}

fn balance_json(balance: &Option<BalanceMode>) -> serde_json::Value {
    match balance {
        Some(BalanceMode::Script(s)) => json!({
            "kind": "script",
            "command": s.command,
            "run": s.run.is_some(),
            "timeout_secs": s.timeout_secs,
        }),
        Some(BalanceMode::Http(_)) => json!({"kind": "http"}),
        None => serde_json::Value::Null,
    }
}

fn add(ctx: &Ctx, args: &Args) -> Result<()> {
    let Some(id) = args.pos(1).map(str::to_string) else {
        bail!(
            "用法：apim provider add <id> --name <名> --base-url <URL> [--homepage <URL>|none] [--health 路径] [--script 脚本路径]"
        );
    };
    let Some(name) = args.flag("name").map(str::to_string) else {
        bail!("--name 必填");
    };
    if name.is_empty() {
        // Args 会把「--name 后面紧跟另一个 flag」的值解析成空串，这里拦住
        bail!("--name 必填（值为空）");
    }
    let Some(base_url) = args.flag("base-url").map(str::to_string) else {
        bail!("--base-url 必填");
    };

    let recipes = ctx.load_recipes()?;
    if recipes.contains_key(&id) {
        bail!("厂商 {id} 已存在，改配置用 provider set");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("ID 只能用小写字母、数字、-");
    }
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        bail!("Base URL 要以 http(s):// 开头");
    }

    let mut recipe = Recipe {
        id: id.clone(),
        name,
        base_url,
        homepage: homepage_opt(args)?,
        supports_groups: false,
        vars: HashMap::new(),
        auth: Default::default(),
        health: health_call(args)?,
        balance: script_spec(args)?,
        origin: None,
    };
    recipe.normalize();
    let path = ctx.save_recipe(&recipe)?;
    println!("已添加厂商 {id}（{}）", path.display());
    Ok(())
}

fn set(ctx: &Ctx, args: &Args) -> Result<()> {
    let Some(id) = args.pos(1) else {
        bail!(
            "用法：apim provider set <id> [--name 名] [--base-url URL] [--homepage URL|none] [--health 路径|none] [--script 路径|none]"
        );
    };
    let mut recipes = ctx.load_recipes()?;
    let Some(recipe) = recipes.get_mut(id) else {
        bail!("厂商 {id} 不存在");
    };

    if let Some(name) = args.flag("name") {
        if name.is_empty() {
            bail!("--name 不能为空");
        }
        recipe.name = name.to_string();
    }
    if let Some(base_url) = args.flag("base-url") {
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            bail!("Base URL 要以 http(s):// 开头");
        }
        recipe.base_url = base_url.to_string();
    }
    if args.flag("homepage").is_some() {
        recipe.homepage = homepage_opt(args)?;
    }
    if args.flag("health").is_some() {
        recipe.health = health_call(args)?;
    }
    if args.flag("script").is_some() {
        // CLI set 是显式指令：--script 直接整体替换额度绑定（含解绑），
        // 不做 TUI 表单那套「未改保留」。
        recipe.balance = script_spec(args)?;
    }
    recipe.normalize();
    let path = ctx.save_recipe(recipe)?;
    println!("已更新厂商 {id}（{}）", path.display());
    Ok(())
}

fn rm(ctx: &Ctx, args: &Args) -> Result<()> {
    let Some(id) = args.pos(1) else {
        bail!("用法：apim provider rm <id> [--force]");
    };
    let recipes = ctx.load_recipes()?;
    let Some(recipe) = recipes.get(id) else {
        bail!("厂商 {id} 不存在");
    };
    let Some(origin) = recipe.origin.clone() else {
        bail!("内置厂商不可删除（可 set 覆盖）");
    };

    let mut keys = ctx.load_keys(&recipes)?;
    let mine: Vec<&str> = keys
        .iter()
        .filter(|k| k.provider == id)
        .map(|k| k.alias.as_str())
        .collect();
    if !mine.is_empty() && !args.has("force") {
        let list = mine.join(", ");
        bail!("厂商 {id} 下还有密钥（{list}），先删除或用 --force 连带删除");
    }
    crate::recipe::delete_user_recipe(&origin)?;
    if !mine.is_empty() {
        keys.retain(|k| k.provider != id);
        ctx.save_keys(&keys)?;
    }
    println!("已删除厂商 {id}");
    Ok(())
}

/// --homepage 的值 → Option；none / 空串 / 缺省 = 未配置。
fn homepage_opt(args: &Args) -> Result<Option<String>> {
    let Some(url) = args.flag("homepage") else {
        return Ok(None);
    };
    if url.is_empty() || url == "none" {
        return Ok(None);
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("主页 URL 要以 http(s):// 开头");
    }
    Ok(Some(url.to_string()))
}

/// --health 的值 → HttpCall；none / 空串 / 缺省 = 不探活。
/// 空串也按 none 处理，避免 `{base_url}/` 向根路径发鉴权请求。
fn health_call(args: &Args) -> Result<Option<HttpCall>> {
    let Some(path) = args.flag("health") else {
        return Ok(None);
    };
    if path.is_empty() || path == "none" {
        return Ok(None);
    }
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    Ok(Some(HttpCall::get(format!("{{base_url}}{path}"))))
}

/// --script 的值 → 额度绑定；none / 空串 / 缺省 = 不绑（解绑）。
/// 空串与 none 同义，避免误报「脚本不存在」。
fn script_spec(args: &Args) -> Result<Option<BalanceMode>> {
    let Some(path) = args.flag("script") else {
        return Ok(None);
    };
    if path.is_empty() || path == "none" {
        return Ok(None);
    }
    let expanded = crate::probe::expand_tilde(path);
    if !std::path::Path::new(&expanded).is_file() {
        bail!("脚本不存在：{path}");
    }
    Ok(Some(BalanceMode::Script(ScriptSpec {
        command: Some(path.to_string()),
        ..ScriptSpec::default()
    })))
}
