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

    pub(crate) fn load_keys(&self, recipes: &HashMap<String, Recipe>) -> Result<Vec<KeyEntry>> {
        crate::config::load_keys_from(&self.config_dir, recipes)
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
         \x20 apim provider add <id> --name <名> --base-url <URL> [--health <路径>|none] [--script <脚本路径>|none]\n\
         \x20 apim provider set <id> [--name <名>] [--base-url <URL>] [--health <路径>|none] [--script <脚本路径>|none]\n\
         \x20 apim provider rm <id> [--force]\n\
         密钥（token 一律走 stdin：echo 'KEY' | apim key add ...）：\n\
         \x20 apim key ls [<provider>] [--json]\n\
         \x20 apim key add <provider> <别名> [--group <分组>]        # 已存在则更新 token\n\
         \x20 apim key set <厂商.别名> [--alias <新别名>] [--group <分组>|none]\n\
         \x20 apim key rm <厂商.别名>\n\
         查询：\n\
         \x20 apim status [<provider>] [--json]      # 真实探活+额度（跑绑定脚本）\n\
         \x20 apim copy <厂商.别名> [--base-url]     # 复制密钥 / Base URL\n\
         \x20 apim use <厂商.别名>                   # 输出 export OPENAI_API_KEY=... （shell eval 用）"
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
mod tests {
    use super::*;
    use anyhow::Result;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn temp_ctx(name: &str) -> Ctx {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("apim-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("recipes")).unwrap();
        Ctx {
            recipes_dir: dir.join("recipes"),
            config_dir: dir,
        }
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// 造一个可执行的假额度脚本，返回绝对路径。
    fn fake_script(ctx: &Ctx, name: &str, body: &str) -> String {
        let dir = ctx.config_dir.join("scripts");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    #[tokio::test]
    async fn provider_key_crud_roundtrip() -> Result<()> {
        let ctx = temp_ctx("crud");
        let script = fake_script(&ctx, "relay.sh", "#!/bin/sh\necho '剩余 9/10'\n");

        // 厂商 add：绑脚本
        provider::run(
            &ctx,
            &argv(&[
                "add",
                "relay",
                "--name",
                "中转",
                "--base-url",
                "https://r.example.com/",
                "--health",
                "/v1/models",
                "--script",
                &script,
            ]),
        )
        .await?;
        let recipes = ctx.load_recipes()?;
        let r = recipes.get("relay").expect("recipe saved");
        assert_eq!(r.base_url, "https://r.example.com"); // 去尾斜杠
        assert_eq!(r.health.as_ref().unwrap().url, "{base_url}/v1/models");
        assert!(r.balance.as_ref().unwrap().script().is_some());

        // 密钥 add + 覆盖更新 token
        let args = argv(&["add", "relay", "main"]);
        keys::add(&ctx, &Args::parse(&args)?, "sk-first").unwrap();
        keys::add(&ctx, &Args::parse(&args)?, "sk-second").unwrap();
        let keys = ctx.load_keys(&ctx.load_recipes()?)?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].token, "sk-second");
        let config = fs::read_to_string(ctx.config_dir.join("config.toml")).unwrap();
        assert!(!config.contains("sk-"), "token 不能进 config.toml");

        // key set 改名/分组，key rm 删除
        keys::run(
            &ctx,
            &argv(&["set", "relay.main", "--alias", "work", "--group", "vip"]),
        )
        .await?;
        let keys = ctx.load_keys(&ctx.load_recipes()?)?;
        assert_eq!(keys[0].id(), "relay.work");
        assert_eq!(keys[0].group.as_deref(), Some("vip"));
        keys::run(&ctx, &argv(&["rm", "relay.work"])).await?;
        assert!(ctx.load_keys(&ctx.load_recipes()?)?.is_empty());

        // status：真跑假脚本（无探活配置 → 不发网络请求）
        keys::add(&ctx, &Args::parse(&args)?, "sk-third").unwrap();
        query::status(&ctx, &argv(&["relay"])).await?;

        // provider set 解绑脚本、rm 删除厂商
        provider::run(&ctx, &argv(&["set", "relay", "--script", "none"])).await?;
        assert!(ctx.load_recipes()?["relay"].balance.is_none());
        provider::run(&ctx, &argv(&["rm", "relay", "--force"])).await?;
        assert!(!ctx.load_recipes()?.contains_key("relay"));
        Ok(())
    }

    #[tokio::test]
    async fn provider_add_validates() {
        let ctx = temp_ctx("validate");
        let script = fake_script(&ctx, "x.sh", "#!/bin/sh\ntrue\n");
        // 缺 --name
        assert!(
            provider::run(&ctx, &argv(&["add", "a", "--base-url", "https://x.io"]))
                .await
                .is_err()
        );
        // URL 不合法
        assert!(
            provider::run(
                &ctx,
                &argv(&["add", "a", "--name", "A", "--base-url", "ftp://x.io"])
            )
            .await
            .is_err()
        );
        // 脚本不存在
        assert!(
            provider::run(
                &ctx,
                &argv(&[
                    "add",
                    "a",
                    "--name",
                    "A",
                    "--base-url",
                    "https://x.io",
                    "--script",
                    "/no/such.sh"
                ])
            )
            .await
            .is_err()
        );
        // 重复 id
        provider::run(
            &ctx,
            &argv(&[
                "add",
                "a",
                "--name",
                "A",
                "--base-url",
                "https://x.io",
                "--script",
                &script,
            ]),
        )
        .await
        .unwrap();
        assert!(
            provider::run(
                &ctx,
                &argv(&["add", "a", "--name", "A", "--base-url", "https://x.io"])
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn key_add_rejects_unknown_provider() {
        let ctx = temp_ctx("unknown-provider");
        let args = argv(&["add", "nope", "main"]);
        assert!(keys::add(&ctx, &Args::parse(&args).unwrap(), "sk-x").is_err());
    }

    #[tokio::test]
    async fn provider_rm_protections() {
        let ctx = temp_ctx("rm-protect");
        // 内置不可删
        assert!(
            provider::run(&ctx, &argv(&["rm", "deepseek"]))
                .await
                .is_err()
        );
        // 有密钥时默认拒绝
        provider::run(
            &ctx,
            &argv(&["add", "r2", "--name", "R", "--base-url", "https://x.io"]),
        )
        .await
        .unwrap();
        let args = argv(&["add", "r2", "main"]);
        keys::add(&ctx, &Args::parse(&args).unwrap(), "sk-x").unwrap();
        assert!(provider::run(&ctx, &argv(&["rm", "r2"])).await.is_err());
        provider::run(&ctx, &argv(&["rm", "r2", "--force"]))
            .await
            .unwrap();
        assert!(
            ctx.load_keys(&ctx.load_recipes().unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn args_parse_flags_and_positionals() {
        let argv = [
            "add".to_string(),
            "glm".to_string(),
            "--name".to_string(),
            "GLM".to_string(),
        ];
        let args = Args::parse(&argv).unwrap();
        assert_eq!(args.pos(0), Some("add"));
        assert_eq!(args.pos(1), Some("glm"));
        assert_eq!(args.flag("name"), Some("GLM"));
        assert_eq!(args.flag("group"), None);
    }

    #[test]
    fn args_treat_trailing_flag_as_boolean() {
        // --json/--force/--base-url 无值：出现即真
        let argv = ["status".to_string(), "--json".to_string()];
        let args = Args::parse(&argv).unwrap();
        assert!(args.has("json"));
        assert_eq!(args.pos(0), Some("status"));

        let argv = ["rm".to_string(), "r2".to_string(), "--force".to_string()];
        let args = Args::parse(&argv).unwrap();
        assert!(args.has("force"));
        assert_eq!(args.pos(1), Some("r2"));

        // 布尔 flag 后面跟另一个 flag 时不会被吞成值
        let argv = [
            "ls".to_string(),
            "--json".to_string(),
            "--x".to_string(),
            "v".to_string(),
        ];
        let args = Args::parse(&argv).unwrap();
        assert!(args.has("json"));
        assert_eq!(args.flag("x"), Some("v"));
    }
}
