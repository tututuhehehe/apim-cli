//! 厂商整份复制：另存新 YAML + 额度脚本文件副本。
//! TUI 厂商栏 `y` 键与 CLI `apim provider copy` 共用这里的实现。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Recipe;
use super::store::save_user_recipe_to;

/// 不能复制的厂商 id：内置 `openai`。
///
/// 它的 OAuth 凭据是**全局一份**（`~/.config/apim/openai-oauth.json`）：`o` 登录、密钥表的
/// AUTH 行、`x` 导入 Codex 官方路都只认 id 正好是 `openai` 的那个厂商。复制出来的
/// `openai-copy` 于是成了一个「看着像 OpenAI、却永远登录不上、也没有 AUTH 行」的厂商——
/// 与其让用户踩坑，不如直接拒绝（想加第二个 OpenAI 兼容厂商就用 `provider add` 建新 id，
/// 它本来就不带 OAuth）。「凭据按厂商 id 存」是另一件事，见 `docs/TODO.md`。
pub const UNCOPYABLE_PROVIDER_IDS: &[&str] = &["openai"];

/// 复制前的硬闸：TUI `y` 与 CLI `provider copy` 都经 [`duplicate_recipe`]，规则只写在这里。
pub fn ensure_copyable(src_id: &str) -> Result<()> {
    if UNCOPYABLE_PROVIDER_IDS.contains(&src_id) {
        bail!(
            "{src_id} 不能复制：它的 OAuth 登录是全局凭据，副本没法单独登录。\
             想加第二个 OpenAI 兼容厂商，用 provider add 建一个新 id 的厂商（它不带 OAuth）"
        );
    }
    Ok(())
}

/// 复制厂商的结果 = (新 recipe（origin 已指向新 YAML）, 额度脚本落地的新文件)。
/// 脚本项为 None = 没绑外部脚本 / 内联 run / 原文件已丢失（此时绑定原样带走）。
///
/// 整份复制厂商：auth/vars/探活/额度全带走，另存为用户 YAML。
/// 内置 `openai` 一律拒绝（[`ensure_copyable`]）：它的 OAuth 凭据是全局的，副本登不了。
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
    // 内置 openai 不能复制（OAuth 凭据是全局的，副本登不了），TUI 与 CLI 共用这一道闸
    ensure_copyable(&src.id)?;
    let new_id = resolve_new_id(recipes, &src.id, new_id)?;
    let name = match new_name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
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
            if !super::is_valid_id(id) {
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
    let source = PathBuf::from(crate::util::expand_tilde(command));
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
    // 同名残留（例如删掉副本厂商后遗留的脚本）不报错也不覆盖，顺延计数
    let target = unique_script_target(scripts_dir, &target_name);
    // fs::copy 连权限位一起复制（可执行位保住，recipe YAML 本身仍是 600）。
    fs::copy(&source, &target).with_context(|| format!("copy → {}", target.display()))?;
    Ok(Some((tilde_path(&target), target)))
}

/// 目标文件名被占时在扩展名前顺延（`x-quota.sh` → `x-quota-2.sh`）：
/// 与 resolve_new_id 同策略——绝不覆盖已有文件，也不会因残留文件卡死复制。
fn unique_script_target(scripts_dir: &Path, name: &str) -> PathBuf {
    let candidate = scripts_dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = path.extension().and_then(|s| s.to_str());
    for n in 2.. {
        let cand_name = match ext {
            Some(ext) => format!("{stem}-{n}.{ext}"),
            None => format!("{stem}-{n}"),
        };
        let cand = scripts_dir.join(cand_name);
        if !cand.exists() {
            return cand;
        }
    }
    unreachable!()
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
mod tests;
