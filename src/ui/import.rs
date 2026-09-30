//! 一键导入面板的渲染：选客户端 → 勾选模型 → 进行中 → 失败。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{centered, pane_block, theme};
use crate::app::{App, ImportFlow, ImportStep};
use crate::clients::Agent;

/// 模型列表最多显示的行数，超出靠滚动。
const VISIBLE_ROWS: usize = 14;

pub(crate) fn draw_import(frame: &mut Frame, app: &App, flow: &ImportFlow, area: Rect) {
    match flow.step {
        ImportStep::Agent => draw_agent(frame, app, flow, area),
        ImportStep::Models => draw_model_picker(frame, flow, area),
        ImportStep::DefaultModel => draw_default_picker(frame, flow, area),
        ImportStep::Working => draw_message(
            frame,
            " 一键导入 ",
            "正在写入 Codex 配置并让 codex 校验…",
            " Esc 关闭（任务继续，结果会以底部提示条反馈） ",
            theme::ACCENT,
            area,
        ),
        ImportStep::Failed => draw_message(
            frame,
            " 一键导入 · 失败 ",
            flow.error.as_deref().unwrap_or("未知错误"),
            " Esc 关闭 ",
            theme::ERR,
            area,
        ),
    }
}

// ---- 第一步：选客户端 --------------------------------------------------

fn draw_agent(frame: &mut Frame, app: &App, flow: &ImportFlow, area: Rect) {
    let height = (Agent::ALL.len() * 2 + 6) as u16;
    let rect = centered(76, height, area);
    frame.render_widget(Clear, rect);
    let block = pane_block(" 一键导入 · 选择客户端 ", true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<Line> = Vec::new();
    for (index, agent) in Agent::ALL.iter().enumerate() {
        let selected = index == flow.agent;
        let marker = if selected { "▶ " } else { "  " };
        let label_style = if selected {
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme::TEXT)
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}{}", agent.label()),
            label_style,
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", agent.config_hint()),
            Style::new().fg(theme::MUTED),
        )));
        if selected {
            lines.push(Line::from(Span::styled(
                format!("    {}", agent.note()),
                Style::new().fg(theme::MUTED),
            )));
        } else {
            lines.push(Line::from(""));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        flow_note(app, flow),
        Style::new().fg(theme::GOLD),
    )));
    lines.push(Line::from(Span::styled(
        " j/k 移动   ⏎ 下一步   Esc 取消 ",
        Style::new().fg(theme::MUTED),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// 面板上那句「当前 Codex 用的是谁 / 这次要导入谁」。
fn flow_note(app: &App, flow: &ImportFlow) -> String {
    match &app.codex {
        Some(state) if state.key_id() == flow.key_id => {
            format!(
                "当前：{} 已在 Codex（{} 个模型）",
                state.key_id(),
                state.models.len()
            )
        }
        Some(state) => format!(
            "将导入：{}（当前是 {}，它的配置块会保留）",
            flow.key_id,
            state.key_id()
        ),
        None => format!("将导入：{}", flow.key_id),
    }
}

// ---- 第二步：勾选模型 --------------------------------------------------

fn draw_model_picker(frame: &mut Frame, flow: &ImportFlow, area: Rect) {
    let visible = flow.visible();
    let rows = visible.len().clamp(1, VISIBLE_ROWS) as u16;
    let rect = centered(78, rows + 5, area);
    frame.render_widget(Clear, rect);
    let block = pane_block(format!(" 一键导入 · {} · 勾选模型 ", flow.key_id), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 搜索框
            Constraint::Min(1),    // 列表
            Constraint::Length(1), // 统计
            Constraint::Length(1), // 快捷键
        ])
        .split(inner);

    draw_search_box(frame, chunks[0], &flow.filter, flow.searching);

    if flow.loading {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "正在获取模型…",
                Style::new().fg(theme::MUTED),
            ))),
            chunks[1],
        );
    } else if visible.is_empty() {
        let note = if flow.filter.trim().is_empty() {
            "（这把密钥看不到任何模型）"
        } else {
            "（没有匹配的模型）"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                note,
                Style::new().fg(theme::MUTED),
            ))),
            chunks[1],
        );
    } else {
        let offset = flow.cursor.saturating_sub(VISIBLE_ROWS.saturating_sub(1));
        let lines: Vec<Line> = visible
            .iter()
            .enumerate()
            .skip(offset)
            .take(VISIBLE_ROWS)
            .map(|(position, index)| model_line(flow, position, *index))
            .collect();
        frame.render_widget(Paragraph::new(lines), chunks[1]);
    }

    frame.render_widget(Paragraph::new(count_line(flow)), chunks[2]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " 空格 勾选  a 全选/清空  / 搜索  ⏎ 下一步（选默认模型）  Esc 取消 ",
            Style::new().fg(theme::MUTED),
        ))),
        chunks[3],
    );
}

