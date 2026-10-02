//! 一键导入面板测试的公共脚手架（内存 App + 断言小工具）；用例按主题分在
//! `flow.rs`（状态机 / 勾选 / 搜索 / 按键路由）与 `apply.rs`（写盘派发 / 回执 / 代际）。
//! 全部用 `test_app` 造内存 App，不碰真实配置。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Focus;
use crate::app::tests::test_app;
use crate::clients::codex::DEFAULT_EFFORT;
use crate::clients::{Agent, ImportReport, RestartReport};

mod apply;
mod flow;

// ---- 脚手架（两个子模块共用）-------------------------------------------

/// 与 `m` 键浏览一致的模型名列表（不再区分端点能力）。
fn mixed_entries() -> Vec<String> {
    vec![
        "gpt-6-sol".to_string(),
        "chat-only".to_string(),
        "deepseek-v4".to_string(),
    ]
}

fn open(app: &mut App, key_id: &str) {
    app.modal = Modal::Import(ImportFlow::new(key_id.to_string(), 1));
}

/// 当前面板的代际号（测试里几乎总是要传它给 apply/import_receive）。
fn seq_of(app: &App) -> u64 {
    flow(app).seq
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

fn picker_app() -> App {
    let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1"])]);
    app.focus = Focus::Keys;
    open(&mut app, "alpha.a1");
    let mut app_flow = ImportFlow::new("alpha.a1".into(), 1);
    app_flow.step = ImportStep::Models;
    app_flow.items = mixed_entries()
        .into_iter()
        .map(|name| ModelPick {
            name,
            checked: false,
        })
        .collect();
    app.modal = Modal::Import(app_flow);
    app
}

fn report(models: &[&str], model: &str) -> ImportReport {
    ImportReport {
        provider_key: "alpha".into(),
        model: Some(model.into()),
        models: models.iter().map(|name| (*name).to_string()).collect(),
        detail: Some(format!("强度 {DEFAULT_EFFORT}")),
        backups: Vec::new(),
    }
}

fn outcome(key_id: &str, result: Result<ImportReport, String>) -> ImportOutcome {
    ImportOutcome {
        agent: Agent::Codex,
        key_id: key_id.into(),
        seq: 1,
        result,
        restart: RestartReport::default(),
    }
}

/// 造一个「codex 现在真的在用这把密钥」的现场：把 `app` 的客户端配置目录填上 config.toml。
///
/// 这正是导入刚写完后的样子 —— ★ 现在是回读现场得出的，所以要测「打了 ★」就得先有现场。
fn codex_uses(app: &App, token: &str) {
    let home = app
        .agent_homes
        .get(&Agent::Codex)
        .expect("测试 App 有客户端配置目录");
    std::fs::write(
        home.join("config.toml"),
        format!(
            "model = \"gpt-6-sol\"\nmodel_provider = \"alpha\"\n\n\
             [model_providers.alpha]\nname = \"alpha\"\n\
             base_url = \"https://example.invalid/v1\"\nwire_api = \"responses\"\n\
             experimental_bearer_token = \"{token}\"\n"
        ),
    )
    .unwrap();
}

fn flow_mut(app: &mut App) -> &mut ImportFlow {
    match &mut app.modal {
        Modal::Import(flow) => flow,
        other => panic!("期望导入面板，实际 {other:?}"),
    }
}
