use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, List, ListItem, ListState, Padding, Paragraph, Row, Table,
    TableState, Wrap,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, Modal};
use crate::config::KeyEntry;
use crate::form::{Field, Form, LineEdit};
use crate::probe::Health;
use crate::recipe;

pub mod theme {
    use ratatui::style::Color;

    pub const ACCENT: Color = Color::Rgb(56, 189, 248);
    pub const OK: Color = Color::Rgb(52, 211, 153);
    pub const ERR: Color = Color::Rgb(248, 113, 113);
    pub const GOLD: Color = Color::Rgb(251, 191, 36);
    pub const MUTED: Color = Color::Rgb(148, 163, 184);
    pub const TEXT: Color = Color::Rgb(226, 232, 240);
    pub const HIGHLIGHT_BG: Color = Color::Rgb(30, 58, 95);
    pub const BORDER: Color = Color::Rgb(71, 85, 105);
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);

    match &app.modal {
        Modal::Form { form, .. } => draw_form(frame, form, area),
        Modal::ConfirmDeleteKey { key_id } => {
            draw_confirm(frame, "删除密钥", std::slice::from_ref(key_id), 1, area);
        }
        Modal::ConfirmDeleteProvider { provider_id, .. } => {
            draw_confirm(
                frame,
                "删除厂商",
                std::slice::from_ref(provider_id),
                1,
                area,
            );
        }
        Modal::None => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let refresh = match app.last_refresh {
        Some(at) => format!("{}ms 前刷新", at.elapsed().as_millis()),
        None => "尚未刷新".into(),
    };
    let recipe = app.current_recipe();
    let name = recipe.map(|r| r.name.as_str()).unwrap_or("—");
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(18)])
        .split(area);
    let left = Line::from(vec![
        Span::styled(
            " apim ",
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("本地 API 管理", Style::new().fg(theme::MUTED)),
        Span::raw("  ·  "),
        Span::styled(
            name,
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
    ]);
    let right = Line::from(Span::styled(
        format!(" {refresh} "),
        Style::new().fg(theme::MUTED),
    ));
    frame.render_widget(Paragraph::new(left), cols[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
        .split(area);

    draw_providers(frame, app, cols[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(9)])
        .split(cols[1]);

    draw_keys(frame, app, right[0]);
    draw_balance(frame, app, right[1]);
}

fn pane_block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let border = if focused {
        theme::ACCENT
    } else {
        theme::BORDER
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(title.into())
        .title_style(Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD))
        .padding(Padding::horizontal(1))
}

fn draw_providers(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Providers;
    let block = pane_block(format!(" 厂商 · {} ", app.provider_ids.len()), focused);

    let items: Vec<ListItem> = app
        .provider_ids
        .iter()
        .map(|id| {
            let recipe = app.recipes.get(id);
            let name = recipe.map(|r| r.name.as_str()).unwrap_or(id.as_str());
            let first = app.keys.iter().find(|k| k.provider == *id);
            let (money, status) = match first {
                Some(key) => provider_summary(app, key),
                None => ("—".into(), "无密钥".into()),
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    name.to_string(),
                    Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{money}  {status}"),
                    Style::new().fg(theme::MUTED),
                )),
            ])
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.selected_provider));

    let list = List::new(items)
        .block(block)
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::new()
                .bg(theme::HIGHLIGHT_BG)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut state);
}

fn provider_summary(app: &App, key: &KeyEntry) -> (String, String) {
    let state = app.state_for(key);
    let money = state
        .balance
        .as_ref()
        .and_then(|b| b.view.as_ref())
        .and_then(|v| v.items.first())
        .map(|item| {
            let amount = item.ctx.get("total_balance").cloned().unwrap_or_else(|| {
                item.fields
                    .first()
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            });
            recipe::money(item.currency.as_deref(), &amount)
        })
        .unwrap_or_else(|| "…".into());
    let status = match &state.health {
        Health::Live { ms, .. } => format!("{}ms", ms),
        Health::Down { message, .. } => message.clone(),
        Health::Checking => "检查中".into(),
        Health::Unknown => "未检查".into(),
    };
    (money, status)
}

