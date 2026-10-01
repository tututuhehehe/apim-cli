//! 一键导入的写盘派发与回执测试：runner 注入点 / 成功与失败回执 / 请求代际 /
//! 与 `m` 键列表一致。

use super::*;

// ---- 结果反馈 ----------------------------------------------------------

#[tokio::test]
async fn successful_import_toasts_marks_key_and_closes_panel() {
    let mut app = picker_app();
    app.import_toggle();
    app.import_confirm_models();
    let mut result = outcome("alpha.a1", Ok(report(&["gpt-6-sol"], "gpt-6-sol")));
    result.last_import = Some(last_import("alpha.a1"));
    result.seq = seq_of(&app);
    app.import_result(result);

    assert!(matches!(app.modal, Modal::None), "成功后应关面板");
    assert_eq!(
        app.toast_text(),
        Some(
            "已导入 Codex：alpha.a1 · 1 个模型 · 默认 gpt-6-sol · 强度 high · 表名 alpha · 重启 Codex 后才会列出新模型"
        )
    );
    assert!(
        app.is_active(Agent::Codex, "alpha.a1"),
        "★ 应打在这把密钥上"
    );
    assert!(!app.is_active(Agent::Codex, "alpha.a2"));
}

#[tokio::test]
async fn failed_import_keeps_panel_open_with_reason() {
    let mut app = picker_app();
    app.import_toggle();
    app.import_confirm_models();
    let mut failed = outcome("alpha.a1", Err("codex 未识别这些模型：x".into()));
    failed.seq = seq_of(&app);
    app.import_result(failed);

    assert_eq!(flow(&app).step, ImportStep::Failed);
    assert_eq!(flow(&app).error.as_deref(), Some("codex 未识别这些模型：x"));
    assert!(
        app.last_import_of(Agent::Codex).is_none(),
        "失败不能留下 ★ 状态"
    );
}

#[test]
fn late_result_after_panel_closed_becomes_a_toast() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.modal = Modal::None;
    app.import_result(outcome("alpha.a1", Err("迟到的失败".into())));
    assert!(matches!(app.modal, Modal::None));
    assert_eq!(app.toast_text(), Some("导入失败：迟到的失败"));
}

// ---- 请求代际（原来地笔记 2.3）--------------------------------------------

/// 面板关掉再为**同一把密钥**重开时，上一代的模型列表不能落到新面板里。
#[tokio::test]
async fn stale_model_list_from_an_earlier_generation_is_dropped() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;

    // 第一代：发起请求后关掉面板
    app.open_import();
    let first_seq = seq_of(&app);
    app.import_choose_agent();
    app.cancel_modal();

    // 第二代：同一把密钥重开并重新请求
    app.open_import();
    let second_seq = seq_of(&app);
    assert_ne!(first_seq, second_seq, "每次打开面板都应是新的一代");
    app.import_choose_agent();

    // 第一代的结果迟到 → 必须被丢弃（面板仍是 loading）
    app.import_receive_models("alpha.a1".into(), first_seq, Ok(mixed_entries()));
    assert!(flow(&app).items.is_empty(), "旧代际的列表不能落到新面板");
    assert!(flow(&app).loading, "面板应仍在等自己那一代的结果");

    // 第二代的结果才落地
    app.import_receive_models("alpha.a1".into(), second_seq, Ok(mixed_entries()));
    assert_eq!(visible_names(&app), mixed_entries());
}

/// 上一代的导入回执不能关掉新面板（同一把密钥重来时）。
#[tokio::test]
async fn stale_import_outcome_does_not_close_a_newer_panel() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;

    app.open_import();
    let first_seq = seq_of(&app);
    app.cancel_modal();

    // 新面板停在 Waiting
    app.open_import();
    let mut stale = outcome("alpha.a1", Ok(report(&["gpt-6-sol"], "gpt-6-sol")));
    stale.seq = first_seq;
    stale.last_import = Some(last_import("alpha.a1"));
    app.import_result(stale);

    assert!(
        matches!(app.modal, Modal::Import(_)),
        "旧代际的回执不该关掉新面板"
    );
    assert!(app.toast_text().is_some(), "但要给一条提示");
    assert!(
        app.is_active(Agent::Codex, "alpha.a1"),
        "导入本身是成功的，★ 记录应更新"
    );
}

