//! 「一键导入到客户端」面板：选客户端 → 勾选模型 → 写入 → 校验 → 反馈。
//!
//! 面板状态存在 `Modal::Import(ImportFlow)` 里，本文件管状态迁移与按键，`ui/import.rs`
//! 只管渲染，真正改写 codex 配置的逻辑在 `crate::clients::codex`。

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Focus, Modal, TaskMsg};
use crate::clients::{Agent, CodexState, ImportReport, ImportRequest};
use crate::probe;

/// 面板当前停在哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    /// 第一步：选目标客户端。
    Agent,
    /// 第二步：勾选要导入的模型（多选）。
    Models,
    /// 第三步：从已勾选的模型里选哪个当默认（写进 codex 顶层 `model`）。
    /// 只勾了一个模型时自动跳过这步。
    DefaultModel,
    /// 正在写入 + 校验（不可交互，避免半截状态）。
    Working,
    /// 失败，显示原因（Esc/Enter 关闭）。
    Failed,
}

/// 面板里的一个模型条目（列表和 `m` 键浏览用的是同一份模型名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPick {
    pub name: String,
    pub checked: bool,
}

/// 一键导入面板的完整状态（迟到的结果按 key_id 匹配，面板关掉就丢弃）。
#[derive(Debug, Clone)]
pub struct ImportFlow {
    pub key_id: String,
    pub step: ImportStep,
    /// Agent 步骤的选中下标。
    pub agent: usize,
    /// Models 步骤是否还在拉模型列表。
    pub loading: bool,
    pub items: Vec<ModelPick>,
    /// 「可见列表」里的光标下标。
    pub cursor: usize,
    /// DefaultModel 步骤的光标下标（指向「已勾选模型」列表）。
    pub default_cursor: usize,
    pub filter: String,
    pub searching: bool,
    /// Failed 步骤的失败原因。
    pub error: Option<String>,
}

impl ImportFlow {
    pub fn new(key_id: String) -> Self {
        Self {
            key_id,
            step: ImportStep::Agent,
            agent: 0,
            loading: false,
            items: Vec::new(),
            cursor: 0,
            default_cursor: 0,
            filter: String::new(),
            searching: false,
            error: None,
        }
    }

    /// 已勾选的模型名，按 items 顺序（顺带决定默认模型面板的列表顺序）。
    pub fn checked_models(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| item.checked)
            .map(|item| item.name.clone())
            .collect()
    }

    /// 可见条目在 `items` 里的下标：只按搜索关键字筛。
    ///
    /// **不按厂商声明的端点能力隐藏任何模型**：`supported_endpoint_types` 是 new-api 后台的
    /// 端点映射配置，不是真实能力探测 —— 实测 ikun 把 `gpt-6-sol` 标成只有 `openai`，
    /// 但它在 Codex 里（走 /responses）完全能用。拿它当硬过滤会把能用的模型藏起来，
    /// 所以只把声明当作行尾提示，不参与筛选。
    pub fn visible(&self) -> Vec<usize> {
        let needle = self.filter.trim().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| needle.is_empty() || item.name.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn checked_count(&self) -> usize {
        self.items.iter().filter(|item| item.checked).count()
    }

    /// 光标当前指向的 item 下标（可见列表为空时为 None）。
    pub fn cursor_item(&self) -> Option<usize> {
        self.visible().get(self.cursor).copied()
    }
}

/// 后台导入任务的结果回执。
#[derive(Debug)]
pub struct ImportOutcome {
    pub key_id: String,
    pub result: Result<ImportReport, String>,
    /// 成功后记录的状态（用于 ★ 标记与面板提示）。
    pub state: Option<CodexState>,
    /// 状态文件写失败时的说明（导入本身是成功的）。
    pub state_error: Option<String>,
    /// 重启掉的 codex 守护进程数（0 = 当时没在跑，或已被 APIM_NO_RESTART_CODEX 禁用）。
    pub restarted: usize,
}

impl App {
    fn import_flow(&self) -> Option<&ImportFlow> {
        match &self.modal {
            Modal::Import(flow) => Some(flow),
            _ => None,
        }
    }

