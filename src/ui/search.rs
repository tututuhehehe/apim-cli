//! `/` 搜索弹窗：单行输入 + 实时过滤提示，背后的列表边输边变。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Clear, Paragraph};

use super::form_modal::{render_edit, visible_caret};
use super::{centered, pane_block, theme};
use crate::app::SearchTarget;
use crate::form::LineEdit;

pub(crate) fn draw_search(frame: &mut Frame, target: SearchTarget, edit: &LineEdit, area: Rect) {
    let rect = centered(56, 4, area);
    frame.render_widget(Clear, rect);

    let title = match target {
        SearchTarget::Key => " 搜索密钥 ",
        SearchTarget::Provider => " 搜索厂商 ",
    };
    let block = pane_block(title, true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let input = Paragraph::new(render_edit(edit, inner.width)).style(Style::new().fg(theme::TEXT));
    frame.render_widget(input, inner);

    let hint = Rect {
        y: inner.y + 1,
        ..inner
    };
    frame.render_widget(
        Paragraph::new("输入关键字实时过滤，Enter 应用 / Esc 取消")
            .style(Style::new().fg(theme::MUTED)),
        hint,
    );

    frame.set_cursor_position((inner.x + visible_caret(edit, inner.width), inner.y));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn search_modal_renders_input_cursor_and_hint() {
        let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
        let edit = LineEdit::new("glm");
        terminal
            .draw(|frame| draw_search(frame, SearchTarget::Key, &edit, frame.area()))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        // CJK 双宽字形在 buffer 里会夹空格（搜 索 密 钥），断言前压掉
        let text: String = text.chars().filter(|c| *c != ' ').collect();
        assert!(text.contains("搜索密钥"), "应有目标标题: {text}");
        assert!(text.contains("glm_"), "输入值应带光标渲染: {text}");
        assert!(text.contains("实时过滤"), "应有操作提示: {text}");
    }
}
