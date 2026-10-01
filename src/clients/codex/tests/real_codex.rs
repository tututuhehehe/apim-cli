//! 真机端到端（默认忽略：CI 没有 codex）。

use std::fs;

use serde_json::Value;

use crate::clients::codex::catalog::{CATALOG_FILE, locate_codex};
use crate::clients::codex::codex_home;
use crate::clients::codex::import::import_in;

use super::helpers::{request_for, temp_dir};

/// 需要本机真实 codex 的端到端校验：以**真实的 `~/.codex/config.toml`** 为输入，
/// 生成目录 → 改写配置 → 让 codex 自己解析这份新配置并确认勾选的模型都在。
///
/// 手动跑：`cargo test -- --ignored codex_real_end_to_end --nocapture`
/// 全程只写 target/ 下的临时目录，不碰真实 `~/.codex`。
#[cfg(unix)]
#[test]
#[ignore = "需要本机安装 codex；用 cargo test -- --ignored 手动跑"]
fn codex_real_end_to_end() {
    let Some(bin) = locate_codex() else {
        eprintln!("跳过：没找到 codex 可执行文件");
        return;
    };
    let dir = temp_dir("real");
    let home = dir.join("codex-home");
    fs::create_dir_all(&home).unwrap();

    // 拿用户的真实配置当输入，验证注释 / projects / tui / 旧 provider 块都不丢
    let real_config = codex_home().join("config.toml");
    let original = fs::read_to_string(&real_config).ok();
    if let Some(text) = &original {
        fs::write(home.join("config.toml"), text).unwrap();
    }

    let request = request_for("codex", &["deepseek-flash", "deepseek-chat"]);
    let report = import_in(&home, &request, Some(&bin)).expect("真机导入应成功");

    let written = fs::read_to_string(home.join("config.toml")).unwrap();
    let catalog = fs::read_to_string(home.join(CATALOG_FILE)).unwrap();
    eprintln!(
        "codex {} → 目录 {} 字节 / {} 个模型；配置 {} 字节",
        bin.display(),
        catalog.len(),
        report.models.len(),
        written.len()
    );
    eprintln!("---- 生成目录里的条目 ----");
    let parsed: Value = serde_json::from_str(&catalog).unwrap();
    for entry in parsed["models"].as_array().unwrap() {
        eprintln!(
            "  {} priority={} visibility={} apply_patch={:?} levels={}",
            entry["slug"],
            entry["priority"],
            entry["visibility"],
            entry["apply_patch_tool_type"],
            entry["supported_reasoning_levels"]
                .as_array()
                .map(|levels| levels
                    .iter()
                    .map(|level| level["effort"].as_str().unwrap_or("?").to_string())
                    .collect::<Vec<_>>()
                    .join("/"))
                .unwrap_or_default()
        );
    }

    // 真实配置里原有的东西一个都不能丢
    if let Some(original) = original {
        for marker in [
            "[projects.",
            "[tui]",
            "model_reasoning_effort",
            "[model_providers.",
        ] {
            if original.contains(marker) {
                assert!(
                    written.contains(marker),
                    "改写后丢了原有内容 {marker}\n{written}"
                );
            }
        }
        assert!(
            written.contains("[model_providers.ikun]"),
            "应新增 ikun 的 provider 块\n{written}"
        );
    }
}
