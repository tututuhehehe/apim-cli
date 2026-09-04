//! `apim key`：密钥级增删改查。token 永远走 stdin，不进 argv。

use anyhow::{Result, bail};
use serde_json::json;

use super::{Args, Ctx, read_token_stdin};
use crate::config::KeyEntry;

pub(crate) async fn run(ctx: &Ctx, argv: &[String]) -> Result<()> {
    let args = Args::parse(argv)?;
    match args.pos(0) {
        Some("ls") | Some("list") => ls(ctx, &args),
        Some("add") => {
            let token = read_token_stdin()?;
            add(ctx, &args, &token)
        }
        Some("set") => set(ctx, &args),
        Some("rm") | Some("remove") => rm(ctx, &args),
        Some(other) => bail!("未知子命令：key {other}（可用 ls/add/set/rm）"),
        None => bail!("用法：apim key <ls|add|set|rm> ..."),
    }
}

fn ls(ctx: &Ctx, args: &Args) -> Result<()> {
    let recipes = ctx.load_recipes()?;
    let keys = ctx.load_keys(&recipes)?;
    let filter = args.pos(1);
    let matched: Vec<_> = keys
        .iter()
        .filter(|k| filter.is_none_or(|p| k.provider == p))
        .collect();
    if args.has("json") {
        let out: Vec<_> = matched
            .iter()
            .map(|k| {
                json!({
                    "id": k.id(),
                    "provider": k.provider,
                    "alias": k.alias,
                    "group": k.group,
                    "token": k.masked_token(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    for k in matched {
        let group = k.group.as_deref().unwrap_or("—");
        println!("{:<24} 分组 {group:<8} {}", k.id(), k.masked_token());
    }
    Ok(())
}

/// 已存在的 (provider, alias) 走覆盖更新（换 key 就重新 add）。
/// token 由调用方传入（命令行入口从 stdin 读，测试直接给）。
pub(crate) fn add(ctx: &Ctx, args: &Args, token: &str) -> Result<()> {
    let Some(provider) = args.pos(1).map(str::to_string) else {
        bail!("用法：apim key add <provider> <别名> [--group 分组]");
    };
    let Some(alias) = args.pos(2).map(str::to_string) else {
        bail!("用法：apim key add <provider> <别名> [--group 分组]");
    };
    if alias.is_empty() {
        bail!("别名必填");
    }
    if token.is_empty() {
        bail!("密钥必填（stdin）");
    }
    let recipes = ctx.load_recipes()?;
    if !recipes.contains_key(&provider) {
        let known: Vec<_> = {
            let mut ids: Vec<_> = recipes.keys().cloned().collect();
            ids.sort();
            ids
        };
        bail!("厂商 {provider} 不存在（已有：{}）", known.join(", "));
    }

    let mut keys = ctx.load_keys(&recipes)?;
    let group = args.flag("group").map(str::to_string);
    if let Some(existing) = keys
        .iter_mut()
        .find(|k| k.provider == provider && k.alias == alias)
    {
        existing.token = token.to_string();
        if let Some(group) = &group {
            existing.group = Some(group.clone()).filter(|g| !g.is_empty());
        }
        ctx.save_keys(&keys)?;
        println!("已更新密钥 {provider}.{alias}");
    } else {
        keys.push(KeyEntry {
            provider,
            alias,
            group: group.filter(|g| !g.is_empty()),
            token: token.to_string(),
        });
        ctx.save_keys(&keys)?;
        println!("已添加密钥（先 `apim status <厂商>` 验证一下）");
    }
    Ok(())
}

fn set(ctx: &Ctx, args: &Args) -> Result<()> {
    let Some(id) = args.pos(1) else {
        bail!("用法：apim key set <厂商.别名> [--alias 新别名] [--group 分组|none]");
    };
    let Some((provider, alias)) = split_id(id) else {
        bail!("密钥 id 形如 厂商.别名，得到：{id}");
    };
    let recipes = ctx.load_recipes()?;
    let mut keys = ctx.load_keys(&recipes)?;
    let Some(idx) = keys
        .iter()
        .position(|k| k.provider == provider && k.alias == alias)
    else {
        bail!("密钥 {id} 不存在");
    };

    if let Some(new_alias) = args.flag("alias") {
        if new_alias.is_empty() {
            bail!("新别名不能为空");
        }
        let taken = keys
            .iter()
            .any(|k| k.provider == provider && k.alias == new_alias && k.alias != alias);
        if taken {
            bail!("{provider}.{new_alias} 已存在");
        }
        keys[idx].alias = new_alias.to_string();
    }
    if let Some(group) = args.flag("group") {
        keys[idx].group = if group == "none" {
            None
        } else {
            Some(group.to_string())
        };
    }
    let new_id = keys[idx].id();
    ctx.save_keys(&keys)?;
    println!("已更新密钥 {new_id}");
    Ok(())
}

fn rm(ctx: &Ctx, args: &Args) -> Result<()> {
    let Some(id) = args.pos(1) else {
        bail!("用法：apim key rm <厂商.别名>");
    };
    let Some((provider, alias)) = split_id(id) else {
        bail!("密钥 id 形如 厂商.别名，得到：{id}");
    };
    let recipes = ctx.load_recipes()?;
    let mut keys = ctx.load_keys(&recipes)?;
    let before = keys.len();
    keys.retain(|k| !(k.provider == provider && k.alias == alias));
    if keys.len() == before {
        bail!("密钥 {id} 不存在");
    }
    ctx.save_keys(&keys)?;
    println!("已删除密钥 {id}");
    Ok(())
}

/// "glm.main" -> ("glm", "main")，只按第一个 . 切。
fn split_id(id: &str) -> Option<(&str, &str)> {
    let (provider, alias) = id.split_once('.')?;
    if provider.is_empty() || alias.is_empty() {
        return None;
    }
    Some((provider, alias))
}
