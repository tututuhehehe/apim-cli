//! 模型目录生成（`catalog::build`）的测试。

use serde_json::{Value, json};

use crate::clients::codex::catalog;

/// 生成目录：照官方文档手写的迷你条目（不再克隆 codex 内置的 GPT 模板）。
#[test]
fn built_catalog_matches_official_minimal_shape() {
    let models = vec!["deepseek-flash".to_string(), "deepseek-chat".to_string()];
    let built = catalog::build(&models, "deepseek-chat").unwrap();
    let entries = built["models"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    // 默认模型排第一（priority 决定 codex 选择器里的排序）
    assert_eq!(entries[0]["slug"], "deepseek-chat");
    assert_eq!(entries[1]["slug"], "deepseek-flash");
    assert_eq!(entries[0]["priority"], 1);
    assert_eq!(entries[0]["display_name"], "deepseek-chat");
    // /model 只看 visibility=list；supported_in_api 决定会不会被 auth 过滤掉
    assert_eq!(entries[0]["visibility"], "list");
    assert_eq!(entries[0]["supported_in_api"], true);
    // 思考强度：四档固定，默认档跟着选
    assert_eq!(entries[0]["default_reasoning_level"], "high");
    let efforts: Vec<&str> = entries[0]["supported_reasoning_levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|level| level["effort"].as_str().unwrap())
        .collect();
    assert_eq!(efforts, ["medium", "high", "xhigh", "max"]);
    // 工具形态用官方文档的写法
    assert_eq!(entries[0]["shell_type"], "shell_command");
    assert_eq!(entries[0]["apply_patch_tool_type"], "freeform");
    // 必须有：两者都缺会让 codex 解析整个目录失败
    // base_instructions 是 codex 的必填字段，不能缺也不能给空串
    assert!(
        entries[0]["base_instructions"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "base_instructions 不能为空"
    );
    // 跨 codex 版本的两个必填字段名（cc-switch 记录过踩坑）
    assert_eq!(entries[0]["supports_reasoning_summaries"], true);
    assert_eq!(entries[0]["supports_reasoning_summary_parameter"], true);
    assert_eq!(entries[0]["supports_parallel_tool_calls"], true);
    // 不夹带 GPT 模板的专属字段（上一版就是被这些坑了）
    for key in [
        "model_messages",
        "tool_mode",
        "use_responses_lite",
        "node_repl_auto_review_required",
    ] {
        assert!(entries[0].get(key).is_none(), "{key} 不该出现在条目里");
    }
    // fail-open：与 DeepSeek 官方目录 / cc-switch 模板一致，免得粘图片被拦
    assert_eq!(entries[0]["input_modalities"], json!(["text", "image"]));
    assert_eq!(entries[0]["supports_search_tool"], false);
    // 严格网关会 400 的全分辨率图片能力显式关掉
    assert_eq!(entries[0]["supports_image_detail_original"], false);
    assert_eq!(entries[0]["context_window"], 272000);
    assert_eq!(entries[0]["max_context_window"], 272000);
    // OpenAI 专属的迁移/加速档提示显式清空
    assert_eq!(entries[0]["upgrade"], Value::Null);
    assert_eq!(entries[0]["availability_nux"], Value::Null);
    assert_eq!(entries[0]["service_tiers"], json!([]));
    // 体积：官方迷你条目应该在 1KB 量级（克隆 GPT 模板时是 64KB/模型）
    let size = serde_json::to_string(&built).unwrap().len();
    assert!(size < 4000, "目录应该很小，实际 {size} 字节");
}

#[test]
fn build_rejects_empty_model_list() {
    let built = catalog::build(&[], "m");
    assert!(built.is_err());
}
