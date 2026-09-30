//! 一键导入面板的状态机与按键测试。全部用 `test_app` 造内存 App，不碰真实配置。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::tests::test_app;
use crate::clients::DEFAULT_EFFORT;
use crate::probe::ModelEntry;

fn entry(id: &str, responses: Option<bool>) -> ModelEntry {
    ModelEntry {
        id: id.to_string(),
        responses,
    }
}

/// 三种端点能力都有的模型列表：明确支持 / 明确不支持 / 没给信息。
fn mixed_entries() -> Vec<ModelEntry> {
    vec![
        entry("gpt-6-sol", Some(true)),
        entry("chat-only", Some(false)),
        entry("deepseek-v4", None),
    ]
}

fn open(app: &mut App, key_id: &str) {
    app.modal = Modal::Import(ImportFlow::new(key_id.to_string()));
}

fn flow(app: &App) -> &ImportFlow {
    match &app.modal {
        Modal::Import(flow) => flow,
        other => panic!("期望导入面板，实际 {other:?}"),
    }
}

fn visible_names(app: &App) -> Vec<String> {
    let flow = flow(app);
    flow.visible()
        .into_iter()
        .map(|index| flow.items[index].name.clone())
        .collect()
}

fn checked_names(app: &App) -> Vec<String> {
    flow(app)
        .items
        .iter()
        .filter(|item| item.checked)
        .map(|item| item.name.clone())
        .collect()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

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
    assert!(app.import_awaiting("alpha.a1", ImportStep::Models));

    app.import_receive_models("alpha.a1".into(), Ok(mixed_entries()));
    let flow = flow(&app);
    assert!(!flow.loading);
    assert_eq!(flow.step, ImportStep::Models);
    // 不预勾任何模型：勾哪些、哪个当默认都由用户在面板里定
    // 也不隐藏任何模型：厂商标注只当提示（实测会漏报）
    assert_eq!(
        visible_names(&app),
        ["gpt-6-sol", "chat-only", "deepseek-v4"]
    );
    assert!(checked_names(&app).is_empty());
    assert_eq!(flow.declared_unsupported_count(), 1);
}

#[tokio::test]
async fn late_model_result_for_another_key_is_dropped() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1", "a2"])]);
    app.focus = Focus::Keys;
    app.open_import();
    app.import_choose_agent();

    app.import_receive_models("alpha.a2".into(), Ok(mixed_entries()));
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
    app.import_receive_models("alpha.a1".into(), Err("HTTP 401 未授权".into()));
    assert_eq!(flow(&app).step, ImportStep::Failed);
    assert_eq!(flow(&app).error.as_deref(), Some("HTTP 401 未授权"));
}

// ---- 第二步：勾选 / 默认 / 过滤 ----------------------------------------

fn picker_app() -> App {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    open(&mut app, "alpha.a1");
    let mut app_flow = ImportFlow::new("alpha.a1".into());
    app_flow.step = ImportStep::Models;
    app_flow.items = mixed_entries()
        .into_iter()
        .map(|entry| ModelPick {
            name: entry.id,
            responses: entry.responses,
            checked: false,
        })
        .collect();
    app.modal = Modal::Import(app_flow);
    app
}

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

    // 厂商标注「不支持 responses」的模型**不会**被隐藏（实测标注会漏报，隐藏会藏掉能用的模型）
    assert_eq!(
        visible_names(&app),
        ["gpt-6-sol", "chat-only", "deepseek-v4"]
    );
    assert_eq!(flow(&app).declared_unsupported_count(), 1);
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

// ---- 结果反馈 ----------------------------------------------------------

fn report(models: &[&str], model: &str) -> ImportReport {
    ImportReport {
        provider_key: "alpha".into(),
        model: model.into(),
        models: models.iter().map(|name| (*name).to_string()).collect(),
        reasoning_effort: DEFAULT_EFFORT.into(),
        backup_path: None,
    }
}

fn outcome(key_id: &str, result: Result<ImportReport, String>) -> ImportOutcome {
    ImportOutcome {
        key_id: key_id.into(),
        result,
        state: None,
        state_error: None,
        restarted: 0,
    }
}

#[tokio::test]
async fn successful_import_toasts_marks_key_and_closes_panel() {
    let mut app = picker_app();
    app.import_toggle();
    app.import_confirm_models();
    let mut result = outcome("alpha.a1", Ok(report(&["gpt-6-sol"], "gpt-6-sol")));
    result.state = Some(CodexState {
        provider: "alpha".into(),
        provider_name: "alpha".into(),
        alias: "a1".into(),
        provider_key: "alpha".into(),
        models: vec!["gpt-6-sol".into()],
        default_model: "gpt-6-sol".into(),
        reasoning_effort: DEFAULT_EFFORT.into(),
    });
    app.import_result(result);

    assert!(matches!(app.modal, Modal::None), "成功后应关面板");
    assert_eq!(
        app.toast_text(),
        Some(
            "已导入 Codex：alpha.a1 · 1 个模型 · 默认 gpt-6-sol · 强度 high · 重启 codex 后 /model 才会列出新模型"
        )
    );
    assert!(app.is_codex_active("alpha.a1"), "★ 应打在这把密钥上");
    assert!(!app.is_codex_active("alpha.a2"));
}

#[tokio::test]
async fn failed_import_keeps_panel_open_with_reason() {
    let mut app = picker_app();
    app.import_toggle();
    app.import_confirm_models();
    app.import_result(outcome("alpha.a1", Err("codex 未识别这些模型：x".into())));

    assert_eq!(flow(&app).step, ImportStep::Failed);
    assert_eq!(flow(&app).error.as_deref(), Some("codex 未识别这些模型：x"));
    assert!(app.codex.is_none(), "失败不能留下 ★ 状态");
}

#[test]
fn late_result_after_panel_closed_becomes_a_toast() {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.modal = Modal::None;
    app.import_result(outcome("alpha.a1", Err("迟到的失败".into())));
    assert!(matches!(app.modal, Modal::None));
    assert_eq!(app.toast_text(), Some("导入失败：迟到的失败"));
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

fn flow_mut(app: &mut App) -> &mut ImportFlow {
    match &mut app.modal {
        Modal::Import(flow) => flow,
        other => panic!("期望导入面板，实际 {other:?}"),
    }
}
