//! 生成 `~/.codex/apim-models.json`（codex 的 `model_catalog_json` 指向它）。
//!
//! 为什么要生成：codex 靠「模型目录」决定 `/model` 里列出哪些模型、以及每个模型的
//! 元数据（思考等级、上下文窗口、工具形态）。自定义 provider 的模型不在目录里时，
//! codex 会打 `Model metadata for ... not found. Defaulting to fallback metadata`
//! 并且 `/model` 里看不到它们。
//!
//! 模板的唯一来源是**本机安装的 codex**：`codex debug models --bundled` 打出它内置的
//! 模型表，我们克隆其中一条改标识字段。这样永远跟 codex 版本一致，不需要在 apim 里
//! 内嵌一份会过期的模板文本。
//!
//! 两条来自 codex 源码的硬约束：
//! - 条目必须带 `base_instructions` 或 `model_messages.instructions_template`，否则
//!   整个目录解析失败（`deserialize_model_infos_with_legacy_base`）；
//! - `apply_patch_tool_type` / `shell_type` 决定 codex 往请求里塞哪种工具，所以整条
//!   模板连同它的 instructions 一起克隆，保证「instructions 描述的工具」和「实际声明
//!   的工具」一致。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};

/// 勾选模型一律给这四档思考等级（不按模型差异化）。
/// `model_reasoning_effort` 也只用这四个值，面板按 `e` 循环切换。
pub const EFFORTS: [&str; 4] = ["medium", "high", "xhigh", "max"];

/// 默认思考强度；codex 顶层 `model_reasoning_effort` 与目录条目的
/// `default_reasoning_level` 都用它。
pub const DEFAULT_EFFORT: &str = "high";

/// 每档思考强度的中文说明（写进目录条目给 codex 的选择器显示）。
fn effort_description(effort: &str) -> &'static str {
    match effort {
        "medium" => "中等推理",
        "high" => "深度推理",
        "xhigh" => "极高推理",
        "max" => "最高推理",
        _ => "",
    }
}

/// `model_messages` 里没有 `#[serde(default)]` 的必填键：缺失会让整个目录解析失败，
/// 生成条目时必须补成显式 null。
const REQUIRED_MODEL_MESSAGES_KEYS: &[&str] = &[
    "instructions_template",
    "instructions_variables",
    "approvals",
    "collaboration_modes",
    "auto_review",
    "permissions",
    "multi_agent",
];

/// OpenAI 专属、对第三方模型无意义、留着还会弹「升级到 xxx 模型」的键：生成时清掉。
const OPENAI_ONLY_KEYS: &[&str] = &[
    "upgrade",
    "availability_nux",
    "comp_hash",
    "model_specialty",
    "auto_review_model_override",
    "multi_agent_version",
    "multi_agent_reasoning_effort",
    "available_access_programs",
    "default_service_tier",
];

/// 找本机 codex：环境变量优先，其次 PATH，最后几个常见安装位置。
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

/// 取 codex 内置模型表，并挑出一条当模板。
pub fn load_template(codex_bin: Option<&Path>) -> Result<Value, String> {
    let Some(bin) = codex_bin else {
        return Err(
            "未找到 codex 可执行文件；生成模型目录需要它（安装 codex，或用 APIM_CODEX_BIN 指定路径）"
                .to_string(),
        );
    };
    let output = Command::new(bin)
        .args(["debug", "models", "--bundled"])
        .output()
        .map_err(|err| format!("执行 {} 失败：{err}", bin.display()))?;
    if !output.status.success() {
        return Err(format!(
            "`codex debug models --bundled` 失败：{}",
            truncate(&String::from_utf8_lossy(&output.stderr), 160)
        ));
    }
    let catalog: Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("解析 codex 内置模型表失败：{err}"))?;
    pick_template(&catalog)
        .cloned()
        .ok_or_else(|| "codex 内置模型表里没有可用的模板条目".to_string())
}

/// 挑模板：优先「在 API 里可用 + 会在选择器里列出 + 带 instructions」的条目，
/// 其中 priority 最小（codex 认为最旗舰）的那条。
pub(crate) fn pick_template(catalog: &Value) -> Option<&Value> {
    let entries = catalog.get("models")?.as_array()?;
    let usable = |entry: &&Value| {
        entry.get("supported_in_api").and_then(Value::as_bool) == Some(true)
            && has_instructions(entry)
    };
    entries
        .iter()
        .filter(usable)
        .filter(|entry| entry.get("visibility").and_then(Value::as_str) == Some("list"))
        .min_by_key(priority)
        .or_else(|| entries.iter().filter(usable).min_by_key(priority))
}

fn priority(entry: &&Value) -> i64 {
    entry
        .get("priority")
        .and_then(Value::as_i64)
        .unwrap_or(i64::MAX)
}

