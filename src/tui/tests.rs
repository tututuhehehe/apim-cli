use super::*;
use crate::app::{UndoAction, tests::test_app};
use crate::config::KeyEntry;

/// Ctrl+Z 必须走到 undo（按键路由），且不丢历史以外的状态。
#[test]
fn ctrl_z_routes_to_undo() {
    let (mut app, _rx, _rx_models) = test_app(&[("p", &[])]);
    app.config_dir = crate::app::tests::test_config_dir("tui-undo");
    app.push_undo(UndoAction::KeyDeleted {
        key: KeyEntry {
            provider: "p".into(),
            alias: "main".into(),
            group: None,
            token: "sk-test-placeholder".into(),
        },
    });

    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
    );

    assert!(app.undo_stack.is_empty(), "Ctrl+Z 应弹出并执行历史条目");
    assert_eq!(app.toast_text(), Some("已撤销：删除密钥 p.main"));
    assert_eq!(app.keys.len(), 1, "删除的密钥要回到内存");
}

/// 空历史上按 Ctrl+Z：只提示，不能 panic。
#[test]
fn ctrl_z_with_empty_history_is_harmless() {
    let (mut app, _rx, _rx_models) = test_app(&[("p", &[])]);
    app.config_dir = crate::app::tests::test_config_dir("tui-undo-empty");
    handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
    );
    assert_eq!(app.toast_text(), Some("没有可撤销的操作"));
}
