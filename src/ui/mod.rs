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

    let balance_height = if app.current_provider_id() == Some("openai") {
        14
    } else {
        9
    };
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(balance_height)])
        .split(cols[1]);

    keys::draw_keys(frame, app, right[0]);
    balance::draw_balance(frame, app, right[1]);
}

/// 非模型厂商的状态口径：它没有 HTTP 探活，只能用额度脚本的成败说话
/// （脚本跑通 = vendor 的 API 拿这把 key 真答上了）。
/// `None` = 模型厂商，调用方照旧看 Health；密钥表与左栏厂商摘要共用这一处。
pub(crate) fn script_status(
    app: &App,
    provider: &str,
    key: &crate::config::KeyEntry,
) -> Option<(Style, &'static str)> {
    if app.recipes.get(provider).is_none_or(|r| r.is_model()) {
        return None;
    }
    Some(match &app.state_for(key).balance {
        Some(balance) if balance.error.is_none() => (Style::new().fg(theme::OK), "● 可用"),
        Some(_) => (Style::new().fg(theme::ERR), "● 失败"),
        None => (Style::new().fg(theme::MUTED), "—"),
    })
}

/// AUTH（OpenAI Codex OAuth）的当前状态。
///
/// 密钥表的状态列与右下额度面板共用这一处口径：两边各算一份会出现
/// 「表里写着正在登录、面板还摇头显示旧额度」这种自相矛盾的画面。
pub(crate) enum OAuthState {
    /// 正在跑登录（浏览器授权，最多 10 分钟）
    LoggingIn,
    /// 正在查额度，且还没有旧读数
    Checking,
    /// 还没查过
    NotQueried,
    /// 没有凭据
    NotConfigured,
    /// 有读数
    Ready,
    /// 查询或凭据出错
    Failed,
}

/// 当前厂商是内置 OpenAI 时的 AUTH 状态；其它厂商 → `None`。
pub(crate) fn oauth_state(app: &App) -> Option<OAuthState> {
    if app.current_provider_id() != Some("openai") {
        return None;
    }
    Some(if app.oauth_login_running {
        OAuthState::LoggingIn
    } else if app.oauth_checking && app.oauth_balance.is_none() {
        OAuthState::Checking
    } else {
        match &app.oauth_balance {
            None => OAuthState::NotQueried,
            Some(Ok(values)) if values.is_empty() => OAuthState::NotConfigured,
            Some(Ok(_)) => OAuthState::Ready,
            Some(Err(_)) => OAuthState::Failed,
        }
    })
}

/// 密钥表「AUTH」行的状态标签（口径见 `oauth_state`）。
/// `None` = 当前不是内置 OpenAI，不画这一行。
pub(crate) fn oauth_row_status(app: &App) -> Option<(Style, &'static str)> {
    Some(match oauth_state(app)? {
        OAuthState::LoggingIn => (Style::new().fg(theme::MUTED), "… 登录中"),
        OAuthState::Checking => (Style::new().fg(theme::MUTED), "… 查询中"),
        OAuthState::NotQueried => (Style::new().fg(theme::MUTED), "—"),
        OAuthState::NotConfigured => (Style::new().fg(theme::MUTED), "未配置"),
        OAuthState::Ready => (Style::new().fg(theme::OK), "● 已连接"),
        OAuthState::Failed => (Style::new().fg(theme::ERR), "● 失败"),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;

    /// AUTH 的状态口径只有这一处：密钥表那一行与额度面板都必须从这里取，
    /// 两个面板不许各算一份（否则会出现「表里写着正在登录、面板还显示旧额度」）。
    #[test]
    fn oauth_state_is_one_shared_source() {
        let (mut app, _rx, _rx_task) = test_app(&[("openai", &["api-key"]), ("alpha", &["a1"])]);

        // 非内置 OpenAI：两边都不该画 AUTH
        app.focus_provider("alpha");
        assert!(oauth_state(&app).is_none());
        assert!(oauth_row_status(&app).is_none());

        // 内置 OpenAI，且还没查过
        app.focus_provider("openai");
        assert!(matches!(oauth_state(&app), Some(OAuthState::NotQueried)));
        assert_eq!(oauth_row_status(&app).unwrap().1, "—");

        // 登录进行中：密钥表与额度面板看到的是同一个状态（不是“查询中”，也不是旧读数）
        app.oauth_login_running = true;
        assert!(matches!(oauth_state(&app), Some(OAuthState::LoggingIn)));
        assert_eq!(oauth_row_status(&app).unwrap().1, "… 登录中");

        // 没有凭据
        app.oauth_login_running = false;
        app.oauth_balance = Some(Ok(Vec::new()));
        assert!(matches!(oauth_state(&app), Some(OAuthState::NotConfigured)));
        assert_eq!(oauth_row_status(&app).unwrap().1, "未配置");

        // 有读数 / 出错
        app.oauth_balance = Some(Ok(vec!["Codex 1mo：已用 27%".into()]));
        assert!(matches!(oauth_state(&app), Some(OAuthState::Ready)));
        assert_eq!(oauth_row_status(&app).unwrap().1, "● 已连接");
        app.oauth_balance = Some(Err("Codex 用量请求失败".into()));
        assert!(matches!(oauth_state(&app), Some(OAuthState::Failed)));
        assert_eq!(oauth_row_status(&app).unwrap().1, "● 失败");
    }
}
