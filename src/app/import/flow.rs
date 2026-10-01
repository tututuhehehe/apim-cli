//! 一键导入面板的纯状态：当前步骤、勾选情况、可见列表与搜索过滤。不碰 IO。

use crate::clients::Agent;

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
    /// 本次面板的请求代际（App 单调发号）。迟到的模型列表/导入回执代际不匹配就丢弃。
    pub seq: u64,
    pub step: ImportStep,
    /// Agent 步骤选中的客户端（真正参与后续分派，不只是画光标）。
    pub agent: Agent,
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
    pub fn new(key_id: String, seq: u64) -> Self {
        Self {
            key_id,
            seq,
            step: ImportStep::Agent,
            agent: Agent::Codex,
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