/// `m` 键浏览弹窗不参与代际（固定 seq=0），仍要能正常落地。
#[tokio::test]
async fn browse_modal_ignores_generation() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    app.modal = crate::app::Modal::Models {
        key_id: "alpha.a1".into(),
        status: crate::app::ModelsStatus::Loading,
        filter: String::new(),
        searching: false,
    };
    app.apply_models("alpha.a1".into(), 0, Ok(mixed_entries()));
    match &app.modal {
        crate::app::Modal::Models {
            status: crate::app::ModelsStatus::Done { items, .. },
            ..
        } => assert_eq!(items, &mixed_entries()),
        other => panic!("浏览弹窗应落到 Done，实际 {other:?}"),
    }
}

// ---- 「导入面板列表 == m 键列表」（原来地笔记 2.7）------------------------

/// 用户明确要求：导入面板列出的模型必须和 `m` 键看到的完全一致。
/// 这条断言的作用是「有人以后给某一边加筛选/排序时立刻红」。
#[tokio::test]
async fn import_panel_lists_exactly_what_the_browse_modal_lists() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    let payload = mixed_entries();

    // 导入面板（选完客户端 → 模型列表落地）
    app.open_import();
    app.import_choose_agent();
    app.import_receive_models("alpha.a1".into(), seq_of(&app), Ok(payload.clone()));
    let from_import = visible_names(&app);

    // `m` 键浏览弹窗（同一份 payload）
    app.modal = Modal::None;
    app.open_models();
    app.apply_models("alpha.a1".into(), 0, Ok(payload.clone()));
    let from_browse = match &app.modal {
        Modal::Models {
            status: crate::app::ModelsStatus::Done { items, .. },
            ..
        } => items.clone(),
        other => panic!("浏览弹窗应为 Done，实际 {other:?}"),
    };

    assert_eq!(
        from_import, from_browse,
        "导入面板与 m 键的模型列表必须一致"
    );
    assert_eq!(from_import, payload, "两边都应是原样（不筛不排）");
}

// ---- 面板 → 客户端的注入点（原来地笔记 B11）--------------------------------

/// 「请求确实交给了所选的那个客户端」——以前面板级测试全部绕过 `start_import`，
/// 这条补上：注入一个假 runner，断言收到的是 `Agent::Codex` 与面板上那份请求。
#[tokio::test]
async fn import_request_goes_to_the_selected_agent() {
    use std::sync::{Arc, Mutex};
    /// runner 收到的调用记录：(交给了哪个客户端, 密钥 id, 勾选的模型)
    type Calls = Arc<Mutex<Vec<(Agent, String, Vec<String>)>>>;
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;

    let seen: Calls = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();
    app.import_runner = Some(Arc::new(move |agent, request| {
        recorder
            .lock()
            .unwrap()
            .push((agent, request.key_id(), request.models.clone()));
        Ok(report(&["gpt-6-sol"], "gpt-6-sol"))
    }));

    app.open_import();
    // 第一步选中 Codex（单客户端，符号语义上仍是显式选中的那个）
    assert_eq!(flow(&app).agent, Agent::Codex);
    app.import_choose_agent();
    app.import_receive_models("alpha.a1".into(), seq_of(&app), Ok(mixed_entries()));
    app.import_toggle();
    app.import_confirm_models();
    assert_eq!(flow(&app).step, ImportStep::Working, "应已派发到后台任务");

    // 等后台任务把请求交给 runner
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while seen.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let calls = seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "runner 应被调用一次");
    assert_eq!(calls[0].0, Agent::Codex, "交给的应是所选客户端");
    assert_eq!(calls[0].1, "alpha.a1");
    assert_eq!(calls[0].2, vec!["gpt-6-sol".to_string()]);
}
