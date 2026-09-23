//! 厂商整份复制：另存新 YAML + 额度脚本文件副本。
//! TUI 厂商栏 `y` 键与 CLI `apim provider copy` 共用这里的实现。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Recipe;
use super::store::save_user_recipe_to;

/// 复制厂商的结果 = (新 recipe（origin 已指向新 YAML）, 额度脚本落地的新文件)。
/// 脚本项为 None = 没绑外部脚本 / 内联 run / 原文件已丢失（此时绑定原样带走）。
///
/// 整份复制厂商：auth/vars/探活/额度全带走，另存为用户 YAML。
/// - 新 id：显式给则校验（小写字母/数字/-、不与现有冲突）；缺省自动 `<源id>-copy`，
///   被占则 `-copy-2`、`-copy-3`……
/// - 新名称：缺省 `<原名> 副本`。
/// - 绑定了外部额度脚本的，把脚本**复制**成独立文件（fs::copy 连权限位一起带走），
///   新 recipe 指向新文件，之后改脚本互不影响；内联 run / 原文件不存在的不复制。
pub fn duplicate_recipe(
    recipes: &HashMap<String, Recipe>,
    src: &Recipe,
    new_id: Option<&str>,
    new_name: Option<&str>,
    recipes_dir: &Path,
    scripts_dir: &Path,
) -> Result<(Recipe, Option<PathBuf>)> {
    let new_id = resolve_new_id(recipes, &src.id, new_id)?;
    let name = match new_name {
        Some(n) if !n.is_empty() => n.to_string(),
        Some(_) => bail!("新名称不能为空"),
        None => format!("{} 副本", src.name),
    };

    let mut copy = src.clone();
    copy.id = new_id.clone();
    copy.name = name;
    copy.origin = None;

    // 先复制脚本再写 recipe：脚本失败时厂商不会落盘半个状态。
    let script_copy = if let Some(spec) = copy.balance.as_mut()
        && let Some(command) = spec.command.clone()
        && let Some((new_cmd, target)) =
            copy_balance_script(&src.id, &new_id, &command, scripts_dir)?
    {
        copy.balance.as_mut().unwrap().command = Some(new_cmd);
        Some(target)
    } else {
        None
    };

    let path = save_user_recipe_to(recipes_dir, &copy)?;
    copy.origin = Some(path);
    Ok((copy, script_copy))
}

/// 新 id 决策：显式 id 走校验；缺省自动顺延（glm → glm-copy → glm-copy-2 …）。
fn resolve_new_id(
    recipes: &HashMap<String, Recipe>,
    src_id: &str,
    explicit: Option<&str>,
) -> Result<String> {
    match explicit {
        Some(id) => {
            if id.is_empty() {
                bail!("新 ID 不能为空");
            }
            if !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                bail!("新 ID 只能用小写字母、数字、-");
            }
            if recipes.contains_key(id) {
                bail!("厂商 {id} 已存在");
            }
            Ok(id.to_string())
        }
        None => {
            for n in 1.. {
                let cand = if n == 1 {
                    format!("{src_id}-copy")
                } else {
                    format!("{src_id}-copy-{n}")
                };
                if !recipes.contains_key(&cand) {
                    return Ok(cand);
                }
            }
            unreachable!()
        }
    }
}

/// 复制外部额度脚本到 scripts_dir，返回 (新 command 值, 新文件路径)。
/// 命名：原文件名以 `<源id>-` 开头就换成 `<新id>-`（glm-quota.sh → glm-copy-quota.sh），
/// 否则前缀 `<新id>-`。原文件不存在（内联 run 无 command；悬空引用）不复制。
fn copy_balance_script(
    src_id: &str,
    new_id: &str,
    command: &str,
    scripts_dir: &Path,
) -> Result<Option<(String, PathBuf)>> {
    let source = PathBuf::from(crate::probe::expand_tilde(command));
    if !source.is_file() {
        return Ok(None);
    }
    let file = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let target_name = match file.strip_prefix(&format!("{src_id}-")) {
        Some(rest) => format!("{new_id}-{rest}"),
        None => format!("{new_id}-{file}"),
    };
    fs::create_dir_all(scripts_dir).with_context(|| format!("mkdir {}", scripts_dir.display()))?;
    let target = scripts_dir.join(target_name);
    if target.exists() {
        bail!("目标脚本已存在：{}", target.display());
    }
    // fs::copy 连权限位一起复制（可执行位保住，recipe YAML 本身仍是 600）。
    fs::copy(&source, &target).with_context(|| format!("copy → {}", target.display()))?;
    Ok(Some((tilde_path(&target), target)))
}

/// $HOME 下的绝对路径缩写成 ~/ 前缀（与 recipe 里手写脚本的惯例一致）。
fn tilde_path(path: &Path) -> String {
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::super::{Auth, AuthKind, ScriptSpec, load_dir};
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

    #[test]
    fn duplicate_copies_recipe_and_script_as_independent_file() {
        let recipes_dir = temp_dir("full");
        let scripts_dir = temp_dir("full-scripts");
        let src_script = real_script(&scripts_dir, "my-relay-quota.sh", "#!/bin/sh\necho q\n");
        let src = with_script(base_recipe("my-relay"), &src_script, Some(20));
        save_user_recipe_to(&recipes_dir, &src).unwrap();
        let map = one_recipe_map(&src);

        let (recipe, script_copy) =
            duplicate_recipe(&map, &src, None, None, &recipes_dir, &scripts_dir)
                .expect("复制应成功");

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
            crate::probe::expand_tilde(stored_cmd),
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
}
