//! 表单弹窗：字段行 + 光标 + 提示/错误。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{centered, pane_block, theme};
use crate::form::{Field, Form, LineEdit};

pub(crate) fn draw_form(frame: &mut Frame, form: &Form, area: Rect) {
    // 不显示的行（勾选框收起的）不占位置、也不占高度
    let visible: Vec<usize> = (0..form.fields.len())
        .filter(|i| !form.fields[*i].is_hidden())
        .collect();
    let n = visible.len() as u16;
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

    let label_cols = visible
        .iter()
        .map(|i| UnicodeWidthStr::width(form.fields[*i].label()))
        .max()
        .unwrap_or(4)
        + 2;

    let mut cursor: Option<(u16, u16)> = None;
    for (row, index) in visible.iter().enumerate() {
        let field = &form.fields[*index];
        let active = *index == form.active;
        let (line, caret) = field_line(field, active, rows[row].width, label_cols);
        let mut para = Paragraph::new(line);
        if active {
            para = para.style(Style::new().bg(theme::HIGHLIGHT_BG));
        }
        frame.render_widget(para, rows[row]);
        if let Some(caret) = caret {
            cursor = Some((rows[row].x + label_cols as u16 + 1 + caret, rows[row].y));
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
            ..
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
                Some(visible_caret(edit, field_width))
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
        Field::Toggle {
            label,
            value,
            enabled,
            hint,
            ..
        } => {
            let mark = if *value { "[x]" } else { "[ ]" };
            let mark_style = if *enabled {
                Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::MUTED)
            };
            let line = Line::from(vec![
                pad_span(label, label_cols),
                Span::styled(format!("{mark} "), mark_style),
                Span::styled(hint.to_string(), Style::new().fg(theme::MUTED)),
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
/// CJK 等宽字符按显示宽度折算，光标列与终端实际列对齐。
pub(crate) fn visible_caret(edit: &LineEdit, width: u16) -> u16 {
    let (text, caret, _) = visible_tail(edit, width as usize);
    let before_caret: String = text.chars().take(caret).collect();
    before_caret.width() as u16
}

/// When the value is wider than the field, show its tail so the caret stays
/// visible. Returns (visible text, caret offset in visible text, first
/// visible char index). 窗口大小按显示宽度（CJK 记 2 列）计，光标列不漂移。
fn visible_tail(edit: &LineEdit, width: usize) -> (String, usize, usize) {
    let chars: Vec<char> = edit.value.chars().collect();
    let total = chars.len();
    let cursor = edit.cursor.min(total);
    if width == 0 {
        return (String::new(), 0, cursor);
    }
    let char_w = |c: char| c.width().unwrap_or(0);
    let total_w: usize = chars.iter().map(|&c| char_w(c)).sum();
    if total_w <= width {
        return (edit.value.clone(), cursor, 0);
    }
    // 溢出：从尾部往前收一个放得下的窗口，起点不越过光标（保证光标可见）
    let mut start = total;
    let mut acc = 0;
    while start > 0 && acc + char_w(chars[start - 1]) <= width {
        acc += char_w(chars[start - 1]);
        start -= 1;
    }
    if start > cursor {
        // 尾部窗口装不下光标之前的内容：改为从光标处向右展示
        start = cursor;
    }
    let mut end = start;
    let mut acc = 0;
    while end < total && acc + char_w(chars[end]) <= width {
        acc += char_w(chars[end]);
        end += 1;
    }
    (chars[start..end].iter().collect(), cursor - start, start)
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

    // ---- 宽字符（CJK 记 2 列）：光标列按显示宽度，不按字符数 --------------

    #[test]
    fn visible_tail_wide_chars_fit_by_display_width() {
        let edit = LineEdit::new("你好世界"); // 4 字符、显示宽 8
        // 显示宽 8 > 限宽 5：窗口只装得下「世界」（宽 4），而不是末 5 个字符
        let (text, caret, start) = visible_tail(&edit, 5);
        assert_eq!(text, "世界");
        assert_eq!(caret, 2, "caret 仍是可见文本内的字符下标");
        assert_eq!(start, 2);
        // 光标列 = 前缀显示宽 4，若按字符数会错报 2（终端光标漂移两格）
        assert_eq!(visible_caret(&edit, 5), 4);
        // 全部放得下时按显示宽计
        assert_eq!(visible_caret(&edit, 8), 8);
    }

    #[test]
    fn visible_tail_keeps_caret_visible_in_wide_text() {
        // 光标停在「世」前（下标 2）：尾部窗口起点不越过光标
        let mut edit = LineEdit::new("你好世界");
        edit.left();
        edit.left();
        assert_eq!(edit.cursor, 2);
        let (text, caret, start) = visible_tail(&edit, 5);
        assert_eq!((text, caret, start), ("世界".to_string(), 0, 2));
        assert_eq!(visible_caret(&edit, 5), 0);
        // 光标更靠前（「好」前）时尾部窗口装不下，改为从光标处向右展示
        let mut early = LineEdit::new("你好世界");
        early.left();
        early.left();
        early.left();
        let (text, caret, start) = visible_tail(&early, 5);
        assert_eq!((text, caret, start), ("好世".to_string(), 0, 1));
        assert_eq!(visible_caret(&early, 5), 0);
    }

    #[test]
    fn visible_caret_counts_mixed_width_text() {
        let mut edit = LineEdit::new("a你b");
        edit.left(); // 光标落在 你 和 b 之间
        assert_eq!(edit.cursor, 2);
        // 显示宽 1+2+1=4 > 限宽 3：窗口「你b」，光标在「你」后 → 列 2
        assert_eq!(visible_caret(&edit, 3), 2);
        // render_edit 的下划线光标插在字符下标处，显示列与 visible_caret 一致
        assert_eq!(render_edit(&edit, 3), "你_b");
    }
}
