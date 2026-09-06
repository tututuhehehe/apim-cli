//! 顶栏（标题 + 刷新时间）与底栏（快捷键 + toast）。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{App, Focus, theme};

pub(crate) fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    // 读数在任意一条探测结果到达时更新（全局视角），按「上次探活」表述。
    let refresh = match app.last_refresh {
        Some(at) => format!("上次探活 {}ms 前", at.elapsed().as_millis()),
        None => "尚未探活".into(),
    };
    let recipe = app.current_recipe();
    let name = recipe.map(|r| r.name.as_str()).unwrap_or("—");
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(18)])
        .split(area);
    let left = Line::from(vec![
        Span::styled(
            " apim ",
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("本地 API 管理", Style::new().fg(theme::MUTED)),
        Span::raw("  ·  "),
        Span::styled(
            name,
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
    ]);
    let right = Line::from(Span::styled(
        format!(" {refresh} "),
        Style::new().fg(theme::MUTED),
    ));
    frame.render_widget(Paragraph::new(left), cols[0]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}

pub(crate) fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let toast = app.toast_text();
    let keys = if app.focus == Focus::Providers {
        " j/k 移动  Tab 切换  ⏎ 打开主页  c 复制BaseURL  i 详情  a 添加  e 编辑  d 删除  r 刷新  q 退出 "
    } else {
        " j/k 移动  Tab 切换  m 模型  c 复制密钥  i 详情  a 添加  e 编辑  d 删除  r 刷新  q 退出 "
    };
    let mut spans = Vec::new();
    if let Some(toast) = toast {
        spans.push(Span::styled(
            format!(" {toast}  "),
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(hint) = filter_hint(app) {
        spans.push(Span::styled(
            format!(" {hint}  "),
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(keys, Style::new().fg(theme::MUTED)));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// 过滤生效时底栏的「筛选: xxx (命中/总数)  Esc 清除」提示。
fn filter_hint(app: &App) -> Option<String> {
    let key = app.key_filter.as_deref().map(|f| {
        (
            f,
            app.keys_in_provider_filtered().len(),
            app.keys_in_provider().len(),
        )
    });
    let provider = app
        .provider_filter
        .as_deref()
        .map(|f| (f, app.provider_ids_filtered().len(), app.provider_ids.len()));
    let both = key.is_some() && provider.is_some();
    let mut parts: Vec<String> = Vec::new();
    if let Some((f, m, n)) = provider {
        parts.push(filter_part("厂商", both, f, m, n));
    }
    if let Some((f, m, n)) = key {
        parts.push(filter_part("密钥", both, f, m, n));
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("{}  Esc 清除", parts.join("  ")))
}

fn filter_part(side: &str, both: bool, f: &str, m: usize, n: usize) -> String {
    if both {
        format!("筛选 {side}: {f} ({m}/{n})")
    } else {
        format!("筛选: {f} ({m}/{n})")
    }
}
