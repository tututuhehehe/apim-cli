//! 生成并校验 `~/.codex/apim-models.json`（codex 的 `model_catalog_json` 指向它）。
//!
//! 条目形状（哪些字段必填、**别克隆哪些**）、为什么这个文件非有不可、以及版本敏感项见
//! `docs/clients/codex.md` 的「`apim-models.json`（模型目录）」—— **改这个文件之前先读它**。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::util::truncate;

/// 勾选模型一律给这四档思考等级（不按模型差异化）。
/// 顶层 `model_reasoning_effort` 固定 [`DEFAULT_EFFORT`]，**面板不提供逐个选择**：
/// 想换档就在 codex 里用 `/model` 选，或直接改 `apim-models.json`。
pub const EFFORTS: [&str; 4] = ["medium", "high", "xhigh", "max"];

/// 生成的模型目录文件名。写进 `model_catalog_json` 时用相对路径
/// （codex 按 `CODEX_HOME` 解析相对路径，源码 `load_model_catalog` 已确认）。
pub const CATALOG_FILE: &str = "apim-models.json";

/// 默认思考强度：目录条目的 `default_reasoning_level` 与顶层 `model_reasoning_effort` 都用它。
pub const DEFAULT_EFFORT: &str = "high";

/// 上下文窗口：用 codex 自己给「未知模型」的默认值（`models-manager` 的
/// `model_info_from_slug` 就是 272000），因此跟「没有目录」时的行为一致。
/// 想按模型写真实窗口，直接改 `~/.codex/apim-models.json` 即可。
const CONTEXT_WINDOW: i64 = 272_000;

/// 工具形态：`shell_command` + `freeform apply_patch`，与 GLM / DeepSeek 官方接入文档一致，
/// 避免 codex 声明第三方网关不认的工具类型。
const SHELL_TYPE: &str = "shell_command";

/// 给模型的系统提示词。codex 把 `base_instructions` 当**必填字段**（缺少直接拒整份文件），
/// 但也不该把 62KB 的 GPT harness 塞给第三方模型，所以只给一句中性的 Codex 身份说明
/// （cc-switch 的第三方网关模板也是这个做法）。
const BASE_INSTRUCTIONS: &str = "You are Codex, a coding agent working in the user's terminal. \
     You and the user share the same workspace; work with them to achieve their goals.";

/// 每档思考强度的中文说明（codex 的 `/model` 选择器会显示）。
fn effort_description(effort: &str) -> &'static str {
    match effort {
        "medium" => "中等推理",
        "high" => "深度推理",
        "xhigh" => "极高推理",
        "max" => "最高推理",
        _ => "",
    }
}

