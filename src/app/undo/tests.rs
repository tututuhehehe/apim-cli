use super::*;
use crate::app::{Focus, Modal, tests::test_app};
use crate::form::{Field, Form};
use crate::recipe::ScriptSpec;
use std::collections::HashMap;
use std::path::Path;

/// 测试用 App：临时 config_dir，绝不影响真实 ~/.config/apim。
fn app_with(providers: &[(&str, &[&str])], dir: &str) -> App {
    let (mut app, _rx, _rx_models) = test_app(providers);
    app.config_dir = crate::app::tests::test_config_dir(dir);
    app
}

fn key_form(alias: &str, token: &str) -> Form {
    let mut form = crate::form::key_add(vec!["p".into()], 0);
    form.fields[1] = Field::text("别名", alias);
    form.fields[2] = Field::text("分组", "");
    form.fields[3] = Field::text("密钥", token);
    form
}

fn provider_form(id: &str, name: &str, base: &str, script: &str) -> Form {
    let mut form = crate::form::provider_add();
    form.fields[0] = Field::text("ID", id);
    form.fields[1] = Field::text("名称", name);
    form.fields[2] = Field::text("Base URL", base);
    form.fields[3] = Field::text("主页 URL", "");
    form.fields[4] = Field::text("探活路径", "");
    form.fields[5] = Field::text("脚本路径", script);
    form
}

/// 从磁盘读回密钥（走真实解析路径，确认内存与磁盘一致）。
fn on_disk(app: &App) -> Vec<KeyEntry> {
    config::load_keys_from(&app.config_dir, &app.recipes).unwrap()
}

#[test]
fn empty_history_just_toasts() {
    let mut app = app_with(&[("p", &["main"])], "empty");
    app.undo();
    assert_eq!(app.toast_text(), Some("没有可撤销的操作"));
    assert_eq!(app.keys.len(), 1, "空历史不能动数据");
}

#[tokio::test]
async fn undo_key_add_removes_key_from_memory_and_disk() {
    let mut app = app_with(&[("p", &[])], "key-add");
    app.save_key_form(&key_form("main", "sk-added"), None);
    assert_eq!(app.keys.len(), 1);
    assert_eq!(on_disk(&app).len(), 1);
    assert_eq!(app.undo_stack.len(), 1);

    app.undo();
    assert!(app.keys.is_empty());
    assert!(on_disk(&app).is_empty(), "磁盘也要回退");
    assert!(app.undo_stack.is_empty(), "撤销后历史条目出栈");
}

#[tokio::test]
async fn undo_key_rename_restores_old_alias_and_token() {
    let mut app = app_with(&[("p", &["main"])], "key-rename");
    app.save_key_form(&key_form("work", "sk-new"), Some("p.main"));
    assert_eq!(app.keys[0].id(), "p.work");

    app.undo();
    assert_eq!(app.keys.len(), 1);
    assert_eq!(app.keys[0].id(), "p.main", "别名与 id 都要回到旧值");
    assert_eq!(app.keys[0].token, "sk-test-placeholder", "旧 token 要还原");
    let disk = on_disk(&app);
    assert_eq!(disk[0].id(), "p.main");
    assert_eq!(disk[0].token, "sk-test-placeholder");
}

#[test]
fn undo_key_delete_restores_entry() {
    let mut app = app_with(&[("p", &["main"])], "key-del");
    app.modal = Modal::ConfirmDeleteKey {
        key_id: "p.main".into(),
    };
    app.confirm_delete_key();
    assert!(app.keys.is_empty());

    app.undo();
    assert_eq!(app.keys.len(), 1);
    assert_eq!(app.keys[0].token, "sk-test-placeholder");
    assert_eq!(on_disk(&app).len(), 1);
}

#[test]
fn undo_provider_add_removes_user_yaml() {
    let mut app = app_with(&[("p", &[])], "provider-add");
    app.focus = Focus::Providers;
    app.save_provider_form(
        &provider_form("relay", "中转", "https://r.example.com", ""),
        None,
    );
    let yaml = app.config_dir.join("recipes/relay.yaml");
    assert!(yaml.is_file());
    assert!(app.recipes.contains_key("relay"));

    app.undo();
    assert!(!yaml.exists(), "用户 YAML 要删掉");
    assert!(!app.recipes.contains_key("relay"));
    assert!(!app.provider_ids.contains(&"relay".to_string()));
}

