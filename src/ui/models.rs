//! 模型列表弹窗：Loading / Error / Done 三态；Done 列出模型名，j/k 滚动跟随。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};

use super::{centered, pane_block, theme};
use crate::app::ModelsStatus;

/// 弹窗列表区最多显示的模型行数，超出靠滚动。
const VISIBLE_ROWS: usize = 15;

pub(crate) fn draw_models(
    frame: &mut Frame,
    provider_name: &str,
    status: &ModelsStatus,
    area: Rect,
) {
    // 列表内容行数（不含提示行）；弹窗高 = 内容 + 提示行 1 + 上下边框 2
    let inner_rows = match status {
        ModelsStatus::Loading => 2,
        ModelsStatus::Error { .. } => 6,
        ModelsStatus::Done { items, .. } => items.len().clamp(1, VISIBLE_ROWS) as u16,
    };
    let rect = centered(56, inner_rows + 3, area);
    frame.render_widget(Clear, rect);

    let block = pane_block(format!(" 模型 · {provider_name} "), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    match status {
        ModelsStatus::Loading => {
            let lines = vec![Line::from(Span::styled(
                "正在获取模型…",
                Style::new().fg(theme::MUTED),
            ))];
            frame.render_widget(Paragraph::new(lines), rows[0]);
        }
        ModelsStatus::Error { message } => {
            let lines = vec![Line::from(Span::styled(
                message.clone(),
                Style::new().fg(theme::ERR),
            ))];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
        }
        ModelsStatus::Done { items, selected } => {
            if items.is_empty() {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "（没有返回任何模型）",
                        Style::new().fg(theme::MUTED),
                    ))),
                    rows[0],
                );
            } else {
                let visible = items.len().min(VISIBLE_ROWS);
                let offset = scroll_offset(*selected, visible);
                let window: Vec<ListItem> = items
                    .iter()
                    .skip(offset)
                    .take(visible)
                    .map(|name| ListItem::new(name.clone()))
                    .collect();
                let mut state = ListState::default();
                state.select(Some(selected.saturating_sub(offset)));
                // 选中样式与厂商列表一致
                let list = List::new(window).highlight_symbol("▶ ").highlight_style(
                    Style::new()
                        .bg(theme::HIGHLIGHT_BG)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
                frame.render_stateful_widget(list, rows[0], &mut state);
            }
        }
    }

    // 复制提示只在有列表可复制时显示（Loading/Error/空列表时 c 无操作）
    if matches!(status, ModelsStatus::Done { items, .. } if !items.is_empty()) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " j/k 移动  c 复制  Esc 关闭 ",
                Style::new().fg(theme::MUTED),
            ))),
            rows[1],
        );
    }
}

/// 滚动偏移：选中行跟随窗口移动，保证 selected 落在 [offset, offset+visible) 内。
fn scroll_offset(selected: usize, visible: usize) -> usize {
    selected.saturating_sub(visible.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn scroll_offset_keeps_selection_inside_window() {
        assert_eq!(scroll_offset(0, 15), 0);
        assert_eq!(scroll_offset(14, 15), 0); // 前 15 项，窗口不动
        assert_eq!(scroll_offset(15, 15), 1); // 第 16 项：窗口下移一行
        assert_eq!(scroll_offset(29, 15), 15);
    }

    #[test]
    fn scroll_offset_handles_tiny_lists_and_windows() {
        assert_eq!(scroll_offset(2, 15), 0); // 列表不足一屏
        assert_eq!(scroll_offset(5, 1), 5); // 单行窗口跟着选中走
        assert_eq!(scroll_offset(0, 0), 0); // 空列表不 panic
    }

    /// 渲染到 TestBackend，拼出整屏纯文本（与 tui.rs 快照同思路，跳过宽字符占位格）。
    fn render_text(status: &ModelsStatus) -> String {
        let backend = ratatui::backend::TestBackend::new(70, 30);
        let mut terminal = ratatui::Terminal::new(backend).expect("测试终端");
        terminal
            .draw(|frame| draw_models(frame, "假厂商", status, frame.area()))
            .expect("渲染");
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            let mut skip = 0u16;
            for x in 0..buf.area.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let sym = buf[(x, y)].symbol();
                out.push_str(sym);
                let width = UnicodeWidthStr::width(sym) as u16;
                if width > 1 {
                    skip = width - 1;
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn done_modal_renders_scrolled_window_and_hint() {
        let items: Vec<String> = (0..20).map(|i| format!("model-{i:02}")).collect();
        let status = ModelsStatus::Done {
            items,
            selected: 17, // 超出一屏，列表应跟着滚
        };
        let text = render_text(&status);
        assert!(text.contains("模型 · 假厂商"), "标题: {text}");
        assert!(text.contains("model-17"), "选中项应可见: {text}");
        assert!(!text.contains("model-00"), "滚出窗口的项不应出现");
        assert!(text.contains("j/k 移动"), "底部提示应出现: {text}");
    }

    #[test]
    fn loading_and_error_modal_render_state_text() {
        let loading = render_text(&ModelsStatus::Loading);
        assert!(loading.contains("正在获取模型…"), "Loading 文案: {loading}");
        let error = render_text(&ModelsStatus::Error {
            message: "HTTP 401".into(),
        });
        assert!(error.contains("HTTP 401"), "错误信息: {error}");
    }
}
