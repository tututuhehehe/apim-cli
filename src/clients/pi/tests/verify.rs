//! `pi --list-models` 的表格解析与校验。

use std::fs;

// 前两个测试只解析表格（纯函数，跨平台），后面四个才要假 pi 脚本 → import 整行带门。
#[cfg(unix)]
use super::helpers::{fake_pi, fake_pi_failing, temp_dir, write_model_table};
use crate::clients::pi::verify;

#[test]
fn parses_the_listing_table() {
    let table = "\
provider   model        context  max-out  thinking  images
apim-ikun  glm-5        128K     16.4K    no        no
opencode   gpt-6-sol    1.1M     128K     yes       yes
";
    let parsed = verify::parse_list_models(table);
    assert_eq!(
        parsed,
        vec![
            ("apim-ikun".to_string(), "glm-5".to_string()),
            ("opencode".to_string(), "gpt-6-sol".to_string()),
        ]
    );
}

#[test]
fn empty_output_parses_as_nothing() {
    assert!(verify::parse_list_models("").is_empty());
    // 只有表头（provider 没有可用凭据时的样子）
    assert!(verify::parse_list_models("provider model context\n").is_empty());
}

#[cfg(unix)]
#[test]
fn verify_passes_when_every_ticked_model_is_listed_under_our_provider() {
    let dir = temp_dir("verify-ok");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-ikun", &["glm-5", "gpt-6-sol"]);
    let models = vec!["glm-5".to_string(), "gpt-6-sol".to_string()];
    assert!(verify::verify(&bin, &dir, "apim-ikun", &models).is_ok());
}

/// 同名模型挂在**别的** provider 下不算数（pi 的表里 provider 是第一列）。
#[cfg(unix)]
#[test]
fn verify_requires_the_model_under_our_provider_key() {
    let dir = temp_dir("verify-other-provider");
    let bin = fake_pi(&dir);
    write_model_table(&dir, "apim-other", &["glm-5"]);
    let models = vec!["glm-5".to_string()];
    let err = verify::verify(&bin, &dir, "apim-ikun", &models).unwrap_err();
    assert!(err.contains("glm-5"), "{err}");
}

#[cfg(unix)]
#[test]
fn verify_reports_pi_failure_with_its_stderr() {
    let dir = temp_dir("verify-fail");
    let bin = fake_pi_failing(&dir);
    let err = verify::verify(&bin, &dir, "apim-ikun", &["glm-5".to_string()]).unwrap_err();
    assert!(err.contains("no such config"), "{err}");
}

/// pi 对 schema 非法的 models.json 会**整份忽略**但退出码仍为 0，只在 stderr 打一行 warning：
/// 这种情况下不能只报「未列出这些模型」，要把 pi 的警告也带上（否则真因不可见）。
#[cfg(unix)]
#[test]
fn verify_surfaces_pi_warnings_when_models_are_missing() {
    let dir = temp_dir("verify-warning");
    let bin = fake_pi(&dir);
    fs::write(
        dir.join("fake-list.txt"),
        "provider model context\nopencode gpt-6-sol 1.1M\n",
    )
    .unwrap();
    fs::write(
        dir.join("fake-warning.txt"),
        "Warning: errors loading models.json\n",
    )
    .unwrap();

    let err = verify::verify(&bin, &dir, "apim-ikun", &["glm-5".to_string()]).unwrap_err();
    assert!(err.contains("glm-5"), "{err}");
    assert!(
        err.contains("errors loading models.json"),
        "要把 pi 的警告一并展示：{err}"
    );
}
