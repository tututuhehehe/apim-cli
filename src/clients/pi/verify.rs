//! 校验：写完让 **pi 自己**把模型列一遍（`pi --list-models`），勾选的必须都在。
//!
//! 跟 codex 那边一个思路（`codex debug models`）：apim 只负责把配置写进 pi 认识的形状，
//! 「pi 到底认不认」由 pi 说了算。没装 pi 就直接失败 —— 不校验的导入等于没验证过。

use std::path::Path;
use std::process::Command;

use crate::util::truncate;

/// `pi --list-models` 的输出是定宽表格：`provider model context max-out thinking images`。
/// 表头之后每行取前两列。返回 `(provider, model)` 列表。
pub fn parse_list_models(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let provider = fields.next()?;
            let model = fields.next()?;
            if provider == "provider" || provider.starts_with('-') {
                return None;
            }
            Some((provider.to_string(), model.to_string()))
        })
        .collect()
}

/// 让 pi 解析一遍新配置：勾选的模型必须出现在它列出的表里（且挂在我们写的 provider 键下）。
pub fn verify(
    pi_bin: &Path,
    agent_dir: &Path,
    provider_key: &str,
    models: &[String],
) -> Result<(), String> {
    let output = Command::new(pi_bin)
        .arg("--list-models")
        .env("PI_CODING_AGENT_DIR", agent_dir)
        .output()
        .map_err(|err| format!("执行 {} 失败：{err}", pi_bin.display()))?;
    if !output.status.success() {
        return Err(format!(
            "pi 读取新配置失败：{}",
            truncate(&String::from_utf8_lossy(&output.stderr), 200)
        ));
    }
    let listed = parse_list_models(&String::from_utf8_lossy(&output.stdout));
    let missing: Vec<&str> = models
        .iter()
        .map(String::as_str)
        .filter(|id| {
            !listed
                .iter()
                .any(|(provider, model)| provider == provider_key && model == id)
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    // pi 对 schema 非法的 models.json 会**整份忽略**，退出码却是 0，只在 stderr 打一行 warning ——
    // 不带上它，用户只能看到「未列出这些模型」，找不到真因。
    let mut message = format!("pi 未列出这些模型：{}", missing.join(", "));
    let stderr = truncate(&String::from_utf8_lossy(&output.stderr), 200);
    if !stderr.is_empty() {
        message.push_str(&format!("；pi 的警告：{stderr}"));
    }
    Err(message)
}