    /// 当前面板里已勾选的模型名（None = 没有导入面板）。
    fn import_checked(&self) -> Vec<String> {
        self.import_flow()
            .map(ImportFlow::checked_models)
            .unwrap_or_default()
    }

    fn import_flow_mut(&mut self) -> Option<&mut ImportFlow> {
        match &mut self.modal {
            Modal::Import(flow) => Some(flow),
            _ => None,
        }
    }

    /// 面板正在等这个密钥的模型列表 / 导入结果（迟到结果据此丢弃）。
    pub fn import_awaiting(&self, key_id: &str, step: ImportStep) -> bool {
        self.import_flow()
            .is_some_and(|flow| flow.key_id == key_id && flow.step == step)
    }

    /// 密钥栏按 `x`：打开面板第一步（选客户端）。
    pub fn open_import(&mut self) {
        if self.focus != Focus::Keys {
            self.toast = Some(("先在右侧选中一把密钥再按 x".into(), Instant::now()));
            return;
        }
        let Some(key) = self.selected_key_entry().cloned() else {
            self.toast = Some(("没有可导入的密钥".into(), Instant::now()));
            return;
        };
        if !self.recipes.contains_key(&key.provider) {
            self.toast = Some((
                format!("厂商 {} 的协议不存在，先修好再导入", key.provider),
                Instant::now(),
            ));
            return;
        }
        self.modal = Modal::Import(ImportFlow::new(key.id()));
    }

    pub fn import_move_agent(&mut self, delta: isize) {
        let count = Agent::ALL.len();
        if let Some(flow) = self.import_flow_mut() {
            flow.agent = clamp_index(flow.agent, delta, count);
        }
    }

