//! 左栏：分页条（模型 / 非模型）+ 当前分页的厂商列表。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState};

use super::{App, Focus, pane_block, theme};
use crate::config::KeyEntry;
use crate::probe::Health;
use crate::recipe::ProviderKind;

pub(crate) fn draw_providers(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let focused = app.focus == Focus::Providers;
    let ids = app.provider_ids_filtered();
    let block = pane_block(tab_title(app), focused);

    let items: Vec<ListItem> = ids
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
    state.select(Some(app.selected_provider()));

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

/// 分页条（占左栏边框的标题位）：`[模型 · 5] │ 非模型 · 3`。
/// 方括号 + 高亮 = 当前分页；计数是该分页自己的（过滤生效时是「命中/总数」）。
///
/// 方括号不是装饰：颜色在快照里看不见，而分页条必须能一眼（一眼=一行文本）分辨。
fn tab_title(app: &App) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (i, kind) in ProviderKind::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::new().fg(theme::BORDER)));
        }
        let total = app.tabs[*kind as usize].provider_ids.len();
        let hit = app.provider_ids_in(*kind).len();
        let count = if hit == total {
            format!("{total}")
        } else {
            format!("{hit}/{total}")
        };
        let label = format!("{} · {count}", kind.label());
        if app.tab == *kind {
            spans.push(Span::styled(
                format!("[{label}]"),
                Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(label, Style::new().fg(theme::MUTED)));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

fn provider_summary(app: &App, key: &KeyEntry) -> (String, String) {
    let state = app.state_for(key);
    let money = state
        .balance
        .as_ref()
        .and_then(|b| b.lines.as_ref())
        .and_then(|lines| lines.first().cloned())
        .unwrap_or_else(|| "…".into());
    // 非模型厂商不探活，状态只能由额度脚本的成败来说（与密钥表同一口径）
    let status = match super::script_status(app, &key.provider, key) {
        Some((_, label)) => label.to_string(),
        None => match &state.health {
            Health::Live { ms, .. } => format!("{}ms", ms),
            Health::Down { message, .. } => message.clone(),
            Health::Checking => "检查中".into(),
            Health::Unknown => "未检查".into(),
        },
    };
    (money, status)
}

/// 分页条：当前页带方括号 + 高亮，非当前页暗色；两侧计数各自算。
///
/// 颜色在纯文本快照里看不见，所以这里直接断言 buffer 里的前景色——
/// 分页条靠「方括号 + 颜色」双保险区分，任何一半丢了都能被测到。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use unicode_width::UnicodeWidthStr;

    fn render(app: &App) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
        terminal
            .draw(|frame| draw_providers(frame, app, frame.area()))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    /// 首行（分页条占边框首行）读懂成人读的文本：宽字符占的两格只算一个。
    fn title_row(buf: &Buffer) -> String {
        let mut out = String::new();
        let mut skip = 0u16;
        for x in 0..buf.area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let sym = buf[(x, 0)].symbol();
            out.push_str(sym);
            let width = UnicodeWidthStr::width(sym) as u16;
            if width > 1 {
                skip = width - 1;
            }
        }
        out
    }

    fn cell_of(buf: &Buffer, sym: &str) -> Option<u16> {
        (0..buf.area.width).find(|x| buf[(*x, 0)].symbol() == sym)
    }

    fn first_non_space(buf: &Buffer, from: u16) -> u16 {
        (from..buf.area.width)
            .find(|x| buf[(*x, 0)].symbol() != " ")
            .unwrap()
    }

    #[test]
    fn tab_strip_marks_the_active_tab_by_brackets_and_color() {
        let (mut app, _rx, _rx_models) = test_app(&[("alpha", &["a1"])]);
        app.recipes.get_mut("alpha").unwrap().kind = ProviderKind::NonModel;
        app.rebuild_provider_list();

        // 分页条：第一格是「模型」标签，分隔符之后是「非模型」标签
        let buf = render(&app);
        let title = title_row(&buf);
        assert!(title.contains("[模型 · 0]"), "{title}");
        assert!(title.contains("非模型 · 1"), "{title}");
        let model_cell = first_non_space(&buf, 1);
        let non_model_cell = first_non_space(&buf, cell_of(&buf, "│").unwrap() + 1);
        // 当前页（模型）：方括号 + ACCENT；另一页 MUTED
        assert_eq!(buf[(model_cell, 0)].symbol(), "[");
        assert_eq!(buf[(model_cell, 0)].fg, theme::ACCENT);
        assert_eq!(buf[(non_model_cell, 0)].fg, theme::MUTED);

        // Tab 高亮换边
        app.switch_tab();
        let buf = render(&app);
        let title = title_row(&buf);
        assert!(title.contains("[非模型 · 1]"), "{title}");
        assert!(title.contains("模型 · 0"), "{title}");
        let model_cell = first_non_space(&buf, 1);
        let non_model_cell = first_non_space(&buf, cell_of(&buf, "│").unwrap() + 1);
        assert_eq!(buf[(model_cell, 0)].fg, theme::MUTED);
        assert_eq!(buf[(non_model_cell, 0)].symbol(), "[");
        assert_eq!(buf[(non_model_cell, 0)].fg, theme::ACCENT);
    }

    /// 非模型厂商没有探活，左栏摘要的状态也只能由额度脚本说来（与密钥表同口径）。
    #[test]
    fn status_uses_the_script_for_non_model_providers() {
        let (mut app, _rx, _rx_models) = test_app(&[("deepl", &["main"])]);
        app.recipes.get_mut("deepl").unwrap().kind = ProviderKind::NonModel;
        let key = app.keys[0].clone();
        // 无脚本结果时是「—」，脚本跑通是「● 可用」——与密钥表同一口径
        assert_eq!(provider_summary(&app, &key).1, "—");
        app.states.entry(key.id()).or_default().balance = Some(crate::probe::BalanceSnapshot {
            lines: Some(vec!["剩余 42 万字符".into()]),
            endpoint: "script deepl".into(),
            status: None,
            elapsed_ms: 1,
            error: None,
        });
        let (money, status) = provider_summary(&app, &key);
        assert_eq!(money, "剩余 42 万字符");
        assert_eq!(status, "● 可用");
    }
}