/// 找本机 codex：环境变量优先，其次 PATH，最后几个常见安装位置。
/// 只在导入后做端到端校验时用到（`codex debug models`）。
pub fn locate_codex() -> Option<PathBuf> {
    for var in ["APIM_CODEX_BIN", "CODEX_BIN"] {
        if let Some(value) = std::env::var_os(var) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in ["codex", "codex.exe"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    [
        "/opt/homebrew/bin/codex",
        "/usr/local/bin/codex",
        "/usr/bin/codex",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

/// 由勾选的模型生成整份目录。默认模型排第一（`priority` 决定 codex 选择器里的排序）。
///
/// 每个条目都带全四档思考等级（`medium/high/xhigh/max`），默认档固定为 [`DEFAULT_EFFORT`] ——
/// apim 面板不让人逐个选：想换档就在 codex 里用 `/model` 选，或直接改 `apim-models.json`。
pub fn build(models: &[String], default_model: &str) -> Result<Value, String> {
    if models.is_empty() {
        return Err("至少勾选一个模型".to_string());
    }
    let mut ordered: Vec<&String> = models.iter().collect();
    if let Some(position) = ordered
        .iter()
        .position(|slug| slug.as_str() == default_model)
    {
        let default = ordered.remove(position);
        ordered.insert(0, default);
    }
    let entries: Vec<Value> = ordered
        .iter()
        .enumerate()
        .map(|(index, slug)| model_entry(slug, DEFAULT_EFFORT, index as i64 + 1))
        .collect();
    Ok(json!({ "models": entries }))
}

/// 一个模型的目录条目。字段集 = GLM / DeepSeek 官方文档 + cc-switch 跨版本实测过的
/// 最小模板，兼顾 codex 各版本的必填字段：
/// - `supports_reasoning_summaries`：codex ≥0.144.5 缺它会拒整份目录（cc-switch v3.18.0 记录）
/// - `supports_parallel_tool_calls`：codex 0.144.5~0.148.0-alpha.15 当必填（cc-switch v3.20.2 记录）
/// - `base_instructions`：必填，不能缺也不能给空串
fn model_entry(slug: &str, effort: &str, priority: i64) -> Value {
    let levels: Vec<Value> = EFFORTS
        .iter()
        .map(|effort| json!({ "effort": effort, "description": effort_description(effort) }))
        .collect();
    json!({
        "slug": slug,
        "display_name": slug,
        "description": format!("apim 导入的模型（{slug}）"),
        "base_instructions": BASE_INSTRUCTIONS,
        "default_reasoning_level": effort,
        "supported_reasoning_levels": levels,
        "shell_type": SHELL_TYPE,
        // list = 会出现在 /model 里（codex 的 show_in_picker 只看这个）
        "visibility": "list",
        "supported_in_api": true,
        "priority": priority,
        // 同一件事的两个字段名：老版本（0.144.5~0.148）认复数名且缺了拒整份文件，
        // 新版认 `_parameter`。两个都给，跨版本都安全。
        "supports_reasoning_summaries": true,
        "supports_reasoning_summary_parameter": true,
        "default_reasoning_summary": "none",
        "support_verbosity": false,
        // 老版本当必填字段；值只影响 codex 会不会并行发工具调用
        "supports_parallel_tool_calls": true,
        "apply_patch_tool_type": "freeform",
        "web_search_tool_type": "text",
        // 中转站不一定实现了 codex 的 hosted search，关掉更安全
        "supports_search_tool": false,
        "supports_image_detail_original": false,
        "truncation_policy": { "mode": "bytes", "limit": 10_000 },
        "context_window": CONTEXT_WINDOW,
        "max_context_window": CONTEXT_WINDOW,
        "effective_context_window_percent": 95,
        // fail-open：未知模型也声明支持图片输入（与 DeepSeek 官方目录、cc-switch 模板一致）。
        // 声明得保守（比如只给 text）反而会让用户粘图片时被 codex 直接拦下
        "input_modalities": ["text", "image"],
        "experimental_supported_tools": [],
        // 下面几个是 OpenAI 专属的迁移/加速档提示，显式清空，别让 codex 弹升级提示
        "availability_nux": null,
        "upgrade": null,
        "default_verbosity": null,
        "additional_speed_tiers": [],
        "service_tiers": [],
    })
}

/// 原子写目录文件（tmp + rename）。
///
/// tmp 名走 `config::tmp_path`（`<原名>.<pid>.tmp`）：与 config.toml 的写盘同一套规则，
/// 两个 apim 实例并发时不会互踩。
///
/// 这里刻意用 `fs::write`（默认 644）而**不是** `config::write_private`：这个文件里没有
/// 密钥，600 反而是坑 —— `sudo codex` 会读不到它。
pub fn write_catalog(path: &Path, catalog: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(catalog)
        .map_err(|err| format!("序列化模型目录失败：{err}"))?;
    let tmp = crate::config::tmp_path(path);
    std::fs::write(&tmp, text).map_err(|err| format!("写 {} 失败：{err}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|err| format!("替换 {} 失败：{err}", path.display()))?;
    Ok(())
}

/// 端到端校验：让 codex 自己解析刚写好的配置 + 目录，勾选的模型必须都在。
/// 这是导入成功与否的判据（codex 解析失败时会直接报错，不会静默降级）。
pub fn verify(codex_bin: &Path, home: &Path, models: &[String]) -> Result<(), String> {
    let output = Command::new(codex_bin)
        .args(["debug", "models"])
        .env("CODEX_HOME", home)
        .output()
        .map_err(|err| format!("执行 {} 失败：{err}", codex_bin.display()))?;
    if !output.status.success() {
        return Err(format!(
            "codex 读取新配置失败：{}",
            truncate(&String::from_utf8_lossy(&output.stderr), 200)
        ));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("codex 输出的模型表无法解析：{err}"))?;
    let slugs: Vec<&str> = parsed
        .get("models")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("slug").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let missing: Vec<&str> = models
        .iter()
        .map(String::as_str)
        .filter(|slug| !slugs.contains(slug))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("codex 未识别这些模型：{}", missing.join(", ")))
    }
}
