//! 详情检查器：只读弹窗。厂商栏按 i 看 recipe 配置（鉴权/端点/脚本/来源/vars），
//! 密钥栏按 i 看密钥完整信息（r 显隐完整 token，c 复制）。数据在渲染时从 App
//! 现查（不快照进弹窗），打开期间自动刷新后内容同步更新。

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{App, Modal, centered, pane_block, theme};
use crate::app::{InspectorTarget, KeyState};
use crate::config::{KeyEntry, mask_token};
use crate::probe::{BalanceSnapshot, Health};
use crate::recipe::{Auth, AuthKind, BalanceMode, Recipe};

/// 弹窗宽度（列）。
const WIDTH: u16 = 64;
/// 标签列宽（显示列，中文按 2 列算）。
const LABEL_COLS: usize = 10;

/// 一行详情：标签 MUTED 左对齐，值 TEXT；hint 紧跟值后（暗色按键提示）。
pub(crate) struct InspectRow {
    label: String,
    value: String,
    hint: Option<String>,
    /// true = 值不截断（token 全文行：宁可被终端裁剪，也不加省略号）。
    unbounded: bool,
}

impl InspectRow {
    fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            hint: None,
            unbounded: false,
        }
    }

    fn with_hint(label: &str, value: impl Into<String>, hint: &str) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            hint: Some(hint.into()),
            unbounded: true,
        }
    }
}

// ---- 行构建（纯函数，便于单测） ----------------------------------------

/// 厂商详情行。vars 只列变量名，值一律遮掩为 ••••——变量可能存访问令牌。
pub(crate) fn provider_rows(
    recipe: &Recipe,
    key_count: usize,
    groups: &[String],
) -> Vec<InspectRow> {
    let mut rows = vec![
        InspectRow::new("名称", recipe.name.clone()),
        InspectRow::new("ID", recipe.id.clone()),
        InspectRow::new(
            "来源",
            recipe
                .origin
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "内置".into()),
        ),
        InspectRow::new("Base URL", recipe.base_url.clone()),
        InspectRow::new(
            "主页",
            recipe
                .homepage
                .clone()
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| "—".into()),
        ),
        InspectRow::new("鉴权", auth_label(&recipe.auth)),
        InspectRow::new(
            "探活",
            recipe
                .health
                .as_ref()
                .map(|h| {
                    let method = if h.method.is_empty() {
                        "GET"
                    } else {
                        h.method.as_str()
                    };
                    format!("{method} {}", h.url)
                })
                .unwrap_or_else(|| "未配置".into()),
        ),
        InspectRow::new("额度", balance_label(recipe.balance.as_ref())),
    ];
    let mut names: Vec<&String> = recipe.vars.keys().collect();
    names.sort();
    for name in names {
        rows.push(InspectRow::new("vars", format!("{name} = ••••")));
    }
    rows.push(InspectRow::new("密钥数", key_count.to_string()));
    rows.push(InspectRow::new(
        "分组",
        if groups.is_empty() {
            "—".into()
        } else {
            groups.join("、")
        },
    ));
    rows
}

/// 密钥详情行。token 行按 reveal 显示掩码或全文，行尾带按键提示。
pub(crate) fn key_rows(
    key: &KeyEntry,
    provider_name: &str,
    state: &KeyState,
    checking: bool,
    updated: Option<Duration>,
    reveal_token: bool,
) -> Vec<InspectRow> {
    vec![
        InspectRow::new("厂商", provider_name),
        InspectRow::new("别名", key.alias.clone()),
        InspectRow::new("分组", key.group_label()),
        InspectRow::with_hint(
            "密钥",
            if reveal_token {
                key.token.clone()
            } else {
                mask_token(&key.token)
            },
            "r 显隐 · c 复制",
        ),
        InspectRow::new("健康", health_label(state, checking)),
        InspectRow::new("余额", balance_summary(state.balance.as_ref())),
        InspectRow::new(
            "上次探测",
            updated.map(human_elapsed).unwrap_or_else(|| "—".into()),
        ),
    ]
}

