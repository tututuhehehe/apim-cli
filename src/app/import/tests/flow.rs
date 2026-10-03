//! 一键导入面板的状态机测试：打开面板 → 选客户端 → 勾选/搜索 → 三步流转 → 按键路由。

use super::*;

// ---- 打开面板 ----------------------------------------------------------
#[test]
fn open_requires_key_focus_and_a_selected_key() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);

    // 焦点在厂商栏：不打开面板，给提示
    app.focus = Focus::Providers;
    app.open_import();
    assert!(matches!(app.modal, Modal::None));
    assert!(app.toast.is_some(), "厂商栏按 x 应提示");

    // 密钥栏：正常打开，停在第一步
    app.focus = Focus::Keys;
    app.toast = None;
    app.open_import();
    assert_eq!(flow(&app).key_id, "alpha.a1");
    assert_eq!(flow(&app).step, ImportStep::Agent);
}

#[test]
fn open_without_keys_toasts_instead_of_opening() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &[])]);
    app.focus = Focus::Keys;
    app.open_import();
    assert!(matches!(app.modal, Modal::None), "没有密钥不该开面板");
    assert!(app.toast.is_some());
}

/// 非模型厂商没有模型列表，导入客户端无从谈起：给提示而不是开一个空面板。
#[test]
fn open_refuses_for_non_model_provider() {
    let (mut app, _rx, _rx_task) = test_app(&[("deepl", &["main"])]);
    app.recipes.get_mut("deepl").unwrap().kind = crate::recipe::ProviderKind::NonModel;
    app.focus = Focus::Keys;
    app.open_import();
    assert!(matches!(app.modal, Modal::None));
    assert_eq!(
        app.toast_text(),
        Some("deepl 假厂商 是非模型厂商，不能导入到客户端")
    );
}

// ---- 第一步：选客户端 → 拉模型 ----------------------------------------

#[tokio::test]
async fn choosing_agent_fetches_models_and_leaves_selection_empty() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    app.open_import();
    app.import_choose_agent();
    // 进入 Models 步骤并在等结果
    assert_eq!(flow(&app).step, ImportStep::Models);
    assert!(flow(&app).loading);
    assert!(app.import_awaiting("alpha.a1", seq_of(&app), ImportStep::Models));

    app.import_receive_models("alpha.a1".into(), seq_of(&app), Ok(mixed_entries()));
    let flow = flow(&app);
    assert!(!flow.loading);
    assert_eq!(flow.step, ImportStep::Models);
    // 拉回来的列表原样落地（不筛不排）
    assert_eq!(visible_names(&app), mixed_entries());
    // 不预勾任何模型：勾哪些、哪个当默认都由用户在面板里定
    assert!(checked_names(&app).is_empty());
}

#[tokio::test]
async fn late_model_result_for_another_key_is_dropped() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1", "a2"])]);
    app.focus = Focus::Keys;
    app.open_import();
    app.import_choose_agent();

    app.import_receive_models("alpha.a2".into(), seq_of(&app), Ok(mixed_entries()));
    assert!(
        flow(&app).items.is_empty(),
        "别的密钥的结果不能填进当前面板"
    );
}

#[tokio::test]
async fn model_fetch_error_lands_in_failed_step() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    app.open_import();
    app.import_choose_agent();
    app.import_receive_models(
        "alpha.a1".into(),
        seq_of(&app),
        Err("HTTP 401 未授权".into()),
    );
    assert_eq!(flow(&app).step, ImportStep::Failed);
    assert_eq!(flow(&app).error.as_deref(), Some("HTTP 401 未授权"));
}

// ---- 第二步：勾选 / 过滤 / 默认 ----------------------------------------

#[test]
fn toggle_all_keeps_every_model_visible() {
    let mut app = picker_app();

    // 光标在第 0 行：空格勾上
    app.import_toggle();
    assert_eq!(checked_names(&app), ["gpt-6-sol"]);

    // a：可见的全勾；再按一次全取消
    app.import_toggle_all();
    assert_eq!(
        checked_names(&app),
        ["gpt-6-sol", "chat-only", "deepseek-v4"]
    );
    app.import_toggle_all();
    assert!(
        checked_names(&app).is_empty(),
        "可见的已全勾，再按应变全取消"
    );
}

#[test]
fn cursor_moves_stay_inside_visible_window() {
    let mut app = picker_app();
    app.import_move(-1);
    assert_eq!(flow(&app).cursor, 0, "顶部再往上不动");
    app.import_move(100);
    assert_eq!(flow(&app).cursor, 2, "钳到最后一个可见条目");
}

#[test]
fn search_filter_narrows_visible_list() {
    let mut app = picker_app();
    app.import_start_search();
    assert!(app.import_searching());
    app.import_search_char('D');
    app.import_search_char('e');
    assert_eq!(visible_names(&app), ["deepseek-v4"]);
    app.import_search_backspace();
    assert_eq!(visible_names(&app), ["deepseek-v4"], "de 仍只匹配 deepseek");
    app.import_exit_search();
    assert!(!app.import_searching());
    assert_eq!(flow(&app).filter, "D");
}

// ---- 应用 --------------------------------------------------------------

#[test]
fn confirm_without_selection_only_toasts() {
    let mut app = picker_app();
    app.import_confirm_models();
    assert_eq!(flow(&app).step, ImportStep::Models, "没勾选不能往下走");
    assert_eq!(app.toast_text(), Some("至少勾选一个模型"));
}

