//! CLI 子命令：给 AI / 脚本用的机器接口。TUI 管人，CLI 管机器，
//! 共用底层（recipe 加载、config 原子写盘、probe 探活）。

pub(crate) mod keys;
pub(crate) mod provider;
pub(crate) mod query;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::config::KeyEntry;
use crate::recipe::Recipe;

/// 解析结果：位置参数 + `--flag 值`。flag 值里的 `none` 是各命令自定义的哨兵。
#[derive(Debug, Default)]
pub(crate) struct Args {
    positional: Vec<String>,
    flags: HashMap<String, String>,
}

impl Args {
    /// `--flag 值` 成对；后面没有值或值是另一个 `--flag` 时按布尔 flag 处理
    /// （值存空串），用 `has()` 查询（--json/--force/--base-url 这类）。
    pub(crate) fn parse(argv: &[String]) -> Result<Self> {
        let mut out = Args::default();
        let mut iter = argv.iter().peekable();
        while let Some(arg) = iter.next() {
            if let Some(name) = arg.strip_prefix("--") {
                if name.is_empty() {
                    bail!("空 flag");
                }
                let value = match iter.peek() {
                    Some(next) if !next.starts_with("--") => {
                        iter.next().cloned().unwrap_or_default()
                    }
                    _ => String::new(),
                };
                out.flags.insert(name.to_string(), value);
            } else {
                out.positional.push(arg.clone());
            }
        }
        Ok(out)
    }

    pub(crate) fn pos(&self, i: usize) -> Option<&str> {
        self.positional.get(i).map(String::as_str)
    }

    /// 带值 flag；布尔 flag 会拿到 Some("")，取值场景请自行校验非空。
    pub(crate) fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

    /// 布尔 flag 是否出现（--json / --force / --base-url）。
    pub(crate) fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }
}

/// 一个命令运行要碰的两个目录（真实运行=APIM_CONFIG_DIR 或默认，测试=临时目录）。
pub(crate) struct Ctx {
    pub config_dir: PathBuf,
    pub recipes_dir: PathBuf,
}

impl Ctx {
    pub(crate) fn real() -> Self {
        let config_dir = crate::config::config_dir();
        let recipes_dir = config_dir.join("recipes");
        Self {
            config_dir,
            recipes_dir,
        }
    }

    pub(crate) fn load_recipes(&self) -> Result<HashMap<String, Recipe>> {
        crate::recipe::load_recipes_with(&self.recipes_dir)
    }

    /// CLI 走宽松加载：坏条目跳过并告警（详见 load_keys_lenient），
    /// 保证 `key ls` / `key rm` 在配置残缺时还能自救。
    pub(crate) fn load_keys(&self, recipes: &HashMap<String, Recipe>) -> Result<Vec<KeyEntry>> {
        crate::config::load_keys_lenient(&self.config_dir, recipes)
    }

    pub(crate) fn save_keys(&self, keys: &[KeyEntry]) -> Result<()> {
        crate::config::save_keys_to(&self.config_dir, keys)
    }

    pub(crate) fn save_recipe(&self, recipe: &Recipe) -> Result<PathBuf> {
        crate::recipe::save_user_recipe_to(&self.recipes_dir, recipe)
    }
}

pub(crate) async fn run(argv: &[String]) -> Result<()> {
    let ctx = Ctx::real();
    let rest = &argv[1..];
    match argv.first().map(String::as_str) {
        Some("provider") => provider::run(&ctx, rest).await,
        Some("key") | Some("keys") => keys::run(&ctx, rest).await,
        Some("status") => query::status(&ctx, rest).await,
        Some("copy") => query::copy(&ctx, rest),
        Some("use") => query::use_env(&ctx, rest),
        _ => {
            print_help();
            bail!("未知命令，见上方用法");
        }
    }
}

pub(crate) fn print_help() {
    println!(
        "apim — 终端 API 密钥管理器（无参数进 TUI）\n\
         \n\
         厂商：\n\
         \x20 apim provider ls [--json]\n\
         \x20 apim provider add <id> --name <名> --base-url <URL> [--homepage <URL>|none] [--health <路径>|none] [--script <脚本路径>|none]\n\
         \x20 apim provider set <id> [--name <名>] [--base-url <URL>] [--homepage <URL>|none] [--health <路径>|none] [--script <脚本路径>|none]\n\
         \x20 apim provider rm <id> [--force]\n\
         密钥（token 一律走 stdin：echo 'KEY' | apim key add ...）：\n\
         \x20 apim key ls [<provider>] [--json]\n\
         \x20 apim key add <provider> <别名> [--group <分组>]        # 已存在则更新 token\n\
         \x20 apim key set <厂商.别名> [--alias <新别名>] [--group <分组>|none]\n\
         \x20 apim key rm <厂商.别名>\n\
         查询：\n\
         \x20 apim status [<provider>] [--json]      # 真实探活+额度（跑绑定脚本）\n\
         \x20 apim copy <厂商.别名> [--base-url]     # 复制密钥 / Base URL\n\
         \x20 apim use <厂商.别名>                   # 输出 export OPENAI_API_KEY=... （shell eval 用）\n\
         \n\
         别名：apim tui = 无参数；apim keys = apim key；provider list/remove = ls/rm。"
    );
}

/// 从 stdin 读一行当 token（不进 argv，防 ps / shell history）。
pub(crate) fn read_token_stdin() -> Result<String> {
    eprintln!("通过 stdin 输入密钥（例：echo '你的key' | apim key add ...）：");
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("read stdin")?;
    let token = line.trim().to_string();
    if token.is_empty() {
        bail!("stdin 为空，密钥必填");
    }
    Ok(token)
}

#[cfg(test)]
mod tests;
