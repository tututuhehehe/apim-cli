//! 事件循环、按键路由、快照渲染。

use std::time::Instant;

use anyhow::Result;
use crossterm::event::{
    EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

use crate::app::{
    App, Focus, ImportFlow, ImportStep, KeyState, Modal, ModelPick, TaskMsg, handle_import_key,
};
use crate::form::{Field, FormEvent};
use crate::probe::{BalanceSnapshot, Health};
use crate::ui;

pub(crate) async fn run_tui() -> Result<()> {
    let (mut app, mut rx, mut rx_task) = App::start()?;
    let mut terminal = ratatui::init();
    let _ = execute!(terminal.backend_mut(), EnableBracketedPaste);
    let mut events = EventStream::new();
    let result = loop_tui(&mut terminal, &mut app, &mut rx, &mut rx_task, &mut events).await;
    ratatui::restore();
    result
}

async fn loop_tui(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::app::ProbeMsg>,
    rx_models: &mut tokio::sync::mpsc::UnboundedReceiver<TaskMsg>,
    events: &mut EventStream,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break; };
                match event? {
                    Event::Paste(text) => {
                        if let Some(form) = app.form() {
                            form.handle_paste(&text);
                        } else {
                            app.search_paste(&text);
                        }
                    }
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        let was_modal = !matches!(app.modal, Modal::None);
                        // 无弹窗 Esc 优先清过滤；清掉了就不能当「退出」处理
                        let esc_clears_filter =
                            !was_modal && key.code == KeyCode::Esc && app.has_filter();
                        handle_key(app, key);
                        if !was_modal
                            && matches!(app.modal, Modal::None)
                            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                            && !esc_clears_filter
                        {
                            break;
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            msg = rx.recv() => {
                if let Some(msg) = msg {
                    app.apply(msg);
                }
            }
            msg = rx_models.recv() => {
                if let Some(msg) = msg {
                    // 弹窗可能已被关掉/换了把密钥打开：两边都按 key_id 匹配，不匹配丢弃
                    match msg {
                        TaskMsg::Models(key_id, result) => app.apply_models(key_id, result),
                        TaskMsg::Import(outcome) => app.import_result(*outcome),
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                app.tick();
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    match &mut app.modal {
        Modal::Form { form, .. } => match form.handle_key(key) {
            FormEvent::Save => app.save_form(),
            FormEvent::Cancel => app.cancel_modal(),
            FormEvent::None => {}
        },
        Modal::Search { edit, .. } => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return;
            }
            match key.code {
                KeyCode::Enter => app.apply_search(),
                KeyCode::Esc => app.cancel_search(),
                KeyCode::Backspace => {
                    edit.backspace();
                    app.apply_live_filter();
                }
                KeyCode::Delete => {
                    edit.delete();
                    app.apply_live_filter();
                }
                KeyCode::Left => edit.left(),
                KeyCode::Right => edit.right(),
                KeyCode::Home => edit.home(),
                KeyCode::End => edit.end(),
                KeyCode::Char(c) => {
                    edit.insert(&c.to_string());
                    app.apply_live_filter();
                }
                _ => {}
            }
        }
        Modal::ConfirmDeleteKey { .. } | Modal::ConfirmDeleteProvider { .. } => match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('d') => app.confirm_delete(),
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => app.cancel_modal(),
            _ => {}
        },
        Modal::Inspector { .. } => match key.code {
            // r 只切遮掩/完整（方法内部限定密钥详情且有 token），c 复制完整 token。
            // Ctrl 组合不拦（同主界面的 Ctrl+C）：避免终端中断手势误把密钥写上剪贴板
            KeyCode::Char('r') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.inspector_toggle_reveal()
            }
            KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.inspector_copy_token()
            }
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.cancel_modal(),
            _ => {}
        },
        Modal::Models { .. } => {
            // 双态：搜索输入态（/ 进入）里字符进过滤器、Esc/Enter 退回列表态；
            // 列表态 j/k 移动、c 复制、/ 再搜索、Esc/Enter/q 关闭。
            let searching = app.models_is_searching();
            match key.code {
                KeyCode::Esc => {
                    if searching {
                        app.models_exit_search();
                    } else {
                        app.cancel_modal();
                    }
                }
                KeyCode::Enter if searching => app.models_exit_search(),
                KeyCode::Enter | KeyCode::Char('q') => app.cancel_modal(),
                KeyCode::Char('/') if !searching => app.models_start_search(),
                KeyCode::Char('j') | KeyCode::Down if !searching => app.move_models_selection(1),
                KeyCode::Char('k') | KeyCode::Up if !searching => app.move_models_selection(-1),
                KeyCode::Char('c') if !searching => app.copy_selected_model(),
                KeyCode::Backspace if searching => app.models_search_backspace(),
                KeyCode::Char(c) if searching && !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.models_search_char(c)
                }
                _ => {}
            }
        }
        Modal::Import(_) => {
            handle_import_key(app, key);
        }
        Modal::None => match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {}
            // Ctrl+Z 撤销最近一次写操作（增删改厂商/密钥）
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => app.undo(),
            KeyCode::Char('q') => {}
            KeyCode::Esc => {
                app.clear_filter();
            }
            KeyCode::Char('/') => app.open_search(),
            KeyCode::Enter if app.focus == Focus::Providers => app.open_homepage(),
            KeyCode::Char('m') if app.focus == Focus::Keys => app.open_models(),
            KeyCode::Char('c') => app.copy_selected(),
            KeyCode::Char('r') => app.refresh_current_provider(),
            KeyCode::Char('a') => app.open_add(),
            KeyCode::Char('e') => app.open_edit(),
            KeyCode::Char('y') if app.focus == Focus::Providers => {
                app.duplicate_selected_provider()
            }
            KeyCode::Char('i') => app.open_inspector(),
            // x：把当前密钥 + 厂商 + 勾选的模型一键导入到 Codex
            KeyCode::Char('x') => app.open_import(),
            KeyCode::Char('d') => app.open_delete(),
            KeyCode::Char('j') | KeyCode::Down => app.move_down(),
            KeyCode::Char('k') | KeyCode::Up => app.move_up(),
            // vim 方向：h 左 = 厂商栏，l 右 = 密钥栏；已在边缘侧时不动
            KeyCode::Char('h') if app.focus == Focus::Keys => app.toggle_focus(),
            KeyCode::Char('l') if app.focus == Focus::Providers => app.toggle_focus(),
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => app.toggle_focus(),
            _ => {}
        },
    }
}

