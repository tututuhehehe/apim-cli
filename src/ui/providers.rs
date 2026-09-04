//! 左栏：厂商列表。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState};

use super::{App, Focus, pane_block, theme};
use crate::config::KeyEntry;
use crate::probe::Health;
use crate::recipe;

pub(crate) fn draw_providers(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
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