    /// 第一步选定客户端：拉这把密钥能看到的模型列表。
    pub fn import_choose_agent(&mut self) {
        let Some(flow) = self.import_flow() else {
            return;
        };
        let key_id = flow.key_id.clone();
        let Some(key) = self.keys.iter().find(|key| key.id() == key_id).cloned() else {
            self.toast = Some(("这把密钥已不存在".into(), Instant::now()));
            self.modal = Modal::None;
            return;
        };
        let Some(recipe) = self.recipes.get(&key.provider).cloned() else {
            self.toast = Some((
                format!("厂商 {} 的协议不存在", key.provider),
                Instant::now(),
            ));
            self.modal = Modal::None;
            return;
        };
        if let Some(flow) = self.import_flow_mut() {
            flow.step = ImportStep::Models;
            flow.loading = true;
            flow.items.clear();
            flow.cursor = 0;
            flow.default_cursor = 0;
            flow.filter.clear();
            flow.searching = false;
            flow.error = None;
        }
        let client = self.client.clone();
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let result = probe::fetch_models(&client, &recipe, &key.token).await;
            let _ = tx.send(TaskMsg::Models(key_id, result));
        });
    }

    /// 模型列表落地。
    pub fn import_receive_models(&mut self, key_id: String, result: Result<Vec<String>, String>) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        if flow.key_id != key_id || flow.step != ImportStep::Models {
            return;
        }
        flow.loading = false;
        let names = match result {
            Ok(names) => names,
            Err(message) => {
                flow.step = ImportStep::Failed;
                flow.error = Some(message);
                return;
            }
        };
        flow.items = names
            .into_iter()
            .map(|name| ModelPick {
                name,
                checked: false,
            })
            .collect();
        if flow.items.is_empty() {
            flow.step = ImportStep::Failed;
            flow.error = Some("这把密钥看不到任何模型".into());
        }
    }

    pub fn import_move(&mut self, delta: isize) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        let count = flow.visible().len();
        flow.cursor = clamp_index(flow.cursor, delta, count);
    }

    pub fn import_toggle(&mut self) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        if let Some(index) = flow.cursor_item() {
            flow.items[index].checked = !flow.items[index].checked;
        }
    }

    /// `a`：可见的全勾上；已经全勾了则全取消。
    pub fn import_toggle_all(&mut self) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        let visible = flow.visible();
        let all_checked = visible.iter().all(|index| flow.items[*index].checked);
        for index in visible {
            flow.items[index].checked = !all_checked;
        }
    }

    /// 第二步确认（`⏎`）：勾完模型后去选默认模型；只勾了一个就跳过那一步直接写。
    pub fn import_confirm_models(&mut self) {
        let checked = self.import_checked();
        if checked.is_empty() {
            self.toast = Some(("至少勾选一个模型".into(), Instant::now()));
            return;
        }
        if checked.len() == 1 {
            let only = checked[0].clone();
            self.start_import(checked, only);
            return;
        }
        if let Some(flow) = self.import_flow_mut() {
            flow.step = ImportStep::DefaultModel;
            flow.default_cursor = 0;
        }
    }

    pub fn import_move_default(&mut self, delta: isize) {
        let count = self.import_checked().len();
        if let Some(flow) = self.import_flow_mut() {
            flow.default_cursor = clamp_index(flow.default_cursor, delta, count);
        }
    }

    /// 第三步确认：光标处那个已勾选模型当默认（写进 codex 的 `model`），然后开写。
    pub fn import_confirm_default(&mut self) {
        let checked = self.import_checked();
        let index = self
            .import_flow()
            .map(|flow| flow.default_cursor)
            .unwrap_or(0)
            .min(checked.len().saturating_sub(1));
        let Some(default_model) = checked.get(index).cloned() else {
            return;
        };
        self.start_import(checked, default_model);
    }

    /// 从「选默认模型」退回「勾选模型」。
    pub fn import_default_back(&mut self) {
        if let Some(flow) = self.import_flow_mut() {
            flow.step = ImportStep::Models;
        }
    }

    pub fn import_start_search(&mut self) {
        if let Some(flow) = self.import_flow_mut() {
            flow.searching = true;
        }
    }

    pub fn import_exit_search(&mut self) {
        if let Some(flow) = self.import_flow_mut() {
            flow.searching = false;
        }
    }

    pub fn import_search_char(&mut self, c: char) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        flow.filter.push(c);
        flow.cursor = 0;
    }

    pub fn import_search_backspace(&mut self) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        flow.filter.pop();
        flow.cursor = 0;
    }

    pub fn import_searching(&self) -> bool {
        self.import_flow().is_some_and(|flow| flow.searching)
    }

    /// 真正开写：后台线程跑 codex 导入 + 校验 + 重启 daemon。
    fn start_import(&mut self, models: Vec<String>, default_model: String) {
        let Some(flow) = self.import_flow() else {
            return;
        };
        let key_id = flow.key_id.clone();

        let Some(key) = self.keys.iter().find(|key| key.id() == key_id).cloned() else {
            self.toast = Some(("这把密钥已不存在".into(), Instant::now()));
            self.modal = Modal::None;
            return;
        };
        let Some(recipe) = self.recipes.get(&key.provider).cloned() else {
            self.toast = Some((
                format!("厂商 {} 的协议不存在", key.provider),
                Instant::now(),
            ));
            self.modal = Modal::None;
            return;
        };
        if let Some(flow) = self.import_flow_mut() {
            flow.step = ImportStep::Working;
            flow.error = None;
        }

        let request = ImportRequest {
            provider_id: recipe.id.clone(),
            provider_name: recipe.name.clone(),
            base_url: recipe.base_url.clone(),
            api_key: key.token.clone(),
            alias: key.alias.clone(),
            models,
            default_model,
        };
        let request_key_id = request.key_id();
        let fallback_key_id = request_key_id.clone();
        let config_dir = self.config_dir.clone();
        let restart_daemon = self.restart_codex_daemon;
        let tx = self.tx_task.clone();
        tokio::spawn(async move {
            let outcome = tokio::task::spawn_blocking(move || {
                let result = crate::clients::codex::import(&request);
                let (state, state_error) = match &result {
                    Ok(report) => {
                        let (state, error) =
                            crate::clients::codex::remember(&config_dir, &request, report);
                        (Some(state), error)
                    }
                    Err(_) => (None, None),
                };
                // 写成功才重启：codex 只会在 daemon 启动时读一次模型目录
                let restarted = if result.is_ok() && restart_daemon {
                    crate::clients::codex::restart_daemon().unwrap_or(0)
                } else {
                    0
                };
                ImportOutcome {
                    key_id: request_key_id,
                    result,
                    state,
                    state_error,
                    restarted,
                }
            })
            .await
            .unwrap_or_else(|err| ImportOutcome {
                key_id: fallback_key_id,
                result: Err(format!("导入任务异常终止：{err}")),
                state: None,
                state_error: None,
                restarted: 0,
            });
            let _ = tx.send(TaskMsg::Import(Box::new(outcome)));
        });
    }

    /// 导入结果落地：成功就给成功反馈并关面板，失败留下面板显示原因。
    pub fn import_result(&mut self, outcome: ImportOutcome) {
        let waiting = self
            .import_flow()
            .is_some_and(|flow| flow.key_id == outcome.key_id);
        match outcome.result {
            Ok(report) => {
                if let Some(state) = outcome.state {
                    self.codex = Some(state);
                }
                let mut note = format!(
                    "已导入 {}：{} · {} 个模型 · 默认 {} · 强度 {}",
                    Agent::Codex.label(),
                    outcome.key_id,
                    report.models.len(),
                    report.model,
                    report.reasoning_effort
                );
                if let Some(backup) = &report.backup_path {
                    note.push_str(&format!(" · 旧配置备份为 {}", file_name(backup)));
                }
                // codex 的模型目录只在 app-server 启动时读一次，必须重启才能让 /model 刷新
                if outcome.restarted > 0 {
                    note.push_str(&format!(
                        " · 已重启 codex 守护进程({} 个)，重开 codex 即可看到新模型",
                        outcome.restarted
                    ));
                } else {
                    note.push_str(" · 重启 codex 后 /model 才会列出新模型");
                }
                if let Some(error) = outcome.state_error {
                    note.push_str(&format!("（状态未记录：{error}）"));
                }
                self.toast = Some((note, Instant::now()));
                if waiting {
                    self.modal = Modal::None;
                }
            }
            Err(message) => {
                if waiting {
                    if let Some(flow) = self.import_flow_mut() {
                        flow.step = ImportStep::Failed;
                        flow.error = Some(message);
                    }
                } else {
                    self.toast = Some((format!("导入失败：{message}"), Instant::now()));
                }
            }
        }
    }
}