// ---- 快照（测试用） ----------------------------------------------------

pub(crate) async fn run_snapshot() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    if let Ok(id) = std::env::var("APIM_SNAPSHOT_PROVIDER")
        && let Some(pos) = app.provider_ids.iter().position(|p| p == &id)
    {
        app.selected_provider = pos;
    }
    app.refresh_blocking().await;
    render_snapshot(&app).await
}

pub(crate) async fn run_snapshot_key_form() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    app.focus = Focus::Keys;
    app.open_add();
    if let Modal::Form { form, .. } = &mut app.modal {
        form.fields[1] = Field::text("别名", "work");
        form.fields[2] = Field::text("分组", "个人");
        form.fields[3] = Field::text("密钥", "sk-pasted-example-token-12345");
    }
    render_snapshot(&app).await
}

pub(crate) async fn run_snapshot_provider_form() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    app.focus = Focus::Providers;
    app.open_add();
    if let Modal::Form { form, .. } = &mut app.modal {
        form.fields[0] = Field::text("ID", "my-relay");
        form.fields[1] = Field::text("名称", "我的中转站");
        form.fields[2] = Field::text("Base URL", "https://relay.example.com");
        form.fields[3] = Field::text("主页 URL", "https://console.example.com");
        form.fields[4] = Field::text("探活路径", "/v1/models");
        form.fields[5] = Field::text("脚本路径", "~/.config/apim/scripts/my-relay.sh");
        form.active = 5;
    }
    render_snapshot(&app).await
}

/// 一键导入面板快照（第一步：选客户端）。不拉接口、不写盘，直接摆出面板状态。
pub(crate) async fn run_snapshot_import() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    app.focus = Focus::Keys;
    app.codex = Some(crate::clients::CodexState {
        provider: "ikun".into(),
        provider_name: "ikun".into(),
        alias: "codex".into(),
        provider_key: "ikun".into(),
        models: vec!["gpt-6-sol".into()],
        default_model: "gpt-6-sol".into(),
        reasoning_effort: crate::clients::DEFAULT_EFFORT.into(),
    });
    app.modal = Modal::Import(ImportFlow::new(snapshot_key_id(&app)));
    render_snapshot(&app).await
}

