//! 表单弹窗：字段行 + 光标 + 提示/错误。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::{centered, pane_block, theme};
use crate::form::{Field, Form, LineEdit};

pub(crate) fn draw_form(frame: &mut Frame, form: &Form, area: Rect) {
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
        let (line, caret) = field_line(field, active, rows[i].width, label_cols);
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

fn field_line(
    field: &Field,
    active: bool,
    row_width: u16,
    label_cols: usize,
) -> (Line<'static>, Option<u16>) {
    match field {
        Field::Text {
            label,
            edit,
            enabled,
        } => {
            let field_width = row_width.saturating_sub(label_cols as u16 + 2);
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
/// 搜索弹窗（ui/search.rs）复用同一套光标渲染。
pub(crate) fn render_edit(edit: &LineEdit, width: u16) -> String {
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

/// 光标在可见文本里的列偏移（供 set_cursor_position 用）。
pub(crate) fn visible_caret(edit: &LineEdit, width: u16) -> u16 {
    let (_, caret, _) = visible_tail(edit, width as usize);
    caret as u16
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_tail_shows_tail_when_overflow() {
        let edit = LineEdit::new("0123456789"); // cursor at end
        let (text, caret, start) = visible_tail(&edit, 4);
        assert_eq!(text, "6789");
        assert_eq!(caret, 4);
        assert_eq!(start, 6);
    }

    #[test]
    fn visible_tail_full_when_fits() {
        let mut edit = LineEdit::new("abc");
        edit.left();
        let (text, caret, start) = visible_tail(&edit, 10);
        assert_eq!(text, "abc");
        assert_eq!(caret, 2);
        assert_eq!(start, 0);
    }
}
