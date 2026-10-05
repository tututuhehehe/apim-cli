//! 右下额度面板：OpenAI Codex OAuth + 当前 API Key 的额度。
use super::{App, pane_block, theme};
use crate::probe::Health;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

pub(crate) fn draw_balance(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let key = app.selected_key_entry();
    let alias = key.map(|k| k.alias.as_str()).unwrap_or("—");
    let is_openai = app.current_provider_id() == Some("openai");
    let block = pane_block(
        if is_openai {
            format!(" 额度 · {alias} + AUTH ")
        } else {
            format!(" 额度 · {alias} ")
        },
        false,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines: Vec<Line> = Vec::new();

    if is_openai {
        lines.push(Line::from(Span::styled(
            "AUTH · OpenAI Codex",
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        )));
        if app.oauth_checking && app.oauth_balance.is_none() {
            lines.push(Line::from(Span::styled(
                "  正在查询 OAuth 额度…",
                Style::new().fg(theme::MUTED),
            )));
        } else {
            match &app.oauth_balance {
                None => lines.push(Line::from(Span::styled(
                    "  尚未查询",
                    Style::new().fg(theme::MUTED),
                ))),
                Some(Ok(values)) if values.is_empty() => lines.push(Line::from(Span::styled(
                    "  未配置 · 运行 apim auth openai login",
                    Style::new().fg(theme::MUTED),
                ))),
                Some(Ok(values)) => {
                    for value in values {
                        lines.push(Line::from(Span::styled(
                            format!("  {value}"),
                            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
                        )));
                    }
                }
                Some(Err(error)) => lines.push(Line::from(Span::styled(
                    format!("  {error}"),
                    Style::new().fg(theme::ERR),
                ))),
            }
        }
        lines.push(Line::from(""));
    }

    let Some(key) = key else {
        if !is_openai {
            lines.push(Line::from(Span::styled(
                "选择一条密钥",
                Style::new().fg(theme::MUTED),
            )));
        }
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    };
    let state = app.state_for(key);
    if let Health::Down { message, ms } = &state.health {
        lines.push(Line::from(Span::styled(
            format!("探活失败 {message} · {ms}ms"),
            Style::new().fg(theme::ERR),
        )));
    }
    if app.is_checking(&key.id()) && state.balance.is_none() {
        lines.push(Line::from(Span::styled(
            "正在查询 API 额度…",
            Style::new().fg(theme::MUTED),
        )));
    } else if let Some(balance) = &state.balance {
        lines.push(Line::from(Span::styled(
            "API · 当前密钥",
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        )));
        if let Some(err) = &balance.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::new().fg(theme::ERR),
            )));
        } else if let Some(output) = &balance.lines {
            if let Some(recipe) = app.current_recipe() {
                lines.push(Line::from(Span::styled(
                    format!("  {} · 分组 {}", recipe.base_url, key.group_label()),
                    Style::new().fg(theme::MUTED),
                )));
            }
            for (i, line) in output.iter().enumerate() {
                let style = if i == 0 {
                    Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(theme::TEXT)
                };
                lines.push(Line::from(Span::styled(format!("  {line}"), style)));
            }
        }
        lines.push(Line::from(Span::styled(
            format!(
                "{} · {}{}ms",
                balance.endpoint,
                balance
                    .status
                    .map(|s| format!("HTTP {s} · "))
                    .unwrap_or_default(),
                balance.elapsed_ms
            ),
            Style::new().fg(theme::MUTED),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "该厂商还没有绑定额度脚本",
            Style::new().fg(theme::MUTED),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
