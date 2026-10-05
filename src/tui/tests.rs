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

// ---- 分页（Tab）------------------------------------------------------

/// Tab 切分页（不再是切焦点）；h/l 与 ←/→ 仍然切左右栏焦点。
#[test]
fn tab_switches_provider_pages_and_arrows_switch_focus() {
    let (mut app, _rx, _rx_models) = test_app(&[("p", &["main"])]);
    app.focus = Focus::Providers;
    assert_eq!(app.tab, crate::recipe::ProviderKind::Model);

    handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.tab, crate::recipe::ProviderKind::NonModel);
    assert_eq!(app.focus, Focus::Providers, "Tab 不再动左右栏焦点");
    handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.tab, crate::recipe::ProviderKind::Model);

    // 焦点在密钥栏时 Tab 同样切分页
    app.focus = Focus::Keys;
    handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.tab, crate::recipe::ProviderKind::NonModel);
    assert_eq!(app.focus, Focus::Keys);

    handle_key(&mut app, KeyEvent::from(KeyCode::Left));
    assert_eq!(app.focus, Focus::Providers);
    handle_key(&mut app, KeyEvent::from(KeyCode::Char('l')));
    assert_eq!(app.focus, Focus::Keys);
}

/// 全链路：在非模型分页按 `a` → 勾选框默认已勾上 → 填表保存 →
/// 类型落盘、分页停在非模型页、旧的分页选中项不受影响。
#[tokio::test]
async fn add_non_model_provider_from_its_page_end_to_end() {
    let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
    app.config_dir = crate::app::tests::test_config_dir("tui-non-model");
    app.focus = Focus::Providers;

    handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
    let Modal::Form { form, .. } = &app.modal else {
        panic!("a 应打开添加厂商表单");
    };
    assert!(
        form.toggle(crate::form::PF_NON_MODEL),
        "在非模型页添加，勾选框默认跟随分页已勾上"
    );
    if let Modal::Form { form, .. } = &mut app.modal {
        form.fields[crate::form::PF_ID] = Field::text("ID", "deepl");
        form.fields[crate::form::PF_NAME] = Field::text("名称", "DeepL 翻译");
        form.fields[crate::form::PF_BASE] =
            Field::text("Base URL", "https://api-free.deepl.example");
    }
    handle_key(&mut app, KeyEvent::from(KeyCode::Enter));

    assert!(matches!(app.modal, Modal::None), "保存后弹窗关闭");
    let recipe = &app.recipes["deepl"];
    assert_eq!(recipe.kind, crate::recipe::ProviderKind::NonModel);
    assert!(
        recipe.health.is_none(),
        "非模型不探活（探活路径默认值被忽略）"
    );
    assert_eq!(app.tab, crate::recipe::ProviderKind::NonModel);
    assert_eq!(app.current_provider_id(), Some("deepl"));
    assert_eq!(app.focus, Focus::Keys, "保存后焦点落到密钥表");
    // 落盘可回读（YAML 里带 kind: non_model）
    let yaml = std::fs::read_to_string(app.config_dir.join("recipes/deepl.yaml")).unwrap();
    assert!(yaml.contains("kind: non_model"), "{yaml}");
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

// ---- `o` 键路由（OpenAI Codex OAuth）----------------------------------

/// `o` 只在内置 OpenAI 的密钥栏是登录；别处必须解释自己而不是静默无效。
/// 走真实按键路由，删掉 tui.rs 的兜底分支这条就会红。
#[test]
fn o_outside_openai_routes_to_the_toast() {
    let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    assert_ne!(app.current_provider_id(), Some("openai"));

    handle_key(&mut app, KeyEvent::from(KeyCode::Char('o')));

    assert!(
        app.toast_text().unwrap().contains("仅内置 OpenAI"),
        "{:?}",
        app.toast_text()
    );
}

/// OpenAI 上按 `o` 走登录分支：这里让登录“已在跑”，不绑端口、不开浏览器，
/// 只验证它没落到“仅内置 OpenAI 支持”的兜底提示上。
#[test]
fn o_on_openai_does_not_take_the_fallback_toast() {
    let (mut app, _rx, _rx_models) = test_app(&[("openai", &["k"])]);
    app.focus_provider("openai");
    app.focus = Focus::Keys;
    assert_eq!(app.current_provider_id(), Some("openai"));
    app.oauth_login_running = true;

    handle_key(&mut app, KeyEvent::from(KeyCode::Char('o')));

    assert_eq!(app.toast_text(), None, "OpenAI 上按 o 不该出现兜底提示");
}
