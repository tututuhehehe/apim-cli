//! 右下额度面板：选中密钥的余额明细（脚本 stdout 逐行直显）。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{App, pane_block, theme};
use crate::probe::Health;

pub(crate) fn draw_balance(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
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
        } else if let Some(output) = &balance.lines {
            if let Some(recipe) = app.current_recipe() {
                lines.push(Line::from(Span::styled(
                    format!("  {}  ·  分组 {}", recipe.base_url, key.group_label()),
                    Style::new().fg(theme::MUTED),
                )));
            }
            // 脚本 stdout 逐行直显，首行当 headline 高亮。
            for (i, line) in output.iter().enumerate() {
                let style = if i == 0 {
                    Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(theme::TEXT)
                };
                lines.push(Line::from(Span::styled(format!("  {line}  "), style)));
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
            "该厂商还没有绑定额度脚本",
            Style::new().fg(theme::MUTED),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}
