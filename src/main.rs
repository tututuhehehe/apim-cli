mod app;
mod clipboard;
mod config;
mod form;
mod probe;
mod recipe;
mod ui;

use anyhow::Result;
use crossterm::event::{
    EnableBracketedPaste, Event, EventStream, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Modal};
use crate::form::FormEvent;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args();
    let flag = args.find(|a| a.starts_with("--snapshot"));
    match flag.as_deref() {
        Some("--snapshot-form") => run_snapshot_form().await,
        Some("--snapshot") => run_snapshot().await,
        _ => run_tui().await,
    }
}

async fn run_tui() -> Result<()> {
    let (mut app, mut rx) = app::start()?;
    let mut terminal = ratatui::init();
    let _ = execute!(terminal.backend_mut(), EnableBracketedPaste);
    let mut events = EventStream::new();
    let result = loop_tui(&mut terminal, &mut app, &mut rx, &mut events).await;
    ratatui::restore();
    result
}

async fn loop_tui(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<probe::ProbeResult>,
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
                        }
                    }
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        handle_key(app, key);
                        if matches!(app.modal, Modal::None)
                            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
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
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                app.tick();
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) {
    match &mut app.modal {
        Modal::Form(form) => match form.handle_key(key) {
            FormEvent::Save => {
                let form = form.clone();
                app.save_form(&form);
            }
            FormEvent::Cancel => app.cancel_modal(),
            FormEvent::None => {}
        },
        Modal::ConfirmDelete { .. } => match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('d') => app.confirm_delete(),
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => app.cancel_modal(),
            _ => {}
        },
        Modal::None => match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {}
            KeyCode::Char('q') | KeyCode::Esc => {}
            KeyCode::Char('c') => app.copy_selected(),
            KeyCode::Char('r') => app.refresh_current_provider(),
            KeyCode::Char('a') => app.open_add(),
            KeyCode::Char('e') => app.open_edit(),
            KeyCode::Char('d') => app.open_delete(),
            KeyCode::Char('j') | KeyCode::Down => app.move_down(),
            KeyCode::Char('k') | KeyCode::Up => app.move_up(),
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => app.toggle_focus(),
            _ => {}
        },
    }
}

async fn run_snapshot() -> Result<()> {
    let (mut app, _rx) = app::start()?;
    app.refresh_blocking().await;
    render_snapshot(&app).await
}

async fn run_snapshot_form() -> Result<()> {
    let (mut a, _rx) = app::start()?;
    a.open_add();
    if let app::Modal::Form(form) = &a.modal {
        let mut filled = form.clone();
        filled.alias.value = "work".into();
        filled.alias.cursor = 4;
        filled.group.value = "个人".into();
        filled.group.cursor = 2;
        filled.token.value = "sk-pasted-example-token-12345".into();
        filled.token.cursor = 29;
        a.modal = app::Modal::Form(filled);
    }
    render_snapshot(&a).await
}

async fn render_snapshot(app: &App) -> Result<()> {
    let backend = TestBackend::new(112, 28);
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