#[test]
fn undo_provider_edit_that_overrode_builtin_restores_builtin() {
    let mut app = app_with(&[("p", &[])], "provider-edit-builtin");
    app.focus = Focus::Providers;
    // p 的 recipe origin=None（等价内置）：编辑会在用户目录生成覆盖 YAML
    app.save_provider_form(
        &provider_form("p", "改过的名字", "https://p2.example", ""),
        Some("p"),
    );
    let yaml = app.config_dir.join("recipes/p.yaml");
    assert!(yaml.is_file(), "覆盖内置应生成用户 YAML");
    assert_eq!(app.recipes["p"].name, "改过的名字");

    app.undo();
    assert!(!yaml.exists(), "撤销覆盖 = 删用户 YAML");
    assert_eq!(app.recipes["p"].name, "p 假厂商", "内置定义恢复");
    assert_eq!(app.recipes["p"].base_url, "https://example.invalid");
    assert!(app.recipes["p"].origin.is_none());
}

#[test]
fn undo_provider_delete_rewrites_yaml() {
    let mut app = app_with(&[("p", &[])], "provider-del");
    app.focus = Focus::Providers;
    app.save_provider_form(
        &provider_form("relay", "中转", "https://r.example.com", ""),
        None,
    );
    let yaml = app.config_dir.join("recipes/relay.yaml");
    // 先清掉 add 那步，只留删除这一步在历史里
    app.undo_stack.clear();

    app.modal = Modal::ConfirmDeleteProvider {
        provider_id: "relay".into(),
        path: yaml.clone(),
    };
    app.confirm_delete_provider();
    assert!(!yaml.exists());

    app.undo();
    assert!(yaml.is_file(), "撤销删除要写回 YAML");
    assert!(app.recipes.contains_key("relay"));
    assert_eq!(app.recipes["relay"].origin.as_deref(), Some(yaml.as_path()));
}

#[test]
fn undo_provider_copy_removes_yaml_and_script_copy() {
    let mut app = app_with(&[("p", &[])], "provider-copy");
    app.focus = Focus::Providers;
    // 虚拟一个绑定脚本的厂商，并造出真实脚本文件
    let scripts = app.config_dir.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    let src_script = scripts.join("p-quota.sh");
    fs::write(&src_script, "#!/bin/sh\necho q\n").unwrap();
    let mut recipe = app.recipes["p"].clone();
    recipe.balance = Some(ScriptSpec {
        command: Some(src_script.display().to_string()),
        run: None,
        shell: None,
        timeout_secs: None,
    });
    app.recipes.insert("p".into(), recipe);
    app.selected_provider = 0;

    app.duplicate_selected_provider();
    let new_yaml = app.config_dir.join("recipes/p-copy.yaml");
    let new_script = scripts.join("p-copy-quota.sh");
    assert!(new_yaml.is_file() && new_script.is_file());

    app.undo();
    assert!(!new_yaml.exists(), "复制出的 YAML 要删掉");
    assert!(!new_script.exists(), "复制出的脚本副本也要删掉");
    assert!(src_script.exists(), "原脚本不能动");
    assert!(!app.recipes.contains_key("p-copy"));
}

#[test]
fn history_is_capped_and_undo_walks_back_in_order() {
    let mut app = app_with(&[("p", &[])], "cap");
    for i in 0..(MAX_UNDO + 5) {
        app.push_undo(UndoAction::KeyAdded {
            key_id: format!("p.k{i}"),
        });
    }
    assert_eq!(app.undo_stack.len(), MAX_UNDO, "超出上限丢最旧的");
    assert_eq!(
        app.undo_stack.front().map(UndoAction::label).unwrap(),
        format!("新增密钥 p.k5"),
        "保留最近 N 步"
    );
}

#[test]
fn labels_read_naturally() {
    let before = KeyEntry {
        provider: "p".into(),
        alias: "a".into(),
        group: None,
        token: "sk-x".into(),
    };
    let deleted = Recipe {
        id: "relay".into(),
        name: "中转".into(),
        base_url: "https://r".into(),
        homepage: None,
        models_url: None,
        supports_groups: false,
        vars: HashMap::new(),
        auth: Default::default(),
        health: None,
        balance: None,
        origin: Some(Path::new("/tmp/x.yaml").to_path_buf()),
    };
    assert_eq!(
        UndoAction::KeyAdded {
            key_id: "p.a".into()
        }
        .label(),
        "新增密钥 p.a"
    );
    assert_eq!(
        UndoAction::KeyUpdated {
            key_id: "p.a".into(),
            before
        }
        .label(),
        "修改密钥 p.a"
    );
    assert_eq!(
        UndoAction::ProviderCreated {
            id: "relay".into(),
            script_copy: None,
            copied: true
        }
        .label(),
        "复制厂商 relay"
    );
    assert_eq!(
        UndoAction::ProviderDeleted { recipe: deleted }.label(),
        "删除厂商 relay"
    );
}