fn has_instructions(entry: &Value) -> bool {
    !instructions_of(entry).is_empty()
}

/// 条目的指令文本：优先 `model_messages.instructions_template`，退回旧的
/// 顶层 `base_instructions`。
fn instructions_of(entry: &Value) -> String {
    let template = entry
        .get("model_messages")
        .and_then(|messages| messages.get("instructions_template"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !template.is_empty() {
        return template.to_string();
    }
    entry
        .get("base_instructions")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// 由模板 + 勾选的模型生成整份目录。默认模型排在最前，默认思考强度
/// 写进每个条目的 `default_reasoning_level`。
pub fn build(
    template: &Value,
    models: &[String],
    default_model: &str,
    default_effort: &str,
) -> Result<Value, String> {
    if models.is_empty() {
        return Err("至少勾选一个模型".to_string());
    }
    let effort = if EFFORTS.contains(&default_effort) {
        default_effort
    } else {
        DEFAULT_EFFORT
    };
    let base = template
        .as_object()
        .ok_or_else(|| "模板条目不是 JSON 对象".to_string())?
        .clone();
    let instructions = instructions_of(template);

    let levels: Vec<Value> = EFFORTS
        .iter()
        .map(|effort| json!({ "effort": effort, "description": effort_description(effort) }))
        .collect();

    // 默认模型排第一，其余按勾选顺序（priority 决定 codex 选择器里的排序）
    let mut ordered: Vec<&String> = models.iter().collect();
    if let Some(pos) = ordered
        .iter()
        .position(|slug| slug.as_str() == default_model)
    {
        let default = ordered.remove(pos);
        ordered.insert(0, default);
    }

    let mut entries = Vec::with_capacity(ordered.len());
    for (index, slug) in ordered.iter().enumerate() {
        let mut entry = base.clone();
        entry.insert("slug".into(), json!(slug));
        entry.insert("display_name".into(), json!(slug));
        entry.insert(
            "description".into(),
            json!(format!("apim 导入的模型（{slug}）")),
        );
        entry.insert("priority".into(), json!(index as i64 + 1));
        entry.insert("visibility".into(), json!("list"));
        entry.insert("supported_in_api".into(), json!(true));
        entry.insert("default_reasoning_level".into(), json!(effort));
        entry.insert(
            "supported_reasoning_levels".into(),
            Value::Array(levels.clone()),
        );
        for key in OPENAI_ONLY_KEYS {
            entry.remove(*key);
        }
        // 有 default 的数组字段清空，避免残留 OpenAI 的加速档/专属工具
        entry.insert("service_tiers".into(), json!([]));
        entry.insert("additional_speed_tiers".into(), json!([]));
        entry.insert("experimental_supported_tools".into(), json!([]));

        fill_model_messages(&mut entry, &instructions)?;
        // 旧的顶层指令字段已升级进 model_messages，留着只是重复 21KB
        entry.remove("base_instructions");
        entries.push(Value::Object(entry));
    }
    Ok(json!({ "models": entries }))
}

/// 补齐 `model_messages`：必填键补 null，instructions 缺了就回填旧字段的值。
fn fill_model_messages(entry: &mut Map<String, Value>, instructions: &str) -> Result<(), String> {
    let messages = entry
        .entry("model_messages")
        .or_insert_with(|| Value::Object(Map::new()));
    let messages = messages
        .as_object_mut()
        .ok_or_else(|| "模板条目的 model_messages 不是 JSON 对象".to_string())?;
    for key in REQUIRED_MODEL_MESSAGES_KEYS {
        messages.entry((*key).to_string()).or_insert(Value::Null);
    }
    let missing = messages
        .get("instructions_template")
        .is_none_or(Value::is_null);
    if missing {
        if instructions.is_empty() {
            return Err(
                "codex 内置模型表里没有可用的 instructions（base_instructions / \
                 model_messages.instructions_template 都为空）"
                    .to_string(),
            );
        }
        messages.insert("instructions_template".into(), json!(instructions));
    }
    Ok(())
}

/// 原子写目录文件（tmp + rename；不含密钥，权限保持默认）。
pub fn write_catalog(path: &Path, catalog: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(catalog)
        .map_err(|err| format!("序列化模型目录失败：{err}"))?;
    let tmp = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("catalog"),
        std::process::id()
    ));
    std::fs::write(&tmp, text).map_err(|err| format!("写 {} 失败：{err}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|err| format!("替换 {} 失败：{err}", path.display()))?;
    Ok(())
}

/// 端到端校验：让 codex 自己解析刚写好的配置 + 目录，勾选的模型必须都在。
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

fn truncate(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect::<String>() + "…"
}