/// 第三步：从已勾选的模型里选一个当默认（写进 codex 顶层 `model`）。
fn draw_default_picker(frame: &mut Frame, flow: &ImportFlow, area: Rect) {
    let checked = flow.checked_models();
    let rows = checked.len().clamp(1, VISIBLE_ROWS) as u16;
    let rect = centered(78, rows + 4, area);
    frame.render_widget(Clear, rect);
    let block = pane_block(format!(" 一键导入 · {} · 选默认模型 ", flow.key_id), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 说明
            Constraint::Min(1),    // 已勾选列表
            Constraint::Length(1), // 快捷键
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                " 这 {} 个模型都会写进 codex；选中的那个作为默认（config.toml 的 model）",
                checked.len()
            ),
            Style::new().fg(theme::MUTED),
        )))
        .wrap(Wrap { trim: true }),
        chunks[0],
    );

    let offset = flow
        .default_cursor
        .saturating_sub(VISIBLE_ROWS.saturating_sub(1));
    let lines: Vec<Line> = checked
        .iter()
        .enumerate()
        .skip(offset)
        .take(VISIBLE_ROWS)
        .map(|(position, name)| {
            let is_cursor = position == flow.default_cursor;
            let mut spans = vec![
                Span::styled(
                    if is_cursor { "▶ " } else { "  " },
                    Style::new().fg(theme::ACCENT),
                ),
                Span::styled(
                    name.clone(),
                    if is_cursor {
                        Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(theme::TEXT)
                    },
                ),
            ];
            if is_cursor {
                spans.push(Span::styled(
                    format!(
                        "   ← 写进 config.toml 的 model（强度 {}）",
                        crate::clients::DEFAULT_EFFORT
                    ),
                    Style::new().fg(theme::GOLD),
                ));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[1]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " j/k 移动   ⏎ 导入   h 返回勾选   Esc 取消 ",
            Style::new().fg(theme::MUTED),
        ))),
        chunks[2],
    );
}

fn model_line(flow: &ImportFlow, position: usize, index: usize) -> Line<'static> {
    let item = &flow.items[index];
    let is_cursor = position == flow.cursor;
    let unsupported = item.responses == Some(false);
    let name_style = if unsupported {
        Style::new().fg(theme::MUTED)
    } else if is_cursor {
        Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::TEXT)
    };
    let mut spans = vec![
        Span::styled(
            if is_cursor { "▶ " } else { "  " },
            Style::new().fg(theme::ACCENT),
        ),
        Span::styled(
            if item.checked { "[×] " } else { "[ ] " },
            Style::new().fg(if item.checked {
                theme::OK
            } else {
                theme::MUTED
            }),
        ),
        Span::styled(item.name.clone(), name_style),
    ];
    if unsupported {
        // 只是厂商标注，实测会漏报，所以照旧可勾选
        spans.push(Span::styled(
            "   · 厂商标注不支持 responses（仍可勾选）",
            Style::new().fg(theme::MUTED),
        ));
    }
    Line::from(spans)
}

fn count_line(flow: &ImportFlow) -> Line<'static> {
    let mut text = format!(" 已勾选 {}/{}", flow.checked_count(), flow.items.len());
    let declared = flow.declared_unsupported_count();
    if declared > 0 {
        text.push_str(&format!(
            "   · {declared} 个厂商标注不支持 responses（只是提示，不影响勾选）"
        ));
    }
    Line::from(Span::styled(text, Style::new().fg(theme::MUTED)))
}

// ---- 公共零件 ----------------------------------------------------------

fn draw_search_box(frame: &mut Frame, area: Rect, filter: &str, searching: bool) {
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
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_message(
    frame: &mut Frame,
    title: &str,
    message: &str,
    hint: &str,
    color: ratatui::style::Color,
    area: Rect,
) {
    let rect = centered(76, 5, area);
    frame.render_widget(Clear, rect);
    let block = pane_block(title.to_string(), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let lines = vec![
        Line::from(Span::styled(message.to_string(), Style::new().fg(color))),
        Line::from(""),
        Line::from(Span::styled(
            hint.to_string(),
            Style::new().fg(theme::MUTED),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