fn auth_label(auth: &Auth) -> String {
    match auth.kind {
        AuthKind::Bearer => match auth.header.as_deref() {
            Some(header) => format!("bearer · {header}"),
            None => "bearer".into(),
        },
        AuthKind::Header => format!(
            "header · {}",
            auth.header.as_deref().unwrap_or("Authorization")
        ),
        AuthKind::Query => format!(
            "query · {}",
            auth.query_param.as_deref().unwrap_or("api_key")
        ),
    }
}

/// 额度行：HTTP 形态带解析字段数；脚本形态带命令与超时（缺省 15s，同 probe）。
fn balance_label(balance: Option<&BalanceMode>) -> String {
    match balance {
        None => "未配置".into(),
        Some(BalanceMode::Http(spec)) => format!(
            "HTTP {} {} · {} 个解析字段",
            spec.request.method,
            spec.request.url,
            spec.render.fields.len()
        ),
        Some(BalanceMode::Script(spec)) => format!(
            "脚本 {} · 超时 {}s",
            spec.command.as_deref().unwrap_or("(内联)"),
            spec.timeout_secs.unwrap_or(15)
        ),
    }
}

fn health_label(state: &KeyState, checking: bool) -> String {
    if checking {
        return "… 检查中".into();
    }
    match &state.health {
        Health::Unknown => "未配置探活".into(),
        Health::Checking => "… 检查中".into(),
        Health::Live { ms } => {
            // 与主界面状态列同口径（keys.rs status_label）：额度接口明确说账号
            // 不可用/无余额时，不能在这里仍显示「可用」
            let available = state
                .balance
                .as_ref()
                .and_then(|b| b.view.as_ref())
                .and_then(|v| v.available);
            match available {
                Some(false) => format!("● 无额度（{ms}ms）"),
                _ => format!("● 可用 {ms}ms"),
            }
        }
        Health::Down { ms, message } => format!("● 失败 {message}（{ms}ms）"),
    }
}

/// 余额摘要：失败显示错误；有 view 显示 headline；否则「无额度数据」。
fn balance_summary(balance: Option<&BalanceSnapshot>) -> String {
    let Some(snap) = balance else {
        return "无额度数据".into();
    };
    if let Some(err) = &snap.error {
        return err.clone();
    }
    snap.view
        .as_ref()
        .map(|v| v.headline.clone())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "无额度数据".into())
}

fn human_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs} 秒前")
    } else if secs < 3600 {
        format!("{} 分钟前", secs / 60)
    } else {
        format!("{} 小时前", secs / 3600)
    }
}

// ---- 渲染 ---------------------------------------------------------------

pub(crate) fn draw_inspector(frame: &mut Frame, app: &App, area: Rect) {
    let Modal::Inspector {
        target,
        reveal_token,
    } = &app.modal
    else {
        return;
    };
    match target {
        InspectorTarget::Provider(id) => draw_provider(frame, app, id, area),
        InspectorTarget::Key(id) => draw_key(frame, app, id, *reveal_token, area),
    }
}

fn draw_provider(frame: &mut Frame, app: &App, id: &str, area: Rect) {
    let Some(recipe) = app.recipes.get(id) else {
        return;
    };
    let keys: Vec<&KeyEntry> = app.keys.iter().filter(|k| k.provider == id).collect();
    let mut groups: Vec<String> = keys
        .iter()
        .filter_map(|k| k.group.clone())
        .filter(|g| !g.is_empty())
        .collect();
    groups.sort();
    groups.dedup();
    let rows = provider_rows(recipe, keys.len(), &groups);
    render_rows(frame, &format!(" 厂商详情 · {} ", recipe.id), &rows, area);
}

fn draw_key(frame: &mut Frame, app: &App, key_id: &str, reveal_token: bool, area: Rect) {
    let Some(key) = app.keys.iter().find(|k| k.id() == key_id) else {
        return;
    };
    let provider_name = app
        .recipes
        .get(&key.provider)
        .map(|r| r.name.clone())
        .unwrap_or_else(|| key.provider.clone());
    let state = app.state_for(key);
    let rows = key_rows(
        key,
        &provider_name,
        &state,
        app.is_checking(key_id),
        state.updated.map(|at| at.elapsed()),
        reveal_token,
    );
    render_rows(frame, &format!(" 密钥详情 · {} ", key.id()), &rows, area);
}

