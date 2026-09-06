use crossterm::event::{KeyCode, KeyEvent};

use super::*;

fn sample_form() -> Form {
    key_add(vec!["deepseek".into(), "relay".into()], 0)
}

#[test]
fn tab_cycles_and_left_changes_select() {
    let mut form = sample_form();
    assert_eq!(form.active, 1);
    form.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(form.active, 2);
    form.handle_key(KeyEvent::from(KeyCode::BackTab));
    form.handle_key(KeyEvent::from(KeyCode::BackTab));
    assert_eq!(form.active, 0);
    form.handle_key(KeyEvent::from(KeyCode::Left));
    assert_eq!(form.select_value(0), "relay");
}

#[test]
fn readonly_field_ignores_input() {
    let recipe = crate::recipe::Recipe {
        id: "deepseek".into(),
        name: "DeepSeek".into(),
        base_url: "https://x".into(),
        homepage: None,
        models_url: None,
        supports_groups: false,
        vars: Default::default(),
        auth: Default::default(),
        health: None,
        balance: None,
        origin: None,
    };
    let mut form = provider_edit(&recipe, "");
    form.active = PF_ID;
    form.handle_key(KeyEvent::from(KeyCode::Char('z')));
    assert_eq!(form.text(PF_ID), "deepseek");
}

#[test]
fn provider_edit_echoes_script_command() {
    let recipe: crate::recipe::Recipe = serde_yaml::from_str(
        "id: glm\nname: GLM\nbase_url: 'https://x'\nauth: {kind: bearer}\nhomepage: https://bigmodel.cn/console\nbalance:\n  kind: script\n  command: ~/s.sh",
    )
    .unwrap();
    let form = provider_edit(&recipe, "/models");
    assert_eq!(form.text(PF_SCRIPT), "~/s.sh");
    assert_eq!(form.text(PF_HEALTH), "/models");
    assert_eq!(form.text(PF_HOMEPAGE), "https://bigmodel.cn/console");
}

#[test]
fn enter_saves_and_esc_cancels() {
    let mut form = sample_form();
    assert!(matches!(
        form.handle_key(KeyEvent::from(KeyCode::Enter)),
        FormEvent::Save
    ));
    assert!(matches!(
        form.handle_key(KeyEvent::from(KeyCode::Esc)),
        FormEvent::Cancel
    ));
}
