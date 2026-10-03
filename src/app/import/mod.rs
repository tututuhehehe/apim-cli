//! 「一键导入到客户端」面板：选客户端 → 勾选模型 → 写入 → 校验 → 反馈。
//!
//! 面板状态存在 `Modal::Import(ImportFlow)` 里，本文件管面板开关与流程控制：
//! - `flow.rs`：面板状态本身（步骤 / 勾选 / 可见列表 / 搜索过滤），不碰 IO；
//! - `apply.rs`：真正写盘与回执落地（经 `crate::clients` 适配层，面板不认识客户端细节）；
//! - `keys.rs`：按键路由；渲染在 `ui/import.rs`。

use std::time::Instant;

use super::{App, Focus, Modal, TaskMsg};
use crate::clients::Agent;
use crate::probe;

mod apply;
mod flow;
mod keys;

pub(crate) use apply::{ImportOutcome, ImportRunner};
pub(crate) use flow::{ImportFlow, ImportStep, ModelPick};
pub(crate) use keys::handle_import_key;

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

    /// 面板正在等这把密钥（且是这一代）的模型列表 / 导入结果（迟到结果据此丢弃）。
    pub fn import_awaiting(&self, key_id: &str, seq: u64, step: ImportStep) -> bool {
        self.import_flow()
            .is_some_and(|flow| flow.key_id == key_id && flow.seq == seq && flow.step == step)
    }

    /// 取一个新的导入代际号。
    fn next_import_seq(&mut self) -> u64 {
        self.next_import_seq += 1;
        self.next_import_seq
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
        let Some(recipe) = self.recipes.get(&key.provider) else {
            self.toast = Some((
                format!("厂商 {} 的协议不存在，先修好再导入", key.provider),
                Instant::now(),
            ));
            return;
        };
        // 导入客户端靠模型列表：非模型厂商没有模型可勾（Tab 分页里也没有 `x` 的提示）
        if !recipe.is_model() {
            self.toast = Some((
                format!("{} 是非模型厂商，不能导入到客户端", recipe.name),
                Instant::now(),
            ));
            return;
        }
        let seq = self.next_import_seq();
        self.modal = Modal::Import(ImportFlow::new(key.id(), seq));
    }

    /// 面板第一步：换选中的客户端（往上/下走一个）。
    pub fn import_move_agent(&mut self, delta: isize) {
        if let Some(flow) = self.import_flow_mut() {
            let current = Agent::ALL
                .iter()
                .position(|agent| *agent == flow.agent)
                .unwrap_or_default();
            let next = clamp_index(current, delta, Agent::ALL.len());
            flow.agent = Agent::ALL[next];
        }
    }

    /// 第一步选定客户端：拉这把密钥能看到的模型列表。
    pub fn import_choose_agent(&mut self) {
        let Some(flow) = self.import_flow() else {
            return;
        };
        let key_id = flow.key_id.clone();
        let seq = flow.seq;
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
            let _ = tx.send(TaskMsg::Models(key_id, seq, result));
        });
    }

    /// 模型列表落地。
    pub fn import_receive_models(
        &mut self,
        key_id: String,
        seq: u64,
        result: Result<Vec<String>, String>,
    ) {
        let Some(flow) = self.import_flow_mut() else {
            return;
        };
        // 代际不匹配 = 上一个面板（同一把密钥）发的请求，迟到结果直接丢
        if flow.key_id != key_id || flow.seq != seq || flow.step != ImportStep::Models {
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
        // 没有「默认模型」这一步的客户端（pi：默认模型由用户自己在 /model 里挑）：勾完直接开写，
        // 请求里也不带默认模型（`None`）。只勾一个模型时同理（codex 就把它当默认）。
        let needs_default = self
            .import_flow()
            .is_some_and(|flow| flow.agent.default_model_step().is_some());
        if checked.len() == 1 || !needs_default {
            let only = needs_default.then(|| checked[0].clone());
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

    /// 第三步确认：光标处那个已勾选模型当默认（写到哪里由客户端决定），然后开写。
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
        self.start_import(checked, Some(default_model));
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
}

/// 把下标按 delta 移动并钳进 `[0, count)`；count 为 0 时回到 0。
fn clamp_index(current: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, count as isize - 1) as usize
}

#[cfg(test)]
mod tests;