/// 取路径的文件名（提示条上只显示文件名，别撑爆一行）。
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("备份")
        .to_string()
}

/// 把下标按 delta 移动并钳进 `[0, count)`；count 为 0 时回到 0。
fn clamp_index(current: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, count as isize - 1) as usize
}

/// 面板里的按键路由。返回 true 表示这个按键已被面板消费。
pub(crate) fn handle_import_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(step) = app.import_flow().map(|flow| flow.step) else {
        return false;
    };
    let searching = app.import_searching();
    match step {
        ImportStep::Agent => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.import_move_agent(1),
            KeyCode::Char('k') | KeyCode::Up => app.import_move_agent(-1),
            KeyCode::Enter => app.import_choose_agent(),
            KeyCode::Esc | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
        ImportStep::Models if searching => match key.code {
            KeyCode::Enter => app.import_exit_search(),
            KeyCode::Esc => app.import_exit_search(),
            KeyCode::Backspace => app.import_search_backspace(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.import_search_char(c)
            }
            _ => return false,
        },
        ImportStep::Models => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.import_move(1),
            KeyCode::Char('k') | KeyCode::Up => app.import_move(-1),
            KeyCode::Char(' ') => app.import_toggle(),
            KeyCode::Char('a') => app.import_toggle_all(),
            KeyCode::Char('/') => app.import_start_search(),
            KeyCode::Enter => app.import_confirm_models(),
            KeyCode::Esc | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
        // 默认模型：选完开写；h/← 退回上一步重选
        ImportStep::DefaultModel => match key.code {
            KeyCode::Char('j') | KeyCode::Down => app.import_move_default(1),
            KeyCode::Char('k') | KeyCode::Up => app.import_move_default(-1),
            KeyCode::Enter => app.import_confirm_default(),
            KeyCode::Char('h') | KeyCode::Left => app.import_default_back(),
            KeyCode::Esc | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
        // 正在写配置：忽略按键；Esc 允许关面板（任务继续，结果走 toast）
        ImportStep::Working => match key.code {
            KeyCode::Esc => app.cancel_modal(),
            _ => return false,
        },
        ImportStep::Failed => match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.cancel_modal(),
            _ => return false,
        },
    }
    true
}

#[cfg(test)]
mod tests;
