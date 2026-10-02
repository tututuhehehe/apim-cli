//! 一次 pi 导入的编排：定位 pi → 改内存里的 models.json → 写盘 → `pi --list-models` 校验 → 失败回滚。
//!
//! **只写一处**：`models.json` → `providers.<apim-厂商id>`（`name` / `baseUrl`（补 `/v1`）/
//! `api = "openai-completions"` / `apiKey` / `models`（勾选的那几个））。
//!
//! **不动 `settings.json`**：一键导入只是「往模型列表里加上我要的模型和厂商」，默认 provider /
//! 默认模型由用户自己在 pi 里挑（`/model` + `Ctrl+S`），`enabledModels` 也不碰 —— 改用户
//! 原有设定不是这个功能该干的事。
//!
//! 顺序上（同 codex）：**先在内存里改完（失败时磁盘一点没动）**，**校验不过要把改动还原**。

use std::path::Path;

use serde_json::{Map, Value, json};

use super::{agent_dir, config, locate_pi, pi_provider_key, verify};
use crate::clients::lock::ImportLock;
use crate::clients::{ImportReport, ImportRequest, normalize_base_url};

/// 一键导入：定位本机 pi → 写入 models.json → 让 pi 自己列出模型校验。
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
    let mut models = config::read(&models_path)?;
    apply_models(&mut models, &key, request, &base_url)?;

    // 先在内存里改完，写盘失败时磁盘一点没动
    let models_backup = config::write(&models_path, &models)?;

    if let Err(err) = verify::verify(bin, dir, &key, &request.models) {
        let rolled_back = restore(&models_path, models_backup.as_deref());
        return Err(if rolled_back {
            format!("{err}（已还原到导入前的配置）")
        } else {
            format!(
                "{err}（自动还原失败，请手动把 {} 覆盖回去）",
                config::models_path(dir).display()
            )
        });
    }

    Ok(ImportReport {
        provider_key: key,
        // pi 没有「默认模型」这回事要写：用户自己在 /model 里挑
        model: None,
        models: request.models.clone(),
        // pi 侧没有「思考强度」这种要固定的东西（每个模型自己声明 reasoning，档位由 /thinking 选）
        detail: None,
        backups: models_backup.into_iter().collect(),
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
