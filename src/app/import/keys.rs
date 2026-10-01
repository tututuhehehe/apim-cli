//! 一键导入面板的按键路由（按步骤分发）。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, ImportStep};

/// 面板里的按键路由。返回 true 表示这个按键已被面板消费。
pub(crate) fn handle_import_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(step) = app.import_flow().map(|flow| flow.step) else {
        return false;
    };
    let searching = app.import_searching();
    match step {
        ImportStep::Agent => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.import_move_agent(1),
            KeyCode::Char('k') | KeyCode::Up => app.import_move_agent(-1),
            KeyCode::Enter => app.import_choose_agent(),
            KeyCode::Esc | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
        ImportStep::Models if searching => match key.code {
            KeyCode::Enter => app.import_exit_search(),
            KeyCode::Esc => app.import_exit_search(),
            KeyCode::Backspace => app.import_search_backspace(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.import_search_char(c)
            }
            _ => return false,
        },
        ImportStep::Models => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.import_move(1),
            KeyCode::Char('k') | KeyCode::Up => app.import_move(-1),
            KeyCode::Char(' ') => app.import_toggle(),
            KeyCode::Char('a') => app.import_toggle_all(),
            KeyCode::Char('/') => app.import_start_search(),
            KeyCode::Enter => app.import_confirm_models(),
            KeyCode::Esc | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
        // 默认模型：选完开写；h/← 退回上一步重选
        ImportStep::DefaultModel => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.import_move_default(1),
            KeyCode::Char('k') | KeyCode::Up => app.import_move_default(-1),
            KeyCode::Enter => app.import_confirm_default(),
            KeyCode::Char('h') | KeyCode::Left => app.import_default_back(),
            KeyCode::Esc | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
        // 正在写配置：忽略按键；Esc 允许关面板（任务继续，结果走 toast）
        ImportStep::Working => match key.code {
            KeyCode::Esc => app.cancel_modal(),
            _ => return false,
        },
        ImportStep::Failed => match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
    }
    true
}