/// 一键导入面板快照（第二步：勾选模型）。包含一个厂商声明不支持 responses 的模型。
pub(crate) async fn run_snapshot_import_models() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    app.focus = Focus::Keys;
    let mut flow = ImportFlow::new(snapshot_key_id(&app));
    flow.step = ImportStep::Models;
    flow.items = vec![
        ModelPick {
            name: "gpt-6-sol".into(),
            responses: Some(true),
            checked: true,
        },
        ModelPick {
            name: "gpt-6.1-sol".into(),
            responses: Some(true),
            checked: true,
        },
        ModelPick {
            name: "deepseek-v4".into(),
            responses: Some(true),
            checked: false,
        },
        ModelPick {
            name: "text-embedding-3-large".into(),
            responses: Some(false),
            checked: false,
        },
    ];
    app.modal = Modal::Import(flow);
    render_snapshot(&app).await
}

/// 一键导入面板快照（第三步：从已勾选模型里选默认模型）。
pub(crate) async fn run_snapshot_import_default() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    app.focus = Focus::Keys;
    let mut flow = ImportFlow::new(snapshot_key_id(&app));
    flow.step = ImportStep::DefaultModel;
    flow.items = vec![
        ModelPick {
            name: "gpt-6-sol".into(),
            responses: Some(true),
            checked: true,
        },
        ModelPick {
            name: "gpt-6.1-sol".into(),
            responses: Some(true),
            checked: true,
        },
        ModelPick {
            name: "deepseek-v4".into(),
            responses: Some(true),
            checked: false,
        },
    ];
    flow.default_cursor = 1;
    app.modal = Modal::Import(flow);
    render_snapshot(&app).await
}

/// 快照用的密钥 id：有真实密钥就用它，没有就用占位（快照不依赖配置目录内容）。
fn snapshot_key_id(app: &App) -> String {
    app.selected_key_entry()
        .map(|key| key.id())
        .unwrap_or_else(|| "ikun.codex".into())
}

/// 检查器快照：不拉真实接口，给选中密钥塞一份假探测状态后渲染详情弹窗。
/// 默认密钥详情；`APIM_SNAPSHOT_INSPECTOR=provider` 出厂商详情。
/// 数据来自 APIM_CONFIG_DIR（测试时指向假配置目录）。
pub(crate) async fn run_snapshot_inspector() -> Result<()> {
    let (mut app, _rx, _rx_task) = App::start()?;
    if let Ok(id) = std::env::var("APIM_SNAPSHOT_PROVIDER")
        && let Some(pos) = app.provider_ids.iter().position(|p| p == &id)
    {
        app.selected_provider = pos;
    }
    if let Some(key) = app.selected_key_entry().cloned() {
        // 清掉启动自动探测的在途标记，否则快照里「健康」行永远显示「检查中」，
        // 盖住我们注入的假探测结果
        app.probe_seq.clear();
        app.states.insert(
            key.id(),
            KeyState {
                health: Health::Live { ms: 120 },
                balance: Some(BalanceSnapshot {
                    lines: Some(vec!["¥ 4.22".into()]),
                    endpoint: "GET https://api.example.invalid/user/balance".into(),
                    status: Some(200),
                    elapsed_ms: 88,
                    error: None,
                }),
                updated: Some(Instant::now()),
            },
        );
    }
    app.focus = if std::env::var("APIM_SNAPSHOT_INSPECTOR").as_deref() == Ok("provider") {
        Focus::Providers
    } else {
        Focus::Keys
    };
    app.open_inspector();
    render_snapshot(&app).await
}

async fn render_snapshot(app: &App) -> Result<()> {
    let backend = TestBackend::new(112, 30);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, app))?;
    print!("{}", buffer_to_string(terminal.backend().buffer()));
    Ok(())
}

fn buffer_to_string(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        let mut skip = 0u16;
        for x in 0..buf.area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let cell = &buf[(x, y)];
            let sym = cell.symbol();
            line.push_str(sym);
            let width = UnicodeWidthStr::width(sym) as u16;
            if width > 1 {
                skip = width - 1;
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests;
