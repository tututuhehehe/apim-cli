//! 一键导入面板的渲染：选客户端 → 勾选模型 → 进行中 → 失败。

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{centered, draw_search_box, pane_block, scroll_offset, theme};
use crate::app::{ImportFlow, ImportStep};
use crate::clients::Agent;

/// 模型列表最多显示的行数，超出靠滚动。
const VISIBLE_ROWS: usize = 14;

pub(crate) fn draw_import(frame: &mut Frame, flow: &ImportFlow, area: Rect) {
    match flow.step {
        ImportStep::Agent => draw_agent(frame, flow, area),
        ImportStep::Models => draw_model_picker(frame, flow, area),
        ImportStep::DefaultModel => draw_default_picker(frame, flow, area),
        ImportStep::Working => draw_message(
            frame,
            " 一键导入 ",
            &format!(
                "正在写入 {} 配置（{}）并校验…",
                flow.agent.label(),
                flow.agent.config_hint()
            ),
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

fn draw_agent(frame: &mut Frame, flow: &ImportFlow, area: Rect) {
    // 一行一个客户端（显示名）+ 空行 + 快捷键，再加 2 行边框。
    // 光标行的说明一律不展开：这一屏只负责选谁，客户端细节写在各自子模块的文档里。
    let height = (Agent::ALL.len() + 4) as u16;
    let rect = centered(76, height, area);
    frame.render_widget(Clear, rect);
    let block = pane_block(" 一键导入 · 选择客户端 ", true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<Line> = Vec::new();
    for agent in Agent::ALL.iter() {
        let selected = *agent == flow.agent;
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
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " j/k 移动   ⏎ 下一步   Esc 取消 ",
        Style::new().fg(theme::MUTED),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
        let offset = scroll_offset(flow.cursor, VISIBLE_ROWS);
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

/// 第三步：从已勾选的模型里选一个当默认（各客户端自己决定写到哪里）。
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
                " 这 {} 个模型都会写进客户端；选中的那个作为它的默认模型",
                checked.len()
            ),
            Style::new().fg(theme::MUTED),
        )))
        .wrap(Wrap { trim: true }),
        chunks[0],
    );

    let offset = scroll_offset(flow.default_cursor, VISIBLE_ROWS);
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
    let name_style = if is_cursor {
        Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::TEXT)
    };
    Line::from(vec![
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
    ])
}

fn count_line(flow: &ImportFlow) -> Line<'static> {
    let text = format!(" 已勾选 {}/{}", flow.checked_count(), flow.items.len());
    Line::from(Span::styled(text, Style::new().fg(theme::MUTED)))
}

// ---- 公共零件 ----------------------------------------------------------

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
