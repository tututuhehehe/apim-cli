use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::super::{Auth, AuthKind, ProviderKind, ScriptSpec, load_dir};
use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("apim-dup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 造一个最小可用 recipe；脚本与名称由用例自行覆盖。
fn base_recipe(id: &str) -> Recipe {
    Recipe {
        id: id.into(),
        name: format!("{id} 站"),
        kind: ProviderKind::Model,
        base_url: "https://relay.example.com/v1".into(),
        homepage: Some("https://console.example.com".into()),
        models_url: None,
        supports_groups: false,
        vars: HashMap::from([("access_token".to_string(), "at-1".to_string())]),
        auth: Auth {
            kind: AuthKind::Bearer,
            header: None,
            prefix: None,
            query_param: None,
        },
        health: None,
        balance: None,
        origin: None,
    }
}

/// 在 scripts_dir 里造一个可执行的源脚本，返回绝对路径。
fn real_script(scripts_dir: &Path, name: &str, body: &str) -> PathBuf {
    fs::create_dir_all(scripts_dir).unwrap();
    let path = scripts_dir.join(name);
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn with_script(mut r: Recipe, script: &Path, timeout_secs: Option<u64>) -> Recipe {
    r.balance = Some(ScriptSpec {
        command: Some(script.display().to_string()),
        run: None,
        shell: None,
        timeout_secs,
    });
    r
}

fn one_recipe_map(recipe: &Recipe) -> HashMap<String, Recipe> {
    HashMap::from([(recipe.id.clone(), recipe.clone())])
}

/// 整份复制连类型一起带走（非模型厂商的副本仍是非模型）。
#[test]
fn duplicate_keeps_the_provider_kind() {
    let recipes_dir = temp_dir("kind");
    let scripts_dir = temp_dir("kind-scripts");
    let mut src = base_recipe("deepl");
    src.kind = ProviderKind::NonModel;
    src.health = None;
    let map = one_recipe_map(&src);

    let (recipe, _) = duplicate_recipe(&map, &src, None, None, &recipes_dir, &scripts_dir).unwrap();
    assert_eq!(recipe.kind, ProviderKind::NonModel);
}

#[test]
fn duplicate_copies_recipe_and_script_as_independent_file() {
    let recipes_dir = temp_dir("full");
    let scripts_dir = temp_dir("full-scripts");
    let src_script = real_script(&scripts_dir, "my-relay-quota.sh", "#!/bin/sh\necho q\n");
    let src = with_script(base_recipe("my-relay"), &src_script, Some(20));
    save_user_recipe_to(&recipes_dir, &src).unwrap();
    let map = one_recipe_map(&src);

    let (recipe, script_copy) =
        duplicate_recipe(&map, &src, None, None, &recipes_dir, &scripts_dir).expect("复制应成功");

    // 新 id / 默认名称 / origin 指向新 YAML；vars / timeout / homepage 全带走
    assert_eq!(recipe.id, "my-relay-copy");
    assert_eq!(recipe.name, "my-relay 站 副本");
    assert_eq!(
        recipe.origin.as_deref(),
        Some(recipes_dir.join("my-relay-copy.yaml").as_path())
    );
    assert_eq!(
        recipe.vars.get("access_token").map(String::as_str),
        Some("at-1")
    );
    assert_eq!(
        recipe.homepage.as_deref(),
        Some("https://console.example.com")
    );

    // 脚本被复制成独立文件（命名跟随新 id），内容与可执行位保留
    let copied = script_copy.as_ref().expect("脚本应被复制");
    assert_eq!(*copied, scripts_dir.join("my-relay-copy-quota.sh"));
    assert_eq!(fs::read_to_string(copied).unwrap(), "#!/bin/sh\necho q\n");
    let mode = fs::metadata(copied).unwrap().permissions().mode();
    assert!(mode & 0o111 != 0, "可执行位要保留: {mode:o}");
    let balance = recipe.balance.as_ref().unwrap();
    let stored_cmd = balance.command.as_deref().unwrap();
    assert_eq!(
        crate::util::expand_tilde(stored_cmd),
        copied.display().to_string(),
        "存储值（可能是 ~/ 缩写）展开后应指向新脚本文件"
    );
    assert_eq!(balance.timeout_secs, Some(20));

    // 原厂商与原脚本分毫未动
    let mut reloaded = HashMap::new();
    load_dir(&recipes_dir, &mut reloaded).unwrap();
    assert_eq!(reloaded.len(), 2, "新 YAML 落盘且原 YAML 还在");
    assert_eq!(
        reloaded["my-relay"].balance.as_ref().unwrap().command,
        Some(src_script.display().to_string())
    );

    // 再复制一次：自动顺延 -copy-2
    let mut map2 = map.clone();
    map2.insert("my-relay-copy".into(), recipe.clone());
    let (second, second_script) =
        duplicate_recipe(&map2, &src, None, None, &recipes_dir, &scripts_dir).unwrap();
    assert_eq!(second.id, "my-relay-copy-2");
    assert_eq!(
        second_script.as_ref().unwrap(),
        &scripts_dir.join("my-relay-copy-2-quota.sh")
    );
}

#[test]
fn duplicate_explicit_id_and_name() {
    let recipes_dir = temp_dir("explicit");
    let scripts_dir = temp_dir("explicit-scripts");
    let src = base_recipe("my-relay"); // 无脚本：验证「没有就不复制」
    let map = one_recipe_map(&src);
    let (recipe, script_copy) = duplicate_recipe(
        &map,
        &src,
        Some("backup"),
        Some("备份"),
        &recipes_dir,
        &scripts_dir,
    )
    .unwrap();
    assert_eq!(recipe.id, "backup");
    assert_eq!(recipe.name, "备份");
    assert!(script_copy.is_none(), "没绑脚本就不复制");
    assert!(recipe.balance.is_none());

    // 冲突 / 非法 id / 空名都要拦
    assert!(
        duplicate_recipe(
            &map,
            &src,
            Some("my-relay"),
            None,
            &recipes_dir,
            &scripts_dir
        )
        .is_err()
    );
    assert!(
        duplicate_recipe(&map, &src, Some("Bad_ID"), None, &recipes_dir, &scripts_dir).is_err()
    );
    assert!(
        duplicate_recipe(
            &map,
            &src,
            Some("ok2"),
            Some(""),
            &recipes_dir,
            &scripts_dir
        )
        .is_err()
    );
}

#[test]
fn duplicate_inline_run_and_dangling_command_need_no_file() {
    let recipes_dir = temp_dir("inline");
    let scripts_dir = temp_dir("inline-scripts");

    // 内联 run：内容就在 YAML 里，没有文件要复制，原样带走
    let mut inline = base_recipe("my-relay");
    inline.balance = Some(ScriptSpec {
        command: None,
        run: Some("echo 1".into()),
        shell: Some("/bin/zsh".into()),
        timeout_secs: None,
    });
    let (recipe, script_copy) = duplicate_recipe(
        &one_recipe_map(&inline),
        &inline,
        None,
        None,
        &recipes_dir,
        &scripts_dir,
    )
    .unwrap();
    assert!(script_copy.is_none());
    let spec = recipe.balance.as_ref().unwrap();
    assert_eq!(spec.command, None);
    assert_eq!(spec.run.as_deref(), Some("echo 1"));
    assert_eq!(spec.shell.as_deref(), Some("/bin/zsh"));

    // 悬空 command（脚本文件不存在）：不报错，引用原样保留
    let dangling = with_script(base_recipe("my-relay"), Path::new("/no/such/file.sh"), None);
    let (recipe, script_copy) = duplicate_recipe(
        &one_recipe_map(&dangling),
        &dangling,
        Some("d2"),
        None,
        &recipes_dir,
        &scripts_dir,
    )
    .unwrap();
    assert!(script_copy.is_none());
    assert_eq!(
        recipe.balance.as_ref().unwrap().command.as_deref(),
        Some("/no/such/file.sh")
    );
    // scripts_dir 即使被 mkdir 也不能凭空多出文件
    if scripts_dir.exists() {
        assert!(fs::read_dir(&scripts_dir).unwrap().count() == 0);
    }
}

/// 目标脚本名被占（例如删掉副本厂商后遗留的孤儿脚本）：顺延计数，不报错也不覆盖。
#[test]
fn duplicate_suffixes_script_name_when_target_is_taken() {
    let recipes_dir = temp_dir("suffix");
    let scripts_dir = temp_dir("suffix-scripts");
    let src_script = real_script(&scripts_dir, "my-relay-quota.sh", "#!/bin/sh\necho src\n");
    // 预置同名孤儿：内容不能被覆盖
    let orphan = scripts_dir.join("my-relay-copy-quota.sh");
    fs::write(&orphan, "#!/bin/sh\necho orphan\n").unwrap();
    let src = with_script(base_recipe("my-relay"), &src_script, None);
    let (recipe, script_copy) = duplicate_recipe(
        &one_recipe_map(&src),
        &src,
        None,
        None,
        &recipes_dir,
        &scripts_dir,
    )
    .unwrap();

    let copied = script_copy.expect("应顺延出新名字并完成复制");
    assert_eq!(copied, scripts_dir.join("my-relay-copy-quota-2.sh"));
    assert_eq!(
        fs::read_to_string(&copied).unwrap(),
        "#!/bin/sh\necho src\n"
    );
    assert_eq!(
        fs::read_to_string(&orphan).unwrap(),
        "#!/bin/sh\necho orphan\n",
        "已有文件绝不能覆盖"
    );
    assert!(
        recipe
            .balance
            .as_ref()
            .unwrap()
            .command
            .as_deref()
            .unwrap()
            .ends_with("my-relay-copy-quota-2.sh")
    );
}

/// 名称两头空白被 trim；纯空白视为空名报错。
#[test]
fn duplicate_trims_name_and_rejects_blank() {
    let recipes_dir = temp_dir("trim");
    let scripts_dir = temp_dir("trim-scripts");
    let src = base_recipe("my-relay");
    let map = one_recipe_map(&src);
    let (recipe, _) = duplicate_recipe(
        &map,
        &src,
        Some("n1"),
        Some("  备份  "),
        &recipes_dir,
        &scripts_dir,
    )
    .unwrap();
    assert_eq!(recipe.name, "备份");
    assert!(
        duplicate_recipe(
            &map,
            &src,
            Some("n2"),
            Some("   "),
            &recipes_dir,
            &scripts_dir
        )
        .is_err(),
        "纯空白名称应报错"
    );
}

/// 内置 `openai` 不能复制：它的 OAuth 凭据是全局一份，副本永远登录不上。
/// 闸在 `duplicate_recipe` 里，所以 TUI `y` 与 CLI `provider copy` 都拦得住。
#[test]
fn openai_cannot_be_duplicated() {
    let recipes_dir = temp_dir("openai");
    let scripts_dir = temp_dir("openai-scripts");
    let src = base_recipe("openai");
    let map = one_recipe_map(&src);

    let err = duplicate_recipe(&map, &src, None, None, &recipes_dir, &scripts_dir)
        .unwrap_err()
        .to_string();
    assert!(err.contains("不能复制"), "{err}");
    assert!(
        err.contains("provider add"),
        "要告诉用户怎么建第二个：{err}"
    );
    // 显式 id 也拦（不能绕）
    assert!(
        duplicate_recipe(
            &map,
            &src,
            Some("my-openai"),
            None,
            &recipes_dir,
            &scripts_dir
        )
        .is_err()
    );
    // 什么都没写下去
    assert!(!recipes_dir.join("openai-copy.yaml").exists());

    // 其它厂商（包括其它内置）不受影响
    let other = base_recipe("deepseek");
    let map = one_recipe_map(&other);
    assert!(duplicate_recipe(&map, &other, None, None, &recipes_dir, &scripts_dir).is_ok());
}

/// 闸是个可单独调的纯函数：UI/CLI 想在按之前就拦住也能用。
#[test]
fn ensure_copyable_only_blocks_the_reserved_ids() {
    assert!(ensure_copyable("openai").is_err());
    assert!(ensure_copyable("openai-copy").is_ok());
    assert!(ensure_copyable("deepseek").is_ok());
    assert!(ensure_copyable("my-relay").is_ok());
}
