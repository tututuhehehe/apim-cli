//! 一键导入面板测试的公共脚手架（内存 App + 断言小工具）；用例按主题分在
//! `flow.rs`（状态机 / 勾选 / 搜索 / 按键路由）与 `apply.rs`（写盘派发 / 回执 / 代际）。
//! 全部用 `test_app` 造内存 App，不碰真实配置。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::Focus;
use crate::app::tests::test_app;
use crate::clients::{Agent, DEFAULT_EFFORT, ImportReport, LastImport, RestartReport};

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
        model: model.into(),
        models: models.iter().map(|name| (*name).to_string()).collect(),
        reasoning_effort: DEFAULT_EFFORT.into(),
        backup_path: None,
    }
}

fn outcome(key_id: &str, result: Result<ImportReport, String>) -> ImportOutcome {
    ImportOutcome {
        agent: Agent::Codex,
        key_id: key_id.into(),
        seq: 1,
        result,
        last_import: None,
        state_error: None,
        restart: RestartReport::default(),
    }
}

/// 测试用的「上次导入」摘要。
fn last_import(key_id: &str) -> LastImport {
    LastImport {
        key_id: key_id.into(),
        summary: format!("alpha（表名 alpha）· 默认 gpt-6-sol · 强度 {DEFAULT_EFFORT} · 1 个模型"),
    }
}

fn flow_mut(app: &mut App) -> &mut ImportFlow {
    match &mut app.modal {
        Modal::Import(flow) => flow,
        other => panic!("期望导入面板，实际 {other:?}"),
    }
}
