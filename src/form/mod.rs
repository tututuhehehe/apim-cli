//! 通用表单引擎：字段导航、输入分发、弹窗数据。

mod edit;

pub use edit::LineEdit;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum Field {
    Text {
        label: String,
        edit: LineEdit,
        enabled: bool,
        /// true = 这一行不渲染、Tab 跳过、也写不进去（当前类型不适用这个字段）。
        hidden: bool,
    },
    Select {
        label: String,
        options: Vec<String>,
        selected: usize,
        hint: String,
    },
    /// 勾选框（布尔选项）。`enabled: false` = 只读展示（如「类型创建后不可改」）。
    /// `hides` 指向“勾上就不适用”的那一行（如勾上「非模型」后隐藏探活路径）：
    /// 勾选状态翻转时由 Form 自动开关那一行，构造时先按初值对齐一次。
    Toggle {
        label: String,
        value: bool,
        enabled: bool,
        hint: String,
        hides: Option<usize>,
    },
}

impl Field {
    pub fn text(label: &str, value: impl Into<String>) -> Self {
        Self::Text {
            label: label.into(),
            edit: LineEdit::new(value),
            enabled: true,
            hidden: false,
        }
    }

    pub fn readonly(label: &str, value: impl Into<String>) -> Self {
        Self::Text {
            label: label.into(),
            edit: LineEdit::new(value),
            enabled: false,
            hidden: false,
        }
    }

    pub fn select(label: &str, options: Vec<String>, selected: usize, hint: &str) -> Self {
        Self::Select {
            label: label.into(),
            options,
            selected,
            hint: hint.into(),
        }
    }

    pub fn toggle(label: &str, value: bool, hint: &str) -> Self {
        Self::Toggle {
            label: label.into(),
            value,
            enabled: true,
            hint: hint.into(),
            hides: None,
        }
    }

    /// 只读勾选框：空格/←→ 不响应，保存逻辑也不该从这里取值。
    pub fn toggle_locked(label: &str, value: bool, hint: &str) -> Self {
        Self::Toggle {
            label: label.into(),
            value,
            enabled: false,
            hint: hint.into(),
            hides: None,
        }
    }

    /// 勾上这个框时，`index` 那一行直接不出现（勾选框与它控制的字段成对声明）。
    pub fn hides(mut self, index: usize) -> Self {
        if let Self::Toggle { hides, .. } = &mut self {
            *hides = Some(index);
        }
        self
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Text { label, .. } | Self::Select { label, .. } | Self::Toggle { label, .. } => {
                label
            }
        }
    }

    /// 这一行现在是否不显示（只有文本行会藏）。
    pub fn is_hidden(&self) -> bool {
        matches!(self, Self::Text { hidden: true, .. })
    }
}

