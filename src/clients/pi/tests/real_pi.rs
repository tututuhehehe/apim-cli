//! 真机端到端：用**本机装的 pi** 走一遍导入，再让它自己把模型列出来。
//!
//! 默认不跑（`#[ignore]`）：它依赖本机 pi 的版本与行为，且会比单元测试慢。
//! 需要时手动：`cargo test -- pi_real_end_to_end --ignored --nocapture`

use std::fs;

use super::helpers::{request_for, temp_dir};
use crate::clients::pi::import::import_in;
use crate::clients::pi::{config, locate_pi, pi_provider_key};

#[test]
#[ignore = "需要本机安装 pi；用 cargo test -- --ignored 手动跑"]
fn pi_real_end_to_end() {
    let Some(bin) = locate_pi() else {
        eprintln!("跳过：没找到 pi 可执行文件");
        return;
    };
    let dir = temp_dir("real");
    // 拿用户真实的 pi 配置当输入：验证我们只动自己那几个键，别的（主题、扩展、别的 provider）都不丢
    let real_dir = crate::clients::pi::agent_dir();
    for name in [config::MODELS_FILE, config::SETTINGS_FILE] {
        let source = real_dir.join(name);
        if let Ok(text) = fs::read_to_string(&source) {
            fs::write(dir.join(name), text).unwrap();
        }
    }
    let settings_before = fs::read_to_string(config::settings_path(&dir)).ok();

    let request = request_for(&["deepseek-v4", "glm-5"]);
    let report = import_in(&dir, &request, Some(&bin)).expect("真机导入应成功");

    let models = fs::read_to_string(config::models_path(&dir)).unwrap();
    let settings = fs::read_to_string(config::settings_path(&dir)).unwrap();
    eprintln!(
        "pi {} → provider {} / 表名 {} / 默认（不写）{}；models.json {} 字节，settings.json {} 字节",
        bin.display(),
        pi_provider_key(&request.provider_id),
        report.provider_key,
        report.model.is_none(),
        models.len(),
        settings.len()
    );

    // 一键导入只加 provider + 模型：用户的默认 provider / 默认模型设定一个字节都不许变
    assert_eq!(report.model, None, "pi 不写默认模型");
    match settings_before {
        Some(before_text) => assert_eq!(settings, before_text, "settings.json 必须原样"),
        None => assert!(!config::settings_path(&dir).exists()),
    }
}
