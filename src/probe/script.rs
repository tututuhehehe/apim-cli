//! 额度查询的脚本逃生舱：跑 spec 指定的脚本，stdout 逐行进额度面板。密钥只经 env 注入。

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{BalanceSnapshot, compact_error, elapsed_ms, truncate};
use crate::config::KeyEntry;
use crate::recipe::{BalanceView, Recipe, ScriptSpec};

/// stdout 最多保留的行数，超出截断（防野脚本刷屏）。
const MAX_LINES: usize = 50;
const TRUNCATED_MARKER: &str = "…（输出已截断）";

pub(super) async fn run_script_balance(
    recipe: &Recipe,
    spec: &ScriptSpec,
    key: &KeyEntry,
) -> BalanceSnapshot {
    let started = Instant::now();
    let endpoint = format!("script {}", recipe.id);
    match exec_script_balance(recipe, spec, key).await {
        Ok(view) => BalanceSnapshot {
            view: Some(view),
            endpoint,
            status: None,
            elapsed_ms: elapsed_ms(started),
            error: None,
        },
        Err(err) => BalanceSnapshot {
            view: None,
            endpoint,
            status: None,
            elapsed_ms: elapsed_ms(started),
            error: Some(compact_error(&err)),
        },
    }
}

async fn exec_script_balance(
    recipe: &Recipe,
    spec: &ScriptSpec,
    key: &KeyEntry,
) -> Result<BalanceView> {
    let mut cmd = match (spec.command.as_deref(), spec.run.as_deref()) {
        (Some(path), None) => tokio::process::Command::new(expand_tilde(path)),
        (None, Some(script)) => {
            let mut cmd = tokio::process::Command::new(spec.shell.as_deref().unwrap_or("/bin/sh"));
            cmd.arg("-c").arg(script);
            cmd
        }
        _ => anyhow::bail!("balance.kind=script 需要 command 或 run 二选一"),
    };
    cmd.env("APIM_TOKEN", &key.token)
        .env("APIM_BASE_URL", &recipe.base_url)
        .env("APIM_ALIAS", &key.alias)
        .env("APIM_PROVIDER", &recipe.id)
        .envs(script_env_vars(recipe))
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let timeout = Duration::from_secs(spec.timeout_secs.unwrap_or(15));
    let output = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("脚本超时（{}s）", timeout.as_secs()))?
        .context("spawn script")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        let code = output.status.code().unwrap_or(-1);
        if stderr.is_empty() {
            anyhow::bail!("exit {code}");
        }
        // 截断长度与 docs/quota-script-prompt.md 承诺的 200 字符对齐。
        anyhow::bail!("exit {code} · {}", truncate(stderr, 200));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    if lines.len() > MAX_LINES {
        lines.truncate(MAX_LINES);
        lines.push(TRUNCATED_MARKER.to_string());
    }
    if lines.is_empty() {
        anyhow::bail!("脚本无输出");
    }
    Ok(BalanceView {
        available: None,
        headline: lines[0].clone(),
        items: Vec::new(),
        lines,
    })
}

/// recipe.vars → APIM_VAR_<大写名>，脚本里能取到自定义变量（如访问令牌）。
fn script_env_vars(recipe: &Recipe) -> HashMap<String, String> {
    recipe
        .vars
        .iter()
        .map(|(k, v)| {
            let name = format!("APIM_VAR_{}", k.to_uppercase().replace('-', "_"));
            (name, v.clone())
        })
        .collect()
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home)
            .join(rest)
            .to_string_lossy()
            .into_owned();
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::BalanceMode;

    fn script_recipe(run: &str, timeout_secs: Option<u64>) -> Recipe {
        Recipe {
            id: "demo".into(),
            name: "Demo".into(),
            base_url: "https://example.com".into(),
            homepage: None,
            supports_groups: false,
            vars: HashMap::from([("access_token".to_string(), "at-123".to_string())]),
            auth: Default::default(),
            health: None,
            balance: Some(BalanceMode::Script(ScriptSpec {
                command: None,
                run: Some(run.to_string()),
                shell: None,
                timeout_secs,
            })),
            origin: None,
        }
    }

    fn demo_key() -> KeyEntry {
        KeyEntry {
            provider: "demo".into(),
            alias: "main".into(),
            group: None,
            token: "sk-test".into(),
        }
    }

    fn script_spec(recipe: &Recipe) -> &ScriptSpec {
        recipe.balance.as_ref().unwrap().script().unwrap()
    }

    #[tokio::test]
    async fn script_output_becomes_lines_with_env_injected() {
        let recipe = script_recipe(
            "echo \"token=$APIM_TOKEN var=$APIM_VAR_ACCESS_TOKEN\"; echo '周 3/4'",
            None,
        );
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert!(snapshot.error.is_none(), "{:?}", snapshot.error);
        let view = snapshot.view.unwrap();
        assert_eq!(view.headline, "token=sk-test var=at-123");
        assert_eq!(view.lines, vec!["token=sk-test var=at-123", "周 3/4"]);
        assert_eq!(snapshot.endpoint, "script demo");
    }

    #[tokio::test]
    async fn script_nonzero_exit_reports_stderr() {
        let recipe = script_recipe("echo 配置坏了 >&2; exit 3", None);
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert!(snapshot.view.is_none());
        assert_eq!(snapshot.error.as_deref(), Some("exit 3 · 配置坏了"));
    }

    #[tokio::test]
    async fn script_timeout_kills_process() {
        let recipe = script_recipe("sleep 30", Some(1));
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert!(snapshot.view.is_none());
        assert!(snapshot.error.unwrap().contains("脚本超时"));
    }

    #[tokio::test]
    async fn script_empty_output_is_an_error() {
        let recipe = script_recipe("true", None);
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        assert_eq!(snapshot.error.as_deref(), Some("脚本无输出"));
    }

    #[tokio::test]
    async fn script_output_over_limit_is_truncated() {
        let recipe = script_recipe("seq 60", None);
        let snapshot = run_script_balance(&recipe, script_spec(&recipe), &demo_key()).await;
        let lines = snapshot.view.unwrap().lines;
        assert_eq!(lines.len(), MAX_LINES + 1);
        assert_eq!(lines[0], "1");
        assert_eq!(lines[MAX_LINES - 1], "50");
        assert_eq!(lines[MAX_LINES], "…（输出已截断）");
    }
}