pub enum FormEvent {
    None,
    Save,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub active: usize,
    pub error: Option<String>,
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>, active: usize) -> Self {
        let active = active.min(fields.len().saturating_sub(1));
        let mut form = Self {
            title: title.into(),
            fields,
            active,
            error: None,
        };
        // 先把「勾选框 → 它控制的那一行」对齐一次（初值就可能带出隐藏，如非模型的探活路径）
        form.sync_toggle_links();
        form
    }

    pub fn text(&self, i: usize) -> &str {
        match self.fields.get(i) {
            Some(Field::Text { edit, .. }) => &edit.value,
            _ => "",
        }
    }

    pub fn select_value(&self, i: usize) -> &str {
        match self.fields.get(i) {
            Some(Field::Select {
                options, selected, ..
            }) => options.get(*selected).map(String::as_str).unwrap_or(""),
            _ => "",
        }
    }

    /// 勾选框当前值（不是勾选框就返回 false）。
    pub fn toggle(&self, i: usize) -> bool {
        match self.fields.get(i) {
            Some(Field::Toggle { value, .. }) => *value,
            _ => false,
        }
    }

    /// 直接摆一个文本行的值（快照/测试填充用）。只换值，不动这一行的
    /// 可编辑/显示状态——`hidden`、`enabled` 与勾选框的联动照旧。
    pub fn set_text(&mut self, i: usize, value: impl Into<String>) {
        if let Some(Field::Text { edit, .. }) = self.fields.get_mut(i) {
            *edit = LineEdit::new(value);
        }
    }

    pub fn handle_paste(&mut self, text: &str) {
        if let Some(Field::Text {
            edit,
            enabled: true,
            hidden: false,
            ..
        }) = self.fields.get_mut(self.active)
        {
            edit.insert(text);
            self.error = None;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> FormEvent {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return FormEvent::None;
        }
        match key.code {
            KeyCode::Esc => FormEvent::Cancel,
            KeyCode::Enter => FormEvent::Save,
            KeyCode::Tab | KeyCode::Down => {
                self.cycle(1);
                FormEvent::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.cycle(-1);
                FormEvent::None
            }
            KeyCode::Left => {
                self.on_left();
                FormEvent::None
            }
            KeyCode::Right => {
                self.on_right();
                FormEvent::None
            }
            KeyCode::Home => self.with_edit(LineEdit::home),
            KeyCode::End => self.with_edit(LineEdit::end),
            KeyCode::Backspace => self.with_edit(LineEdit::backspace),
            KeyCode::Delete => self.with_edit(LineEdit::delete),
            // 空格在勾选框上是「切换」，在文本框里是一个空格（名称可以带空格）
            KeyCode::Char(' ') if self.toggle_at(self.active).is_some() => {
                self.flip_toggle(self.active);
                FormEvent::None
            }
            KeyCode::Char(c) => self.with_edit(move |e| e.insert(&c.to_string())),
            _ => FormEvent::None,
        }
    }

    fn with_edit(&mut self, f: impl FnOnce(&mut LineEdit)) -> FormEvent {
        if let Some(Field::Text {
            edit,
            enabled: true,
            hidden: false,
            ..
        }) = self.fields.get_mut(self.active)
        {
            f(edit);
            self.error = None;
        }
        FormEvent::None
    }

    /// Tab/↑↓：跳过不显示的行（勾选框收起的那一行不参与跳转）。
    fn cycle(&mut self, dir: i32) {
        let n = self.fields.len();
        if n == 0 {
            return;
        }
        let mut i = self.active;
        for _ in 0..n {
            i = (i as i32 + dir).rem_euclid(n as i32) as usize;
            if !self.fields[i].is_hidden() {
                self.active = i;
                return;
            }
        }
    }

    fn on_left(&mut self) {
        match self.fields.get_mut(self.active) {
            Some(Field::Select {
                options, selected, ..
            }) => {
                if options.len() > 1 {
                    if *selected == 0 {
                        *selected = options.len() - 1;
                    } else {
                        *selected -= 1;
                    }
                }
            }
            Some(Field::Toggle { .. }) => self.flip_toggle(self.active),
            Some(Field::Text { edit, .. }) => edit.left(),
            None => {}
        }
    }

    fn on_right(&mut self) {
        match self.fields.get_mut(self.active) {
            Some(Field::Select {
                options, selected, ..
            }) => {
                if options.len() > 1 {
                    *selected = (*selected + 1) % options.len();
                }
            }
            Some(Field::Toggle { .. }) => self.flip_toggle(self.active),
            Some(Field::Text { edit, .. }) => edit.right(),
            None => {}
        }
    }

    /// 勾选框的「是否是勾选框」判断与翻转（只读的不响应）。
    fn toggle_at(&self, i: usize) -> Option<bool> {
        match self.fields.get(i) {
            Some(Field::Toggle { value, enabled, .. }) if *enabled => Some(*value),
            _ => None,
        }
    }

    fn flip_toggle(&mut self, i: usize) {
        if self.toggle_at(i).is_none() {
            return;
        }
        if let Some(Field::Toggle { value, .. }) = self.fields.get_mut(i) {
            *value = !*value;
            self.error = None;
        }
        self.sync_toggle_links();
        // 当前行可能刚被自己藏掉（理论上只有勾选框能藏别的行，防一手）
        if self.fields.get(self.active).is_some_and(Field::is_hidden) {
            self.cycle(1);
        }
    }

    /// 把每个勾选框的「勾上就藏起来的那一行」按当前值对齐。
    fn sync_toggle_links(&mut self) {
        let hides: Vec<(usize, bool)> = self
            .fields
            .iter()
            .filter_map(|field| match field {
                Field::Toggle {
                    hides: Some(target),
                    value,
                    ..
                } => Some((*target, *value)),
                _ => None,
            })
            .collect();
        for (target, hide) in hides {
            if let Some(Field::Text { hidden, .. }) = self.fields.get_mut(target) {
                *hidden = hide;
            }
        }
    }
}