#[tokio::test]
async fn single_checked_model_skips_the_default_panel() {
    let mut app = picker_app();
    app.import_toggle();
    app.import_confirm_models();
    // 只勾了一个：跳过「选默认模型」，直接进 Working
    assert_eq!(flow(&app).step, ImportStep::Working);
}

#[tokio::test]
async fn multiple_checked_models_ask_which_is_the_default() {
    let mut app = picker_app();
    app.import_toggle_all();
    assert_eq!(
        checked_names(&app),
        ["gpt-6-sol", "chat-only", "deepseek-v4"]
    );
    app.import_confirm_models();
    // 先停在「选默认模型」，光标默认第一个勾选的
    assert_eq!(flow(&app).step, ImportStep::DefaultModel);
    assert_eq!(flow(&app).default_cursor, 0);

    // j/k 在「已勾选」列表里移动；h 退回勾选步骤
    app.import_move_default(1);
    assert_eq!(flow(&app).default_cursor, 1);
    app.import_move_default(99);
    assert_eq!(flow(&app).default_cursor, 2, "钳到最后一个已勾选模型");
    app.import_default_back();
    assert_eq!(flow(&app).step, ImportStep::Models);

    // 再进一次并确认：进入写入
    app.import_confirm_models();
    app.import_confirm_default();
    assert_eq!(flow(&app).step, ImportStep::Working);
}

// ---- 按键路由 ----------------------------------------------------------

#[test]
fn keys_route_through_the_panel() {
    let mut app = picker_app();

    // j/k 移动
    handle_import_key(&mut app, key(KeyCode::Char('j')));
    assert_eq!(flow(&app).cursor, 1);
    handle_import_key(&mut app, key(KeyCode::Char('k')));
    assert_eq!(flow(&app).cursor, 0);

    // 空格勾选 + a 全选
    handle_import_key(&mut app, key(KeyCode::Char(' ')));
    assert_eq!(checked_names(&app), ["gpt-6-sol"]);
    handle_import_key(&mut app, key(KeyCode::Char('a')));
    assert_eq!(checked_names(&app).len(), 3, "a 会把可见的全部勾上");

    // / 进搜索，输入字符进过滤，Esc 退出搜索但不关面板
    handle_import_key(&mut app, key(KeyCode::Char('/')));
    assert!(app.import_searching());
    handle_import_key(&mut app, key(KeyCode::Char('g')));
    assert_eq!(flow(&app).filter, "g");
    handle_import_key(&mut app, key(KeyCode::Esc));
    assert!(!app.import_searching());
    assert!(matches!(app.modal, Modal::Import(_)), "Esc 只退搜索态");

    // Ctrl 组合不进过滤（避免误触）
    handle_import_key(&mut app, key(KeyCode::Char('/')));
    handle_import_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );
    assert_eq!(flow(&app).filter, "g");

    // Esc 第一次退出搜索态，第二次才关面板
    handle_import_key(&mut app, key(KeyCode::Esc));
    assert!(!app.import_searching());
    assert!(matches!(app.modal, Modal::Import(_)));
    handle_import_key(&mut app, key(KeyCode::Esc));
    assert!(matches!(app.modal, Modal::None));
}

#[tokio::test]
async fn keys_route_through_the_default_model_step() {
    let mut app = picker_app();
    app.import_toggle_all();
    handle_import_key(&mut app, key(KeyCode::Enter));
    assert_eq!(flow(&app).step, ImportStep::DefaultModel);

    // j 移动，h 返回勾选
    handle_import_key(&mut app, key(KeyCode::Char('j')));
    assert_eq!(flow(&app).default_cursor, 1);
    handle_import_key(&mut app, key(KeyCode::Char('h')));
    assert_eq!(flow(&app).step, ImportStep::Models);

    // 再进，Enter 开写
    handle_import_key(&mut app, key(KeyCode::Enter));
    handle_import_key(&mut app, key(KeyCode::Enter));
    assert_eq!(flow(&app).step, ImportStep::Working);
}

#[tokio::test]
async fn first_step_enter_advances_to_model_step() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    app.open_import();
    handle_import_key(&mut app, key(KeyCode::Enter));
    assert_eq!(flow(&app).step, ImportStep::Models);
}

#[test]
fn working_step_ignores_keys_except_escape() {
    let mut app = picker_app();
    flow_mut(&mut app).step = ImportStep::Working;
    handle_import_key(&mut app, key(KeyCode::Char('a')));
    assert_eq!(flow(&app).step, ImportStep::Working, "写入中按键应被忽略");
    handle_import_key(&mut app, key(KeyCode::Esc));
    assert!(matches!(app.modal, Modal::None), "Esc 允许关面板");
}

/// 防御性：即使 DefaultModel 步在「一个都没勾」的状态下被按了 Enter（UI 到不了这里，
/// 但将来改动可能放开），也只能原地不动，不能 panic 或写盘。
#[tokio::test]
async fn confirm_default_with_nothing_checked_is_a_no_op() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    let mut import_flow = ImportFlow::new("alpha.a1".into(), 1);
    import_flow.step = ImportStep::DefaultModel;
    import_flow.items = mixed_entries()
        .into_iter()
        .map(|name| ModelPick {
            name,
            checked: false,
        })
        .collect();
    app.modal = Modal::Import(import_flow);

    app.import_confirm_default();
    assert_eq!(seq_of(&app), 1, "不该重新发号");
    assert_eq!(
        flow(&app).step,
        ImportStep::DefaultModel,
        "没可选项时保持原状"
    );
}