fn render_rows(frame: &mut Frame, title: &str, rows: &[InspectRow], area: Rect) {
    let height = (rows.len() as u16 + 4).min(area.height);
    let rect = centered(WIDTH, height, area);
    frame.render_widget(Clear, rect);
    let block = pane_block(title.to_string(), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let value_cols = (inner.width as usize).saturating_sub(LABEL_COLS).max(8);
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|row| {
            let mut spans = vec![Span::styled(
                pad_label(&row.label),
                Style::new().fg(theme::MUTED),
            )];
            let max_cols = if row.unbounded {
                usize::MAX
            } else {
                value_cols
            };
            spans.push(Span::styled(
                truncate_cols(&row.value, max_cols),
                Style::new().fg(theme::TEXT),
            ));
            if let Some(hint) = &row.hint {
                spans.push(Span::styled(
                    format!("  {hint}"),
                    Style::new().fg(theme::MUTED),
                ));
            }
            Line::from(spans)
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Enter/Esc/q 关闭",
        Style::new().fg(theme::MUTED),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// 按显示宽度补空格（`{:<10}` 按字符数补，中文标签会歪）。
fn pad_label(label: &str) -> String {
    let mut out = label.to_string();
    for _ in UnicodeWidthStr::width(label)..LABEL_COLS {
        out.push(' ');
    }
    out
}

/// 按显示宽度截断，超长加 …。
fn truncate_cols(s: &str, max_cols: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_cols {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 1; // 预留省略号一位
    for ch in s.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > max_cols {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{
        BalanceSpec, BalanceView, HttpCall, ParseSpec, RenderField, RenderSpec, ScriptSpec,
    };
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::time::Instant;

    fn recipe(balance: Option<BalanceMode>, vars: &[(&str, &str)], origin: Option<&str>) -> Recipe {
        Recipe {
            id: "demo".into(),
            name: "Demo".into(),
            base_url: "https://api.demo.com".into(),
            homepage: None,
            supports_groups: false,
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            auth: Auth::default(),
            health: Some(HttpCall::get("{base_url}/models")),
            balance,
            origin: origin.map(PathBuf::from),
        }
    }

    fn http_balance() -> BalanceMode {
        BalanceMode::Http(Box::new(BalanceSpec {
            request: HttpCall::get("{base_url}/user/balance"),
            parse: ParseSpec::default(),
            render: RenderSpec {
                headline: "{total_balance}".into(),
                fields: vec![
                    RenderField {
                        label: "总额".into(),
                        value: "{total_balance}".into(),
                    },
                    RenderField {
                        label: "充值".into(),
                        value: "{topped_up_balance}".into(),
                    },
                    RenderField {
                        label: "赠款".into(),
                        value: "{granted_balance}".into(),
                    },
                ],
            },
        }))
    }

    fn key() -> KeyEntry {
        KeyEntry {
            provider: "demo".into(),
            alias: "main".into(),
            group: Some("个人".into()),
            token: "sk-abcdef1234567890abcdef".into(),
        }
    }

    fn row(rows: &[InspectRow], label: &str) -> String {
        rows.iter()
            .find(|r| r.label == label)
            .map(|r| r.value.clone())
            .unwrap_or_default()
    }

    fn joined(rows: &[InspectRow]) -> String {
        rows.iter()
            .map(|r| format!("{} {}", r.label, r.value))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn provider_rows_builtin_script_masks_var_values() {
        let recipe = recipe(
            Some(BalanceMode::Script(ScriptSpec {
                command: Some("~/.config/apim/scripts/demo.sh".into()),
                ..Default::default()
            })),
            &[("access_token", "at-secret-value")],
            None,
        );
        let rows = provider_rows(&recipe, 2, &["个人".into()]);
        assert_eq!(row(&rows, "来源"), "内置");
        let balance = row(&rows, "额度");
        assert!(balance.contains("demo.sh"), "{balance}");
        assert!(balance.contains("超时 15s"), "{balance}");
        assert_eq!(row(&rows, "探活"), "GET {base_url}/models");
        assert_eq!(row(&rows, "密钥数"), "2");
        assert_eq!(row(&rows, "分组"), "个人");
        let all = joined(&rows);
        assert!(all.contains("access_token"), "{all}");
        assert!(all.contains("••••"), "{all}");
        assert!(
            !all.contains("at-secret-value"),
            "vars 值不得出现在详情里: {all}"
        );
    }

    #[test]
    fn provider_rows_http_balance_and_user_origin() {
        let mut recipe = recipe(
            Some(http_balance()),
            &[],
            Some("/home/u/.config/apim/recipes/demo.yaml"),
        );
        recipe.auth = Auth {
            kind: AuthKind::Header,
            header: Some("X-Api-Key".into()),
            ..Default::default()
        };
        let rows = provider_rows(&recipe, 0, &[]);
        let balance = row(&rows, "额度");
        assert!(
            balance.contains("HTTP GET {base_url}/user/balance"),
            "{balance}"
        );
        assert!(balance.contains("3 个解析字段"), "{balance}");
        assert_eq!(row(&rows, "来源"), "/home/u/.config/apim/recipes/demo.yaml");
        assert!(
            row(&rows, "鉴权").contains("X-Api-Key"),
            "{}",
            row(&rows, "鉴权")
        );
        assert_eq!(row(&rows, "分组"), "—");
    }

    #[test]
    fn key_rows_token_masked_by_default_reveals_on_demand() {
        let key = key();
        let masked = key_rows(&key, "Demo", &KeyState::default(), false, None, false);
        let token_row = masked.iter().find(|r| r.label == "密钥").unwrap();
        assert_eq!(token_row.value, mask_token(&key.token));
        assert_eq!(token_row.hint.as_deref(), Some("r 显隐 · c 复制"));
        assert!(
            !joined(&masked).contains(&key.token),
            "遮掩态输出不得含完整 token"
        );

        let revealed = key_rows(&key, "Demo", &KeyState::default(), false, None, true);
        let token_row = revealed.iter().find(|r| r.label == "密钥").unwrap();
        assert_eq!(token_row.value, key.token);
    }

    #[test]
    fn key_rows_empty_states() {
        let rows = key_rows(&key(), "Demo", &KeyState::default(), false, None, false);
        assert_eq!(row(&rows, "健康"), "未配置探活");
        assert_eq!(row(&rows, "余额"), "无额度数据");
        assert_eq!(row(&rows, "上次探测"), "—");
        assert_eq!(row(&rows, "分组"), "个人");
        assert_eq!(row(&rows, "厂商"), "Demo");
    }

    #[test]
    fn key_rows_health_balance_and_updated_variants() {
        let key = key();
        let live = KeyState {
            health: Health::Live { ms: 101 },
            balance: Some(BalanceSnapshot {
                view: Some(BalanceView {
                    available: Some(true),
                    headline: "4.22".into(),
                    items: Vec::new(),
                    lines: Vec::new(),
                }),
                endpoint: "GET https://api.demo.com/user/balance".into(),
                status: Some(200),
                elapsed_ms: 88,
                error: None,
            }),
            updated: Some(Instant::now()),
        };
        let rows = key_rows(
            &key,
            "Demo",
            &live,
            false,
            Some(Duration::from_secs(45)),
            false,
        );
        assert_eq!(row(&rows, "健康"), "● 可用 101ms");
        assert_eq!(row(&rows, "余额"), "4.22");
        assert_eq!(row(&rows, "上次探测"), "45 秒前");

        // 与主界面状态列同口径：额度接口说账号不可用时显示「无额度」而非「可用」
        let no_quota = KeyState {
            health: Health::Live { ms: 33 },
            balance: Some(BalanceSnapshot {
                view: Some(BalanceView {
                    available: Some(false),
                    headline: "0".into(),
                    items: Vec::new(),
                    lines: Vec::new(),
                }),
                endpoint: "GET https://api.demo.com/user/balance".into(),
                status: Some(200),
                elapsed_ms: 9,
                error: None,
            }),
            updated: None,
        };
        let rows = key_rows(&key, "Demo", &no_quota, false, None, false);
        let health = row(&rows, "健康");
        assert!(health.contains("无额度"), "{health}");
        assert!(!health.contains("可用"), "{health}");

        let down = KeyState {
            health: Health::Down {
                ms: 12,
                message: "connection refused".into(),
            },
            balance: Some(BalanceSnapshot {
                view: None,
                endpoint: "script demo".into(),
                status: None,
                elapsed_ms: 5,
                error: Some("exit 3 · 配置坏了".into()),
            }),
            updated: None,
        };
        let rows = key_rows(&key, "Demo", &down, false, None, false);
        let health = row(&rows, "健康");
        assert!(health.contains("● 失败"), "{health}");
        assert!(health.contains("connection refused"), "{health}");
        assert!(row(&rows, "余额").contains("exit 3"));
        assert_eq!(row(&rows, "上次探测"), "—");
    }

    #[test]
    fn provider_rows_vars_sorted_and_missing_homepage_dash() {
        let recipe = recipe(None, &[("b_var", "v1"), ("a_var", "v2")], None);
        let rows = provider_rows(&recipe, 0, &[]);
        let var_values: Vec<&str> = rows
            .iter()
            .filter(|r| r.label == "vars")
            .map(|r| r.value.as_str())
            .collect();
        assert_eq!(var_values, vec!["a_var = ••••", "b_var = ••••"]);
        assert_eq!(row(&rows, "主页"), "—");
        assert_eq!(row(&rows, "额度"), "未配置");
    }

    // ---- 渲染路径（TestBackend，安全断言走真实 draw 输出） ------------------

    fn inspector_app(target: InspectorTarget, reveal_token: bool) -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let (tx_models, _rx_models) = tokio::sync::mpsc::unbounded_channel();
        let mut recipes = HashMap::new();
        let r = recipe(None, &[("access_token", "at-secret-value")], None);
        recipes.insert(r.id.clone(), r);
        let key = key();
        App {
            recipes,
            keys: vec![key],
            provider_ids: vec!["demo".into()],
            selected_provider: 0,
            selected_key: 0,
            focus: crate::app::Focus::Keys,
            states: HashMap::new(),
            toast: None,
            last_refresh: None,
            modal: Modal::Inspector {
                target,
                reveal_token,
            },
            inflight: HashSet::new(),
            tx,
            tx_models,
            client: reqwest::Client::new(),
            next_auto_refresh: Instant::now(),
        }
    }

    fn render_to_string(app: &App) -> String {
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_inspector(frame, app, frame.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            let mut skip = 0u16;
            for x in 0..buf.area.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let sym = buf[(x, y)].symbol();
                out.push_str(sym);
                let width = UnicodeWidthStr::width(sym) as u16;
                if width > 1 {
                    skip = width - 1; // 跳过宽字符的占位格
                }
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn rendered_key_inspector_masks_token_unless_revealed() {
        let app = inspector_app(InspectorTarget::Key("demo.main".into()), false);
        let out = render_to_string(&app);
        assert!(out.contains("密钥详情 · demo.main"), "{out}");
        assert!(out.contains(&mask_token(&key().token)), "{out}");
        assert!(out.contains("r 显隐"), "{out}");
        assert!(
            !out.contains("sk-abcdef1234567890abcdef"),
            "遮掩态渲染输出不得含完整 token: {out}"
        );

        let app = inspector_app(InspectorTarget::Key("demo.main".into()), true);
        let out = render_to_string(&app);
        assert!(
            out.contains("sk-abcdef1234567890abcdef"),
            "reveal 态应显示完整 token: {out}"
        );
    }

    #[test]
    fn rendered_provider_inspector_hides_var_values() {
        let app = inspector_app(InspectorTarget::Provider("demo".into()), false);
        let out = render_to_string(&app);
        assert!(out.contains("厂商详情 · demo"), "{out}");
        assert!(out.contains("内置"), "{out}");
        assert!(out.contains("access_token"), "{out}");
        assert!(
            !out.contains("at-secret-value"),
            "vars 值不得出现在渲染输出里: {out}"
        );
    }

    #[test]
    fn truncate_and_pad_helpers() {
        let long = "https://api.example.com/v1/dashboard/billing/subscription/balances";
        let cut = truncate_cols(long, 20);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(UnicodeWidthStr::width(cut.as_str()) <= 20);
        assert_eq!(truncate_cols("short", 20), "short");
        assert_eq!(pad_label("ID"), "ID        ");
        assert_eq!(pad_label("上次探测"), "上次探测  ");
    }
}
