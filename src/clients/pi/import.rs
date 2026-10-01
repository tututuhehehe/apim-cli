//! 一次 pi 导入的编排：定位 pi → 改内存里的两份配置 → 写盘 → `pi --list-models` 校验 → 失败回滚。
//!
//! 写下去的东西：
//! - `models.json` → `providers.<apim-厂商id>`：`name` / `baseUrl`（补 `/v1`）/
//!   `api = "openai-completions"` / `apiKey` / `models`（勾选的那几个）
//! - `settings.json` → `defaultProvider` / `defaultModel`（面板第三步选的那个），
//!   并在 `enabledModels` 非空时把新模型追加进去（pi 自己保存默认模型时就是这么做的，
//!   否则用户设了 `enabledModels` 之后新模型进不了启动选择与 `Ctrl+P` 循环）
//!
//! 顺序上有两条硬要求（同 codex）：**先在内存里改完（失败时磁盘一点没动）**，
//! **校验不过要把两处改动都还原**。

use std::path::Path;

use serde_json::{Map, Value, json};

use super::{agent_dir, config, locate_pi, pi_provider_key, verify};
use crate::clients::lock::ImportLock;
use crate::clients::{ImportReport, ImportRequest, normalize_base_url};

/// 一键导入：定位本机 pi → 写入两份配置 → 让 pi 自己列出模型校验。
pub fn import(request: &ImportRequest) -> Result<ImportReport, String> {
    let pi_bin = locate_pi();
    import_in(&agent_dir(), request, pi_bin.as_deref())
}

/// 可注入 `<agent-dir>` 与 pi 可执行文件（测试用）。
pub fn import_in(
    dir: &Path,
    request: &ImportRequest,
    pi_bin: Option<&Path>,
) -> Result<ImportReport, String> {
    let Some(bin) = pi_bin else {
        return Err(
            "未找到 pi 可执行文件；导入后要用它列出模型校验（安装 pi，或用 APIM_PI_BIN 指定路径）"
                .to_string(),
        );
    };
    let key = pi_provider_key(&request.provider_id);
    let base_url = normalize_base_url(&request.base_url);

    std::fs::create_dir_all(dir).map_err(|err| format!("创建 {} 失败：{err}", dir.display()))?;
    // 整段「读 → 改 → 写 → 校验」都在锁里：否则两个 apim 会互相抹掉对方写的 provider 条目
    let _lock = ImportLock::acquire(dir, "pi")?;

    let models_path = config::models_path(dir);
    let settings_path = config::settings_path(dir);
    let mut models = config::read(&models_path)?;
    let mut settings = config::read(&settings_path)?;
    apply_models(&mut models, &key, request, &base_url)?;
    apply_settings(&mut settings, &key, &request.default_model);

    // 先写 models.json（它才是主体）：这一步失败时磁盘还一点没动
    let models_backup = config::write(&models_path, &models)?;
    let settings_backup = match config::write(&settings_path, &settings) {
        Ok(backup) => backup,
        Err(err) => {
            restore(&models_path, models_backup.as_deref());
            return Err(format!("{err}（已还原 models.json）"));
        }
    };

    if let Err(err) = verify::verify(bin, dir, &key, &request.models) {
        let rolled_back = restore(&models_path, models_backup.as_deref())
            & restore(&settings_path, settings_backup.as_deref());
        return Err(if rolled_back {
            format!("{err}（已还原到导入前的配置）")
        } else {
            format!(
                "{err}（自动还原失败，请手动把 {} / {} 覆盖回两份配置）",
                config::models_path(dir).display(),
                config::settings_path(dir).display()
            )
        });
    }

    Ok(ImportReport {
        provider_key: key,
        model: request.default_model.clone(),
        models: request.models.clone(),
        // pi 侧没有「思考强度」这种要固定的东西（每个模型自己声明 reasoning，档位由 /thinking 选）
        detail: None,
        backups: models_backup.into_iter().chain(settings_backup).collect(),
    })
}

/// `providers.<key>`：只动 apim 负责的键，用户手写的 `headers` / `compat` /
/// `modelOverrides` / `authHeader` 原样保留。
fn apply_models(
    models: &mut Map<String, Value>,
    key: &str,
    request: &ImportRequest,
    base_url: &str,
) -> Result<(), String> {
    let providers = models
        .entry("providers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "models.json 里的 providers 不是对象，apim 不覆盖它".to_string())?;
    let entry = providers
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("models.json 里 providers.{key} 不是对象，apim 不覆盖它"))?;
    entry.insert("name".into(), json!(request.provider_name));
    entry.insert("baseUrl".into(), json!(base_url));
    // 我们的中转站一律是 OpenAI 兼容端点（apim 探活也是按 /v1/models 探的）
    entry.insert("api".into(), json!("openai-completions"));
    entry.insert("apiKey".into(), json!(request.api_key));
    let listed: Vec<Value> = request.models.iter().map(|id| model_entry(id)).collect();
    entry.insert("models".into(), Value::Array(listed));
    Ok(())
}

/// 一条模型定义。只写 pi 认识的最小集合：
/// - `input` fail-open 给 `[text, image]`（少给就等于把图片能力悄悄关掉）
/// - `reasoning: true` 让 `/thinking` 能选档（模型不认就只是没有思考块）
/// - **不写** `contextWindow` / `maxTokens` / `cost`：留给 pi 的默认值，apim 不编数字
fn model_entry(id: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "reasoning": true,
        "input": ["text", "image"],
    })
}

/// `settings.json`：默认 provider / 模型指过去，并（在有 scope 时）把新模型加进 `enabledModels`。
///
/// 后半段是**镜像 pi 自己的行为**（`AgentSession._addPersistedDefaultToNonEmptyScope`）：
/// 用户按 `Ctrl+S` 存默认模型时，pi 会在 `enabledModels` 非空时把该模型追加进去，
/// 否则它进不了启动选择与 `Ctrl+P` 的循环 —— apim 不这么做就会「导入成功但选不到」。
fn apply_settings(settings: &mut Map<String, Value>, key: &str, model: &str) {
    settings.insert("defaultProvider".into(), json!(key));
    settings.insert("defaultModel".into(), json!(model));
    let reference = format!("{key}/{model}");
    if let Some(list) = settings
        .get_mut("enabledModels")
        .and_then(Value::as_array_mut)
        && !list.is_empty()
        && !list.iter().any(|v| {
            v.as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&reference))
        })
    {
        list.push(json!(reference));
    }
}

/// 用备份把文件还原回去；没有备份（原来是新文件）就删掉它。返回是否全部还原成功。
fn restore(path: &Path, backup: Option<&Path>) -> bool {
    match backup {
        // fs::copy 会连权限一起复制（备份是 600）
        Some(backup) => std::fs::copy(backup, path).is_ok(),
        None => match std::fs::remove_file(path) {
            Ok(()) => true,
            Err(err) => err.kind() == std::io::ErrorKind::NotFound,
        },
    }
}
