//! 删除确认弹窗。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, pane_block, theme};

pub(crate) fn draw_confirm(frame: &mut Frame, title: &str, targets: &[String], area: Rect) {
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