fn draw_keys(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Keys;
    let recipe = app.current_recipe();
    let name = recipe.map(|r| r.name.as_str()).unwrap_or("—");
    let n = app.keys_in_provider().len();
    let headline = app
        .selected_key_entry()
        .and_then(|key| {
            app.state_for(key)
                .balance
                .and_then(|b| b.view)
                .and_then(|v| {
                    v.items.first().map(|item| {
                        recipe::money(
                            item.currency.as_deref(),
                            item.ctx.get("total_balance").unwrap_or(&v.headline),
                        )
                    })
                })
        })
        .unwrap_or_else(|| "—".into());

    let block = pane_block(format!(" {name}  ·  {n} 个密钥  ·  {headline} "), focused);
    let empty_inner = block.inner(area);

    let header = Row::new(["#", "别名", "分组", "密钥", "状态"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app
        .keys_in_provider()
        .into_iter()
        .enumerate()
        .map(|(i, idx)| {
            let key = &app.keys[idx];
            let (style, label) = status_label(app, key);
            Row::new([
                Cell::from(format!("{}", i + 1)),
                Cell::from(key.alias.clone()),
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
        let hint = Paragraph::new(" 该厂商还没有密钥，按 a 添加 ")
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
        Health::Live { .. } => {
            let available = state
                .balance
                .as_ref()
                .and_then(|b| b.view.as_ref())
                .and_then(|v| v.available);
            match available {
                Some(false) => (Style::new().fg(theme::GOLD), "● 无额度".into()),
                _ => (Style::new().fg(theme::OK), "● 可用".into()),
            }
        }
    }
}

fn draw_balance(frame: &mut Frame, app: &App, area: Rect) {
    let Some(key) = app.selected_key_entry() else {
        let block = pane_block(" 额度 ", false);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new("选择一条密钥").style(Style::new().fg(theme::MUTED)),
            inner,
        );
        return;
    };
    let block = pane_block(format!(" 额度 · {} ", key.alias), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let state = app.state_for(key);
    let mut lines: Vec<Line> = Vec::new();

    if let Health::Down { message, ms } = &state.health {
        lines.push(Line::from(Span::styled(
            format!("探活失败 {message}  ·  {ms}ms"),
            Style::new().fg(theme::ERR),
        )));
    }

    if app.is_checking(&key.id()) && state.balance.is_none() {
        lines.push(Line::from(Span::styled(
            "正在查询额度…",
            Style::new().fg(theme::MUTED),
        )));
    } else if let Some(balance) = &state.balance {
        if let Some(err) = &balance.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::new().fg(theme::ERR),
            )));
        } else if let Some(view) = &balance.view {
            let avail_line = match view.available {
                Some(true) => Span::styled("● 账号可用", Style::new().fg(theme::OK)),
                Some(false) => Span::styled("● 账号不可用 / 无余额", Style::new().fg(theme::GOLD)),
                None => Span::styled("● 已返回额度", Style::new().fg(theme::ACCENT)),
            };
            lines.push(Line::from(avail_line));
            if let Some(recipe) = app.current_recipe() {
                lines.push(Line::from(Span::styled(
                    format!("  {}  ·  分组 {}", recipe.base_url, key.group_label()),
                    Style::new().fg(theme::MUTED),
                )));
            }

            for item in &view.items {
                let amount = item
                    .ctx
                    .get("total_balance")
                    .cloned()
                    .unwrap_or_else(|| view.headline.clone());
                let money = recipe::money(item.currency.as_deref(), &amount);
                let currency = item.currency.clone().unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {money}  "),
                        Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(currency, Style::new().fg(theme::MUTED)),
                ]));
                if !item.fields.is_empty() {
                    let parts: Vec<String> = item
                        .fields
                        .iter()
                        .map(|(k, v)| format!("{k} {v}"))
                        .collect();
                    lines.push(Line::from(Span::styled(
                        format!("  {}", parts.join("    ")),
                        Style::new().fg(theme::TEXT),
                    )));
                }
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(balance.endpoint.clone(), Style::new().fg(theme::MUTED)),
            Span::raw("  ·  "),
            Span::styled(
                match balance.status {
                    Some(s) => format!("{s} · {}ms", balance.elapsed_ms),
                    None => format!("{}ms", balance.elapsed_ms),
                },
                Style::new().fg(theme::MUTED),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "该厂商还没有配置额度查询",
            Style::new().fg(theme::MUTED),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let toast = app.toast_text();
    let copy_hint = if app.focus == Focus::Providers {
        "c 复制BaseURL"
    } else {
        "c 复制密钥"
    };
    let keys = format!(" j/k 移动  Tab 切换  {copy_hint}  a 添加  e 编辑  d 删除  r 刷新  q 退出 ");
    let line = if let Some(toast) = toast {
        Line::from(vec![
            Span::styled(
                format!(" {toast}  "),
                Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
            ),
            Span::styled(keys, Style::new().fg(theme::MUTED)),
        ])
    } else {
        Line::from(Span::styled(keys, Style::new().fg(theme::MUTED)))
    };
    frame.render_widget(Paragraph::new(line), area);
}

// ---- modals -----------------------------------------------------------

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(area);
    let h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(v[1]);
    h[1]
}

fn draw_form(frame: &mut Frame, form: &Form, area: Rect) {
    let n = form.fields.len() as u16;
    let rect = centered(66, n + 5, area);
    frame.render_widget(Clear, rect);

    let block = pane_block(format!(" {} ", form.title), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints({
            let mut c: Vec<Constraint> = (0..n).map(|_| Constraint::Length(1)).collect();
            c.push(Constraint::Length(1));
            c.push(Constraint::Length(2));
            c
        })
        .split(inner);

    let label_cols = form
        .fields
        .iter()
        .map(|f| UnicodeWidthStr::width(f.label()))
        .max()
        .unwrap_or(4)
        + 2;

    let mut cursor: Option<(u16, u16)> = None;
    for (i, field) in form.fields.iter().enumerate() {
        let active = i == form.active;
        let (line, caret) = match field {
            Field::Text {
                label,
                edit,
                enabled,
            } => {
                let field_width = rows[i].width.saturating_sub(label_cols as u16 + 2);
                let line = Line::from(vec![
                    pad_span(label, label_cols),
                    Span::styled(
                        render_edit(edit, field_width),
                        if *enabled {
                            Style::new().fg(theme::TEXT)
                        } else {
                            Style::new().fg(theme::MUTED)
                        },
                    ),
                ]);
                let caret = if active && *enabled {
                    let (_, caret_from_left, _) = visible_tail(edit, field_width as usize);
                    Some(caret_from_left as u16)
                } else {
                    None
                };
                (line, caret)
            }
            Field::Select {
                label,
                options,
                selected,
                hint,
            } => {
                let value = options.get(*selected).map(String::as_str).unwrap_or("—");
                let affordance = if options.len() > 1 { "‹  ›" } else { "" };
                let line = Line::from(vec![
                    pad_span(label, label_cols),
                    Span::styled(format!("{affordance} "), Style::new().fg(theme::MUTED)),
                    Span::styled(
                        value.to_string(),
                        Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("   {hint}"), Style::new().fg(theme::MUTED)),
                ]);
                (line, None)
            }
        };
        let mut para = Paragraph::new(line);
        if active {
            para = para.style(Style::new().bg(theme::HIGHLIGHT_BG));
        }
        frame.render_widget(para, rows[i]);
        if let Some(caret) = caret {
            cursor = Some((rows[i].x + label_cols as u16 + 1 + caret, rows[i].y));
        }
    }

    let mut footer = vec![Span::styled(
        " Tab/↑↓ 下一项   ←/→ 切换/移动光标   Enter 保存   Esc 取消 ",
        Style::new().fg(theme::MUTED),
    )];
    if let Some(err) = &form.error {
        footer.push(Span::styled(
            format!(" ⚠ {err}"),
            Style::new().fg(theme::ERR).add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(footer)).wrap(Wrap { trim: false }),
        rows[n as usize + 1],
    );

    if let Some((x, y)) = cursor {
        frame.set_cursor_position((x, y));
    }
}

fn pad_span(label: &str, cols: usize) -> Span<'static> {
    let mut text = label.to_string();
    let width = UnicodeWidthStr::width(label);
    for _ in width..cols {
        text.push(' ');
    }
    Span::styled(text, Style::new().fg(theme::MUTED))
}

/// Value with an underscore cursor `_` drawn at the caret position.
fn render_edit(edit: &LineEdit, width: u16) -> String {
    let (text, cursor_from_left, _) = visible_tail(edit, width as usize);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i == cursor_from_left {
            out.push('_');
        }
        out.push(*c);
    }
    if cursor_from_left >= chars.len() {
        out.push('_');
    }
    out
}

/// When the value is wider than the field, show its tail so the caret stays
/// visible. Returns (visible text, caret offset in visible text, first
/// visible char index).
fn visible_tail(edit: &LineEdit, width: usize) -> (String, usize, usize) {
    let chars: Vec<char> = edit.value.chars().collect();
    let total = chars.len();
    let cursor = edit.cursor.min(total);
    if width == 0 {
        return (String::new(), 0, cursor);
    }
    if total <= width {
        return (edit.value.clone(), cursor, 0);
    }
    let start = total.saturating_sub(width).min(cursor);
    let end = (start + width).min(total);
    let text: String = chars[start..end].iter().collect();
    (text, cursor - start, start)
}

fn draw_confirm(frame: &mut Frame, title: &str, targets: &[String], _lines: usize, area: Rect) {
    let rect = centered(56, 6, area);
    frame.render_widget(Clear, rect);

    let block = pane_block(format!(" {title} "), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<Line> = Vec::new();
    for t in targets {
        lines.push(Line::from(Span::styled(
            format!("确定删除 {t} ？"),
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(Span::styled(
        "该操作会同时改写本地配置文件",
        Style::new().fg(theme::MUTED),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Enter 确认删除    Esc 取消",
        Style::new().fg(theme::MUTED),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}