/// 密钥表单：厂商 / 别名 / 分组 / 密钥
pub fn key_add(provider_options: Vec<String>, selected: usize) -> Form {
    Form::new(
        "添加密钥",
        vec![
            Field::select("厂商", provider_options, selected, "←/→ 切换"),
            Field::text("别名", ""),
            Field::text("分组", ""),
            Field::text("密钥", ""),
        ],
        1,
    )
}

pub fn key_edit(
    provider_options: Vec<String>,
    selected: usize,
    alias: &str,
    group: &str,
    token: &str,
) -> Form {
    Form::new(
        "编辑密钥",
        vec![
            Field::select("厂商", provider_options, selected, "←/→ 切换"),
            Field::text("别名", alias),
            Field::text("分组", group),
            Field::text("密钥", token),
        ],
        1,
    )
}

/// 厂商表单字段下标。
pub const PF_ID: usize = 0;
pub const PF_NAME: usize = 1;
pub const PF_BASE: usize = 2;
pub const PF_HOMEPAGE: usize = 3;
pub const PF_NON_MODEL: usize = 4;
pub const PF_HEALTH: usize = 5;
pub const PF_SCRIPT: usize = 6;

/// 厂商表单：ID / 名称 / Base URL / 主页 URL / 非模型 / 探活路径 / 脚本路径。
/// 主页 URL 一般填该厂商的控制面板，TUI 选中厂商按 Enter 用默认浏览器打开。
/// 额度查询只走脚本；声明式 http recipe 属于手写 YAML 的地盘（内置 DeepSeek、
/// new-api 系），表单不再提供预设类型。
/// 「非模型」勾选框：勾上 = 没有模型列表、不探活、不能一键导入客户端，
/// 并且「探活路径」那一行当场消失（不是灰掉——非模型没有这个参数）。
/// 「非模型」勾选框的提示文案（添加表单里空格键切换它）。
const NON_MODEL_HINT: &str = "空格切换（无模型列表、不探活）";

/// 添加表单的默认类型 = 当前分页（在非模型页按 a，多半就是要加非模型厂商）。
/// 无论如何勾选框都看得见，想反着来就空格取消。
/// 勾上后「探活路径」那一行直接不出现（`hides` 声明了联动，Form 自己维护）。
pub fn provider_add(initial_kind: crate::recipe::ProviderKind) -> Form {
    Form::new(
        "添加厂商",
        vec![
            Field::text("ID", ""),
            Field::text("名称", ""),
            Field::text("Base URL", ""),
            Field::text("主页 URL", ""),
            Field::toggle("非模型", !initial_kind.is_model(), NON_MODEL_HINT).hides(PF_HEALTH),
            Field::text("探活路径", "/models"),
            Field::text("脚本路径", ""),
        ],
        PF_ID,
    )
}

pub fn provider_edit(recipe: &crate::recipe::Recipe, health_path: &str) -> Form {
    let script_cmd = recipe
        .balance
        .as_ref()
        .and_then(|s| s.command.clone())
        .unwrap_or_default();
    // 类型创建后不可改：只读展示（保存逻辑同样不读这个勾选框）；非模型时
    // 它把探活路径那一行一起收起来（编辑表单里也不需要那个不存在的参数）
    let kind = Field::toggle_locked("非模型", !recipe.is_model(), "创建后不可改").hides(PF_HEALTH);
    Form::new(
        format!("编辑厂商 · {}", recipe.id),
        vec![
            Field::readonly("ID", recipe.id.clone()),
            Field::text("名称", recipe.name.clone()),
            Field::text("Base URL", recipe.base_url.clone()),
            Field::text("主页 URL", recipe.homepage.clone().unwrap_or_default()),
            kind,
            Field::text("探活路径", health_path),
            Field::text("脚本路径", script_cmd),
        ],
        PF_NAME,
    )
}

#[cfg(test)]
mod tests;
