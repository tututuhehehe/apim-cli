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
            path: Path::new("/tmp/relay.yaml").to_path_buf(),
            script_copy: None,
            copied: true
        }
        .label(),
        "复制厂商 relay"
    );
    // 带脚本副本时 toast 要说明脚本名（否则用户不知道文件一起没了）
    assert_eq!(
        UndoAction::ProviderCreated {
            id: "relay".into(),
            path: Path::new("/tmp/relay.yaml").to_path_buf(),
            script_copy: Some(Path::new("/tmp/relay-copy-quota.sh").to_path_buf()),
            copied: true
        }
        .label(),
        "复制厂商 relay（含脚本 relay-copy-quota.sh）"
    );
    assert_eq!(
        UndoAction::ProviderDeleted { recipe: deleted }.label(),
        "删除厂商 relay"
    );
}

// ---- 补充：失败序 / 多步时序 / 状态清理 / 幂等 ----

/// 编辑“已有用户 YAML”的厂商（ProviderUpdated 的 else 分支）：撤销要按旧内容重写。
#[test]
fn undo_provider_edit_of_user_recipe_rewrites_old_content() {
    let mut app = app_with(&[("p", &[])], "edit-user");
    app.focus = Focus::Providers;
    // 先真存一份用户 YAML（origin=Some）
    app.save_provider_form(
        &provider_form("relay", "原名", "https://a.example.com", ""),
        None,
    );
    app.undo_stack.clear();
    // 再编辑它
    app.save_provider_form(
        &provider_form("relay", "改名", "https://b.example.com", ""),
        Some("relay"),
    );
    assert_eq!(app.recipes["relay"].name, "改名");
    let yaml = app.config_dir.join("recipes/relay.yaml");
    assert!(fs::read_to_string(&yaml).unwrap().contains("改名"));

    app.undo();
    assert_eq!(app.recipes["relay"].name, "原名", "旧内容要写回");
    assert_eq!(app.recipes["relay"].base_url, "https://a.example.com");
    let text = fs::read_to_string(&yaml).unwrap();
    assert!(
        text.contains("原名") && !text.contains("改名"),
        "YAML 要回退: {text}"
    );
    assert!(app.recipes["relay"].origin.is_some(), "仍是用户 recipe");
    // 磁盘回读应与内存一致
    let mut reloaded = HashMap::new();
    crate::recipe::load_dir(&app.config_dir.join("recipes"), &mut reloaded).unwrap();
    assert_eq!(reloaded["relay"].name, "原名");
}

/// 撤销厂商配置变更后，该厂商的缓存探活/额度状态必须清掉（旧数字不可信）。
#[tokio::test]
async fn undo_provider_edit_drops_stale_states_but_keeps_keys() {
    let mut app = app_with(&[("p", &["main"])], "drop-states");
    app.focus = Focus::Providers;
    app.states.insert(
        "p.main".into(),
        crate::app::KeyState {
            health: crate::probe::Health::Live { ms: 42 },
            ..Default::default()
        },
    );
    // 模拟旧配置的探针在途（代际 1；发号器同步推到 1，撤销后重探会拿到新代际）
    app.next_probe_seq = 1;
    app.probe_seq.insert("p.main".into(), 1);
    app.save_provider_form(
        &provider_form("p", "改名", "https://p2.example.com", ""),
        Some("p"),
    );
    app.undo();
    assert_eq!(app.recipes["p"].name, "p 假厂商", "配置要回退");
    assert_eq!(app.keys.len(), 1, "撤销厂商配置不应动密钥");
    // 撤销后按回退后的配置重探：旧读数已被清掉，状态是「检查中」而不是旧的 Live{42}
    assert!(app.is_checking("p.main"), "应重新发起探测");
    assert!(
        matches!(app.states["p.main"].health, crate::probe::Health::Checking),
        "旧读数要清掉"
    );
    // 旧代际（1）的迟到结果不得覆盖新探针
    let new_seq = app.probe_seq["p.main"];
    assert_ne!(new_seq, 1, "重探必须用新代际");
    app.apply((
        1,
        crate::probe::ProbeResult {
            key_id: "p.main".into(),
            health: crate::probe::Health::Live { ms: 42 },
            balance: None,
        },
    ));
    assert!(
        matches!(app.states["p.main"].health, crate::probe::Health::Checking),
        "旧代际结果不得回填旧数字"
    );
}

/// 多步时序：复制 → 编辑副本 → 撤销编辑 → 撤销复制（LIFO，文件逐步清干净）。
#[test]
fn undo_walks_back_copy_then_edit_in_order() {
    let mut app = app_with(&[("p", &[])], "multi-step");
    app.focus = Focus::Providers;
    app.duplicate_selected_provider();
    let copied_yaml = app.config_dir.join("recipes/p-copy.yaml");
    assert!(copied_yaml.is_file());

    app.focus = Focus::Providers;
    app.save_provider_form(
        &provider_form("p-copy", "改了", "https://c.example.com", ""),
        Some("p-copy"),
    );
    assert_eq!(app.recipes["p-copy"].name, "改了");

    app.undo(); // 撤销编辑：副本回到复制时的内容
    assert_eq!(app.recipes["p-copy"].name, "p 假厂商 副本");
    assert!(copied_yaml.is_file(), "副本文件还在");

    app.undo(); // 撤销复制：YAML 与内存条目都清掉
    assert!(!copied_yaml.exists(), "复制出的 YAML 要删掉");
    assert!(!app.recipes.contains_key("p-copy"));
    assert!(app.undo_stack.is_empty());
    assert!(!app.provider_ids.contains(&"p-copy".to_string()));
}

/// 外部已把文件删了：撤销要幂等成功（不能报错卡住）。
#[test]
fn undo_is_idempotent_when_target_file_already_gone() {
    let mut app = app_with(&[("p", &[])], "idempotent");
    app.focus = Focus::Providers;
    app.save_provider_form(
        &provider_form("relay", "中转", "https://r.example.com", ""),
        None,
    );
    let yaml = app.config_dir.join("recipes/relay.yaml");
    fs::remove_file(&yaml).unwrap(); // 模拟被外部删掉/同步工具移走
    app.undo();
    assert!(
        app.toast_text().unwrap().starts_with("已撤销"),
        "已不存在的文件不应让撤销失败: {:?}",
        app.toast_text()
    );
    assert!(!app.recipes.contains_key("relay"));
    assert!(app.undo_stack.is_empty(), "成功后历史条目出栈");
}

/// 跨厂商改名：KeyUpdated 记的是改后 id，撤销后旧 provider 的条目要回来。
#[tokio::test]
async fn undo_key_rename_across_providers() {
    let mut app = app_with(&[("p", &["main"]), ("q", &[])], "cross-provider");
    let mut form = key_form("main", "sk-moved");
    form.fields[0] = Field::select("厂商", vec!["p".into(), "q".into()], 1, "←/→ 切换");
    app.save_key_form(&form, Some("p.main"));
    assert_eq!(app.keys[0].id(), "q.main");

    app.undo();
    assert_eq!(app.keys.len(), 1, "不能留下 q.main + p.main 两条");
    assert_eq!(app.keys[0].id(), "p.main");
    assert_eq!(on_disk(&app)[0].id(), "p.main");
}
