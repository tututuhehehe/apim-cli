//! CLI 层测试：Args 解析、provider/key CRUD、status/copy/use、宽松加载。

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

    // 厂商 add：绑脚本 + 主页
    provider::run(
        &ctx,
        &argv(&[
            "add",
            "relay",
            "--name",
            "中转",
            "--base-url",
            "https://r.example.com/",
            "--homepage",
            "https://console.example.com",
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
    assert_eq!(r.homepage.as_deref(), Some("https://console.example.com"));
    assert_eq!(r.health.as_ref().unwrap().url, "{base_url}/v1/models");
    assert!(r.balance.as_ref().unwrap().command.is_some());

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

    // provider set 改主页 / 解绑脚本、rm 删除厂商
    provider::run(
        &ctx,
        &argv(&["set", "relay", "--homepage", "none", "--script", "none"]),
    )
    .await?;
    let r = &ctx.load_recipes()?["relay"];
    assert!(r.homepage.is_none());
    assert!(r.balance.is_none());
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
async fn non_model_provider_kind_rules() -> Result<()> {
    let ctx = temp_ctx("non-model");
    let script = fake_script(&ctx, "deepl.sh", "#!/bin/sh\necho '剩余 42 万字符'\n");

    // --kind 两种写法都认（non-model / non_model）
    provider::run(
        &ctx,
        &argv(&[
            "add",
            "deepl",
            "--name",
            "DeepL",
            "--base-url",
            "https://api-free.deepl.com",
            "--kind",
            "non-model",
            "--script",
            &script,
        ]),
    )
    .await?;
    let recipes = ctx.load_recipes()?;
    let deepl = &recipes["deepl"];
    assert!(!deepl.is_model());
    assert!(deepl.health.is_none(), "非模型厂商不落探活配置");
    assert!(deepl.balance.is_some());

    // 缺省 = 模型；--kind 写错直接报错（不猜）
    provider::run(
        &ctx,
        &argv(&[
            "add",
            "relay",
            "--name",
            "中转",
            "--base-url",
            "https://r.io",
        ]),
    )
    .await?;
    assert!(ctx.load_recipes()?["relay"].is_model());
    assert!(
        provider::run(
            &ctx,
            &argv(&[
                "add",
                "x",
                "--name",
                "X",
                "--base-url",
                "https://x.io",
                "--kind",
                "service"
            ])
        )
        .await
        .is_err()
    );

    // 非模型厂商给 --health 是矛盾指令，两种路径都报错（不能默默丢掉用户意图）
    assert!(
        provider::run(
            &ctx,
            &argv(&[
                "add",
                "y",
                "--name",
                "Y",
                "--base-url",
                "https://y.io",
                "--kind",
                "non-model",
                "--health",
                "/usage"
            ])
        )
        .await
        .is_err()
    );
    assert!(
        provider::run(&ctx, &argv(&["set", "deepl", "--health", "/usage"]))
            .await
            .is_err()
    );
    // 类型只能在创建时定
    assert!(
        provider::run(&ctx, &argv(&["set", "deepl", "--kind", "model"]))
            .await
            .is_err()
    );
    assert!(
        provider::run(&ctx, &argv(&["set", "relay", "--kind", "non-model"]))
            .await
            .is_err()
    );

    // status：非模型厂商不探活，只看额度脚本（按脚本成败报状态）
    keys::add(
        &ctx,
        &Args::parse(&argv(&["add", "deepl", "main"]))?,
        "sk-deepl",
    )
    .unwrap();
    query::status(&ctx, &argv(&["deepl"])).await?;
    query::status(&ctx, &argv(&["deepl", "--json"])).await?;

    // 脚本解绑后仍然能跑（报「未绑定额度脚本」，不发探活请求）
    provider::run(&ctx, &argv(&["set", "deepl", "--script", "none"])).await?;
    query::status(&ctx, &argv(&["deepl"])).await?;
    Ok(())
}

/// 手写 YAML 把类型改成 `non_model` 但留着原来的 `health:`（UI/CLI 都不让改类型，
/// 手改文件就是现实中的「换类型」）：apim 不该再拿密钥去发探活请求，
/// `provider ls --json` 也不该报出探活（机器接口不能说谎）。
#[tokio::test]
async fn hand_written_non_model_recipe_ignores_its_health_block() -> Result<()> {
    let ctx = temp_ctx("non-model-hygiene");
    fs::write(
        ctx.recipes_dir.join("legacy.yaml"),
        "id: legacy\nname: 旧中转\nkind: non_model\nbase_url: 'http://127.0.0.1:1'\nhealth: {url: '{base_url}/models'}\n",
    )
    .unwrap();
    let recipes = ctx.load_recipes()?;
    let legacy = &recipes["legacy"];
    assert!(legacy.health.is_some(), "YAML 里的字段原样保留");
    assert!(legacy.health_call().is_none(), "但不生效");

    // JSON：非模型的 health 是 null，模型厂商照旧给值
    let json = provider::provider_json(legacy, &[]);
    assert_eq!(json["kind"], "non_model");
    assert!(json["health"].is_null(), "{json}");
    let mut as_model = legacy.clone();
    as_model.kind = crate::recipe::ProviderKind::Model;
    assert_eq!(
        provider::provider_json(&as_model, &[])["health"],
        "{base_url}/models"
    );

    // 真实跑一遍：status 对非模型只看额度脚本，不发探活（发的话这里会等超时/报错）
    keys::add(
        &ctx,
        &Args::parse(&argv(&["add", "legacy", "main"]))?,
        "sk-legacy",
    )
    .unwrap();
    query::status(&ctx, &argv(&["legacy"])).await?;
    query::status(&ctx, &argv(&["legacy", "--json"])).await?;
    Ok(())
}

#[tokio::test]
async fn provider_add_set_reject_empty_name() {
    let ctx = temp_ctx("empty-name");
    // `--name --base-url x` 会被解析成 name=""，必须报错而不是落盘空名
    assert!(
        provider::run(
            &ctx,
            &argv(&["add", "a", "--name", "--base-url", "https://x.io"])
        )
        .await
        .is_err()
    );
    // 显式空串同理
    assert!(
        provider::run(
            &ctx,
            &argv(&["add", "a", "--name", "", "--base-url", "https://x.io"])
        )
        .await
        .is_err()
    );
    // set 也不允许把名字改成空
    provider::run(
        &ctx,
        &argv(&["add", "a", "--name", "A", "--base-url", "https://x.io"]),
    )
    .await
    .unwrap();
    assert!(
        provider::run(&ctx, &argv(&["set", "a", "--name", ""]))
            .await
            .is_err()
    );
    let r = ctx.load_recipes().unwrap()["a"].clone();
    assert_eq!(r.name, "A", "set 失败不能改坏已有配置");
}

#[tokio::test]
async fn provider_health_script_empty_means_none() -> Result<()> {
    let ctx = temp_ctx("empty-none");
    // --health "" / --script "" 与 none 同义：不探活、不绑脚本
    provider::run(
        &ctx,
        &argv(&[
            "add",
            "a",
            "--name",
            "A",
            "--base-url",
            "https://x.io",
            "--health",
            "",
            "--script",
            "",
        ]),
    )
    .await?;
    let r = ctx.load_recipes()?["a"].clone();
    assert!(r.health.is_none(), "空 health 不能变成探活根路径");
    assert!(r.balance.is_none(), "空 script 与 none 同义");
    Ok(())
}

#[tokio::test]
async fn key_add_rejects_unknown_provider() {
    let ctx = temp_ctx("unknown-provider");
    let args = argv(&["add", "nope", "main"]);
    assert!(keys::add(&ctx, &Args::parse(&args).unwrap(), "sk-x").is_err());
}

#[tokio::test]
async fn status_unknown_provider_bails() {
    let ctx = temp_ctx("status-unknown");
    // 拼错的厂商名报错（退出码非 0），而不是「还没有密钥」
    assert!(query::status(&ctx, &argv(&["no-such"])).await.is_err());
}

#[tokio::test]
async fn auth_openai_rejects_an_unknown_subcommand() {
    let ctx = temp_ctx("auth-unknown-subcommand");
    // 拼错的子命令要非零退出（clap 式机器接口），不能只打一行用法就 Ok
    assert!(auth::run(&ctx, &argv(&["openai", "loginn"])).await.is_err());
    assert!(auth::run(&ctx, &argv(&["openai"])).await.is_err());
    // status/logout 是纯本地读写，不联网：没凭据时 status 仍然成功
    assert!(auth::run(&ctx, &argv(&["openai", "status"])).await.is_ok());
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

/// provider copy：整份复制厂商，额度脚本复制成独立文件（不引用原脚本）。
#[tokio::test]
async fn provider_copy_duplicates_recipe_and_script() -> Result<()> {
    let ctx = temp_ctx("copy");
    let body = "#!/bin/sh\necho '剩余 9/10'\n";
    let script = fake_script(&ctx, "relay-quota.sh", body);
    provider::run(
        &ctx,
        &argv(&[
            "add",
            "relay",
            "--name",
            "中转",
            "--base-url",
            "https://r.example.com",
            "--script",
            &script,
        ]),
    )
    .await?;

    // 自动新 id：relay → relay-copy，脚本文件同名换前缀
    provider::run(&ctx, &argv(&["copy", "relay"])).await?;
    let recipes = ctx.load_recipes()?;
    let copied = recipes.get("relay-copy").expect("自动 id 应为 relay-copy");
    assert_eq!(copied.name, "中转 副本");
    let cmd = copied.balance.as_ref().unwrap().command.clone().unwrap();
    assert_ne!(cmd, script, "不能引用原脚本");
    assert!(
        cmd.ends_with("relay-copy-quota.sh"),
        "脚本名跟随新 id: {cmd}"
    );
    let cmd_file = PathBuf::from(crate::util::expand_tilde(&cmd));
    assert!(cmd_file.is_file(), "新脚本应落盘: {cmd}");
    assert_eq!(fs::read_to_string(&cmd_file)?, body);
    // 原厂商与原脚本不动
    assert_eq!(
        recipes["relay"]
            .balance
            .as_ref()
            .unwrap()
            .command
            .as_deref(),
        Some(script.as_str())
    );
    assert_eq!(fs::read_to_string(&script)?, body);

    // 显式新 id + --name；再复制自动顺延 -copy-2
    provider::run(&ctx, &argv(&["copy", "relay", "backup", "--name", "备份"])).await?;
    provider::run(&ctx, &argv(&["copy", "relay"])).await?;
    let recipes = ctx.load_recipes()?;
    assert_eq!(recipes["backup"].name, "备份");
    assert!(recipes.contains_key("relay-copy-2"), "自动顺延 -copy-2");

    // 新 id 冲突 / 源不存在 / 空名都拦截
    assert!(
        provider::run(&ctx, &argv(&["copy", "relay", "backup"]))
            .await
            .is_err()
    );
    assert!(provider::run(&ctx, &argv(&["copy", "nope"])).await.is_err());
    assert!(
        provider::run(&ctx, &argv(&["copy", "relay", "ok3", "--name", ""]))
            .await
            .is_err()
    );
    Ok(())
}

/// 坏配置自救：CLI 的宽松加载跳过坏条目，严格版（TUI 用）整体报错。
#[tokio::test]
async fn cli_load_keys_is_lenient_for_broken_entries() -> Result<()> {
    let ctx = temp_ctx("lenient");
    provider::run(
        &ctx,
        &argv(&["add", "good", "--name", "G", "--base-url", "https://x.io"]),
    )
    .await?;
    keys::add(
        &ctx,
        &Args::parse(&argv(&["add", "good", "main"]))?,
        "sk-ok",
    )
    .unwrap();

    // 手写坏条目：ghost 没有 recipe；good.lost 有 recipe 但 secrets 缺 token
    fs::write(
        ctx.config_dir.join("config.toml"),
        "[[keys]]\nprovider = \"good\"\nalias = \"main\"\n\n\
         [[keys]]\nprovider = \"ghost\"\nalias = \"x\"\n\n\
         [[keys]]\nprovider = \"good\"\nalias = \"lost\"\n",
    )?;
    fs::write(
        ctx.config_dir.join("secrets.toml"),
        "[tokens]\n\"good.main\" = \"sk-ok\"\n",
    )?;

    let recipes = ctx.load_recipes()?;
    // 严格版（TUI 路径）在同样配置下应失败
    assert!(
        crate::config::load_keys_from(&ctx.config_dir, &recipes).is_err(),
        "严格版保持整体失败，TUI 行为不变"
    );
    // CLI 走宽松版：只返回好条目
    let keys = ctx.load_keys(&recipes)?;
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].id(), "good.main");
    assert_eq!(keys[0].token, "sk-ok");

    // 下次写盘顺带清掉坏条目（自救闭环）
    keys::run(&ctx, &argv(&["rm", "good.main"])).await?;
    let config = fs::read_to_string(ctx.config_dir.join("config.toml")).unwrap();
    assert!(!config.contains("ghost"), "写盘后坏条目应被移除");
    assert!(!config.contains("lost"), "写盘后坏条目应被移除");
    Ok(())
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

/// 锁定空值歧义成因：flag 后紧跟另一个 flag 时值存空串（各命令需自行校验非空）。
#[test]
fn args_flag_followed_by_flag_yields_empty_value() {
    let argv = argv(&["add", "relay", "--name", "--base-url", "https://x.io"]);
    let args = Args::parse(&argv).unwrap();
    assert_eq!(args.flag("name"), Some(""));
    assert_eq!(args.flag("base-url"), Some("https://x.io"));
}
