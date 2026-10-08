//! 右侧密钥表 + 状态标签。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table, TableState};

use super::{App, Focus, pane_block, theme};
use crate::config::KeyEntry;
use crate::probe::Health;
pub(crate) fn draw_keys(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let focused = app.focus == Focus::Keys;
    let recipe = app.current_recipe();
    let name = recipe.map(|r| r.name.as_str()).unwrap_or("—");
    let rows_idx = app.keys_in_provider_filtered();
    let n = rows_idx.len();
    let total = app.keys_in_provider().len();
    let count = if n == total {
        format!("{n} 个密钥")
    } else {
        format!("{n}/{total} 个密钥")
    };
    let headline = app
        .selected_key_entry()
        .and_then(|key| {
            app.state_for(key)
                .balance
                .and_then(|b| b.lines)
                .and_then(|lines| lines.first().cloned())
        })
        .unwrap_or_else(|| "—".into());

    let block = pane_block(format!(" {name}  ·  {count}  ·  {headline} "), focused);
    let empty_inner = block.inner(area);

    let header = Row::new(["#", "别名", "分组", "密钥", "状态"])
        .style(Style::new().fg(theme::MUTED).add_modifier(Modifier::BOLD));

    let mut rows: Vec<Row> = rows_idx
        .into_iter()
        .enumerate()
        .map(|(i, idx)| {
            let key = &app.keys[idx];
            let (style, label) = status_label(app, key);
            let alias = match import_badge(app, &key.id()) {
                Some(badge) => Cell::from(Span::styled(
                    format!("{badge} {}", key.alias),
                    Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
                )),
                None => Cell::from(key.alias.clone()),
            };
            Row::new([
                Cell::from(format!("{}", i + 1)),
                alias,
                Cell::from(key.group_label().to_string()),
                Cell::from(key.masked_token()),
                Cell::from(Span::styled(label, style)),
            ])
            .style(Style::new().fg(theme::TEXT))
        })
        .collect();
    if let Some((style, label)) = super::oauth_row_status(app) {
        rows.push(
            Row::new([
                Cell::from("—"),
                Cell::from(Span::styled(
                    "AUTH",
                    Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
                )),
                Cell::from("OAuth"),
                Cell::from("Codex · apim 管理"),
                Cell::from(Span::styled(label, style)),
            ])
            .style(Style::new().fg(theme::TEXT)),
        );
    }

    let mut state = TableState::default();
    // AUTH 行也是一个可选中的行（openai 分页）：没有密钥时它就是唯一那一行
    if n > 0 || super::oauth_row_status(app).is_some() {
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
        let hint = if app.key_filter.is_some() {
            " 没有匹配的密钥，Esc 清除过滤 "
        } else if app.provider_filter().is_some() {
            // 厂商全被滤掉时没有选中厂商，别误导用户去按 a
            " 没有匹配的厂商，Esc 清除过滤 "
        } else {
            " 该厂商还没有密钥，按 a 添加 "
        };
        let hint = Paragraph::new(hint)
            .style(Style::new().fg(theme::MUTED))
            .alignment(Alignment::Center);
        let inner = Rect {
            y: area.y + area.height / 2,
            ..empty_inner
        };
        frame.render_widget(hint, inner);
    }
}

/// 密钥行别名前面的 ★ 角标：`★C`（被 Codex 用）/ `★C,P`（多个客户端都用）。
/// 角标字母来自 `Agent::badge()`；没被任何客户端用 → None（就是普通别名）。
fn import_badge(app: &App, key_id: &str) -> Option<String> {
    let using = app.agents_using(key_id);
    if using.is_empty() {
        return None;
    }
    let badges: Vec<&str> = using.iter().map(|agent| agent.badge()).collect();
    Some(format!("★{}", badges.join(",")))
}

fn status_label(app: &App, key: &KeyEntry) -> (Style, String) {
    if app.is_checking(&key.id()) {
        return (Style::new().fg(theme::MUTED), "… 检查中".into());
    }
    // 非模型厂商不探活：看额度脚本（口径见 ui::script_status）
    if let Some((style, label)) = super::script_status(app, &key.provider, key) {
        return (style, label.to_string());
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
        Health::Live { .. } => (Style::new().fg(theme::OK), "● 可用".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use crate::clients::Agent;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn openai_has_a_fixed_non_key_auth_row() {
        let (mut app, _rx, _rx_task) = test_app(&[("openai", &["api-key"])]);
        app.focus_provider("openai");
        app.focus = Focus::Keys;
        let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
        terminal
            .draw(|frame| draw_keys(frame, &app, frame.area()))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("AUTH"));
        assert!(rendered.contains("Codex"));
        assert_eq!(
            app.selected_key_entry().unwrap().alias,
            "api-key",
            "AUTH 不是密钥（selected_key_entry 为 None），但它是可选中的单独一行"
        );
    }

    /// AUTH 行是可选中的：光标走上去时高亮在 AUTH 行（`x` 在那里 = 导入 Codex 官方路），
    /// 而不是永远停在最后一把密钥上。
    #[test]
    fn auth_row_can_be_selected_and_highlighted() {
        let (mut app, _rx, _rx_task) = test_app(&[("openai", &["api-key"])]);
        app.focus_provider("openai");
        app.focus = Focus::Keys;

        let render = |app: &App| {
            let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
            terminal
                .draw(|frame| draw_keys(frame, app, frame.area()))
                .unwrap();
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
        };

        app.selected_key = 0;
        assert!(
            !render(&app).contains("▶ —   AUTH"),
            "光标在密钥行时 AUTH 行不该带高亮"
        );

        app.selected_key = 1;
        assert!(app.auth_row_selected());
        assert!(app.selected_key_entry().is_none());
        assert!(
            render(&app).contains("▶ —   AUTH"),
            "选中 AUTH 行时高亮应落在它上面"
        );
    }

    /// ★ 角标只反映「这个客户端现在真的在用这把密钥」。
    #[test]
    fn badge_shows_the_clients_that_actually_use_the_key() {
        let (mut app, _rx, _rx_task) = test_app(&[("alpha", &["a1", "a2"])]);
        assert_eq!(
            import_badge(&app, "alpha.a1"),
            None,
            "没被任何客户端用就没有角标"
        );

        app.active_keys
            .insert(Agent::Codex, vec!["alpha.a1".into()]);
        assert_eq!(import_badge(&app, "alpha.a1").as_deref(), Some("★C"));
        assert_eq!(
            import_badge(&app, "alpha.a2"),
            None,
            "别的密钥不该被贴上角标"
        );

        // 两个客户端都用同一把：并排成 ★C,P（顺序同 Agent::ALL）
        app.active_keys.insert(Agent::Pi, vec!["alpha.a1".into()]);
        assert_eq!(import_badge(&app, "alpha.a1").as_deref(), Some("★C,P"));
    }
}
