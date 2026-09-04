//! 右下额度面板：选中密钥的余额明细。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{App, pane_block, theme};
use crate::probe::Health;
use crate::recipe;

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
        } else if let Some(view) = &balance.view {
            lines.push(availability_line(view.available));
            if let Some(recipe) = app.current_recipe() {
                lines.push(Line::from(Span::styled(
                    format!("  {}  ·  分组 {}", recipe.base_url, key.group_label()),
                    Style::new().fg(theme::MUTED),
                )));
            }
            for item in &view.items {
                lines.extend(item_lines(item, view));
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

fn availability_line(available: Option<bool>) -> Line<'static> {
    let span = match available {
        Some(true) => Span::styled("● 账号可用", Style::new().fg(theme::OK)),
        Some(false) => Span::styled("● 账号不可用 / 无余额", Style::new().fg(theme::GOLD)),
        None => Span::styled("● 已返回额度", Style::new().fg(theme::ACCENT)),
    };
    Line::from(span)
}

fn item_lines(item: &recipe::BalanceItem, view: &recipe::BalanceView) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
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
    lines
}
