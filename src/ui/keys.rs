//! 右侧密钥表 + 状态标签。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table, TableState};

use super::{App, Focus, pane_block, theme};
use crate::config::KeyEntry;
use crate::probe::Health;

pub(crate) fn draw_keys(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let focused = app.focus == Focus::Keys;
    let recipe = app.current_recipe();
    let name = recipe.map(|r| r.name.as_str()).unwrap_or("—");
    let rows_idx = app.keys_in_provider_filtered();
    let n = rows_idx.len();
    let total = app.keys_in_provider().len();
    let count = if n == total {
        format!("{n} 个密钥")
    } else {
        format!("{n}/{total} 个密钥")
    };
    let headline = app
        .selected_key_entry()
        .and_then(|key| {
            app.state_for(key)
                .balance
                .and_then(|b| b.lines)
                .and_then(|lines| lines.first().cloned())
        })
        .unwrap_or_else(|| "—".into());

    let block = pane_block(format!(" {name}  ·  {count}  ·  {headline} "), focused);
    let empty_inner = block.inner(area);

    let header = Row::new(["#", "别名", "分组", "密钥", "状态"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = rows_idx
        .into_iter()
        .enumerate()
        .map(|(i, idx)| {
            let key = &app.keys[idx];
            let (style, label) = status_label(app, key);
            // ★ = 这把密钥是 apim 上次导入到 Codex 的那把
            let alias = if app.is_codex_active(&key.id()) {
                Cell::from(Span::styled(
                    format!("★ {}", key.alias),
                    Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
                ))
            } else {
                Cell::from(key.alias.clone())
            };
            Row::new([
                Cell::from(format!("{}", i + 1)),
                alias,
                Cell::from(key.group_label().to_string()),
                Cell::from(key.masked_token()),
                Cell::from(Span::styled(label, style)),
            ])
            .style(Style::new().fg(theme::TEXT))
        })
        .collect();

    let mut state = TableState::default();
    if n > 0 {
        state.select(Some(app.selected_key));
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(block)
    .highlight_symbol("▶ ")
    .row_highlight_style(
        Style::new()
            .bg(theme::HIGHLIGHT_BG)
            .add_modifier(Modifier::BOLD),
    )
    .column_spacing(1);

    frame.render_stateful_widget(table, area, &mut state);

    if n == 0 {
        let hint = if app.key_filter.is_some() {
            " 没有匹配的密钥，Esc 清除过滤 "
        } else if app.provider_filter.is_some() {
            // 厂商全被滤掉时没有选中厂商，别误导用户去按 a
            " 没有匹配的厂商，Esc 清除过滤 "
        } else {
            " 该厂商还没有密钥，按 a 添加 "
        };
        let hint = Paragraph::new(hint)
            .style(Style::new().fg(theme::MUTED))
            .alignment(Alignment::Center);
        let inner = Rect {
            y: area.y + area.height / 2,
            ..empty_inner
        };
        frame.render_widget(hint, inner);
    }
}

fn status_label(app: &App, key: &KeyEntry) -> (Style, String) {
    if app.is_checking(&key.id()) {
        return (Style::new().fg(theme::MUTED), "… 检查中".into());
    }
    let state = app.state_for(key);
    match &state.health {
        Health::Unknown => (Style::new().fg(theme::MUTED), "—".into()),
        Health::Checking => (Style::new().fg(theme::MUTED), "… 检查中".into()),
        Health::Down { message, .. } => {
            let short = if message.starts_with("HTTP ") {
                format!("● {message}")
            } else {
                "● 失败".into()
            };
            (Style::new().fg(theme::ERR), short)
        }
        Health::Live { .. } => (Style::new().fg(theme::OK), "● 可用".into()),
    }
}
