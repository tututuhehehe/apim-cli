//! 绘制入口与公共样式。各面板/弹窗在子模块。

mod balance;
mod confirm;
mod form_modal;
mod header;
mod import;
mod inspector;
mod keys;
mod models;
mod providers;
mod search;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Padding};

use crate::app::{App, Focus, Modal};

pub mod theme {
    use ratatui::style::Color;

    pub const ACCENT: Color = Color::Rgb(56, 189, 248);
    pub const OK: Color = Color::Rgb(52, 211, 153);
    pub const ERR: Color = Color::Rgb(248, 113, 113);
    pub const GOLD: Color = Color::Rgb(251, 191, 36);
    pub const MUTED: Color = Color::Rgb(148, 163, 184);
    pub const TEXT: Color = Color::Rgb(226, 232, 240);
    pub const HIGHLIGHT_BG: Color = Color::Rgb(30, 58, 95);
    pub const BORDER: Color = Color::Rgb(71, 85, 105);
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    header::draw_header(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    header::draw_footer(frame, app, chunks[2]);

    match &app.modal {
        Modal::Form { form, .. } => form_modal::draw_form(frame, form, area),
        Modal::ConfirmDeleteKey { key_id } => {
            confirm::draw_confirm(frame, "删除密钥", std::slice::from_ref(key_id), area);
        }
        Modal::ConfirmDeleteProvider { provider_id, .. } => {
            confirm::draw_confirm(frame, "删除厂商", std::slice::from_ref(provider_id), area);
        }
        Modal::Inspector { .. } => inspector::draw_inspector(frame, app, area),
        Modal::Models {
            key_id,
            status,
            filter,
            searching,
        } => {
            // 弹窗标题显示该 key 所属厂商名
            let name = app
                .keys
                .iter()
                .find(|k| &k.id() == key_id)
                .and_then(|k| app.recipes.get(&k.provider))
                .map(|r| r.name.as_str())
                .unwrap_or("模型");
            models::draw_models(frame, name, status, filter, *searching, area);
        }
        Modal::Search { target, edit, .. } => search::draw_search(frame, *target, edit, area),
        Modal::Import(flow) => import::draw_import(frame, flow, area),
        Modal::None => {}
    }
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
        .split(area);

    providers::draw_providers(frame, app, cols[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(9)])
        .split(cols[1]);

    keys::draw_keys(frame, app, right[0]);
    balance::draw_balance(frame, app, right[1]);
}

pub(crate) fn pane_block<'a>(
    title: impl Into<ratatui::text::Line<'a>>,
    focused: bool,
) -> Block<'a> {
    let border = if focused {
        theme::ACCENT
    } else {
        theme::BORDER
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(title.into())
        .title_style(Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD))
        .padding(Padding::horizontal(1))
}

/// 搜索框那一行（模型列表弹窗与一键导入面板共用，样式别各写一份）。
pub(crate) fn draw_search_box(frame: &mut Frame, area: Rect, filter: &str, searching: bool) {
    use ratatui::text::{Line, Span};
    let mut spans = vec![Span::styled(" 搜索: ", Style::new().fg(theme::MUTED))];
    if filter.is_empty() && !searching {
        spans.push(Span::styled(
            "/ 输入关键字过滤",
            Style::new().fg(theme::MUTED),
        ));
    } else {
        spans.push(Span::styled(
            filter.to_string(),
            Style::new().fg(theme::TEXT),
        ));
        if searching {
            spans.push(Span::styled("_", Style::new().fg(theme::ACCENT)));
        }
    }
    frame.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), area);
}

/// 滚动偏移：选中行跟随窗口移动，保证 selected 落在 `[offset, offset+visible)` 内。
pub(crate) fn scroll_offset(selected: usize, visible: usize) -> usize {
    selected.saturating_sub(visible.saturating_sub(1))
}

pub(crate) fn centered(width: u16, height: u16, area: Rect) -> Rect {
    // 宽高都钳到区域内，矮终端下弹窗贴顶显示而非溢出裁切
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area);
    let h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(v[1]);
    h[1]
}
