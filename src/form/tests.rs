use crossterm::event::{KeyCode, KeyEvent};

use super::*;

/// 当前会渲染出来的行（Tab 跳过的也正是这些）。
fn visible_labels(form: &Form) -> Vec<&str> {
    form.fields
        .iter()
        .filter(|f| !f.is_hidden())
        .map(|f| f.label())
        .collect()
}

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
        kind: crate::recipe::ProviderKind::Model,
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

// ---- 勾选框（添加厂商的「非模型」）-------------------------------------

#[test]
fn toggle_flips_with_space_and_arrows_but_not_inside_text() {
    let mut form = provider_add(crate::recipe::ProviderKind::Model);
    assert!(!form.toggle(PF_NON_MODEL));
    // 空格在文本框里仍是空格（名称可以带空格），在勾选框上才是切换
    form.active = PF_NAME;
    form.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert_eq!(form.text(PF_NAME), " ");

    form.active = PF_NON_MODEL;
    form.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(form.toggle(PF_NON_MODEL));
    form.handle_key(KeyEvent::from(KeyCode::Left));
    assert!(!form.toggle(PF_NON_MODEL), "← 也能切换");
    form.handle_key(KeyEvent::from(KeyCode::Right));
    assert!(form.toggle(PF_NON_MODEL), "→ 也能切换");
}

/// 勾上「非模型」→「探活路径」那一行当场消失（连同它的值一起隐去，
/// 取消勾选又回来）；Tab 也跳过它。
#[test]
fn non_model_toggle_hides_the_health_row() {
    let mut form = provider_add(crate::recipe::ProviderKind::Model);
    form.fields[PF_HEALTH] = Field::text("探活路径", "/v1/models");
    assert!(!form.fields[PF_HEALTH].is_hidden());

    form.active = PF_NON_MODEL;
    form.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(
        form.fields[PF_HEALTH].is_hidden(),
        "勾上非模型就不该再有这一行"
    );
    // Tab 从「非模型」直接跳到「脚本路径」
    form.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(form.active, PF_SCRIPT);
    // 隐藏的行写不进去（哪怕有人把光标摆上去）
    form.active = PF_HEALTH;
    form.handle_key(KeyEvent::from(KeyCode::Char('z')));
    assert_eq!(form.text(PF_HEALTH), "/v1/models");

    // 取消勾选：行回来，值还在（没有丢用户输入）
    form.active = PF_NON_MODEL;
    form.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(!form.fields[PF_HEALTH].is_hidden());
    assert_eq!(form.text(PF_HEALTH), "/v1/models");
}

/// 默认类型跟随分页：在非模型页按 a，勾选框已经勾上（探活那一行也就不出现）。
#[test]
fn provider_add_defaults_the_toggle_to_the_current_tab() {
    let model = provider_add(crate::recipe::ProviderKind::Model);
    assert!(!model.toggle(PF_NON_MODEL));
    assert!(!model.fields[PF_HEALTH].is_hidden());

    let non_model = provider_add(crate::recipe::ProviderKind::NonModel);
    assert!(non_model.toggle(PF_NON_MODEL));
    assert!(non_model.fields[PF_HEALTH].is_hidden());
    assert_eq!(
        visible_labels(&non_model),
        ["ID", "名称", "Base URL", "主页 URL", "非模型", "脚本路径"]
    );
}

/// 编辑表单：类型只读、非模型的探活行直接不出现（不是灰掉）。
#[test]
fn provider_edit_locks_kind_and_hides_health_for_non_model() {
    let mut recipe: crate::recipe::Recipe = serde_yaml::from_str(
        "id: deepl\nname: DeepL\nkind: non_model\nbase_url: 'https://api.deepl.com'\nauth: {kind: bearer}",
    )
    .unwrap();
    let mut form = provider_edit(&recipe, "");
    form.active = PF_NON_MODEL;
    form.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(form.toggle(PF_NON_MODEL), "只读勾选框不得被翻动");
    assert!(
        form.fields[PF_HEALTH].is_hidden(),
        "非模型的编辑表单里不该有探活路径这一行"
    );
    // Tab 也不往那一行走：名称 → … → 脚本路径
    assert_eq!(
        visible_labels(&form),
        ["ID", "名称", "Base URL", "主页 URL", "非模型", "脚本路径"]
    );

    // 模型厂商：同一行是可见、可编辑的探活路径
    recipe.kind = crate::recipe::ProviderKind::Model;
    let mut model = provider_edit(&recipe, "/models");
    assert!(!model.toggle(PF_NON_MODEL));
    assert!(!model.fields[PF_HEALTH].is_hidden());
    model.active = PF_HEALTH;
    model.handle_key(KeyEvent::from(KeyCode::Char('x')));
    assert_eq!(model.text(PF_HEALTH), "/modelsx");
}
