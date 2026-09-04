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
    },
    Select {
        label: String,
        options: Vec<String>,
        selected: usize,
        hint: String,
    },
}

impl Field {
    pub fn text(label: &str, value: impl Into<String>) -> Self {
        Self::Text {
            label: label.into(),
            edit: LineEdit::new(value),
            enabled: true,
        }
    }

    pub fn readonly(label: &str, value: impl Into<String>) -> Self {
        Self::Text {
            label: label.into(),
            edit: LineEdit::new(value),
            enabled: false,
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

    pub fn label(&self) -> &str {
        match self {
            Self::Text { label, .. } | Self::Select { label, .. } => label,
        }
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
        Self {
            title: title.into(),
            fields,
            active,
            error: None,
        }
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

    pub fn handle_paste(&mut self, text: &str) {
        if let Some(Field::Text { edit, enabled, .. }) = self.fields.get_mut(self.active)
            && *enabled
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
            KeyCode::Char(c) => self.with_edit(move |e| e.insert(&c.to_string())),
            _ => FormEvent::None,
        }
    }

    fn with_edit(&mut self, f: impl FnOnce(&mut LineEdit)) -> FormEvent {
        if let Some(Field::Text { edit, enabled, .. }) = self.fields.get_mut(self.active)
            && *enabled
        {
            f(edit);
            self.error = None;
        }
        FormEvent::None
    }

    fn cycle(&mut self, dir: i32) {
        let n = self.fields.len();
        if n == 0 {
            return;
        }
        let i = (self.active as i32 + dir).rem_euclid(n as i32);
        self.active = i as usize;
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
            Some(Field::Text { edit, .. }) => edit.right(),
            None => {}
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
pub const PF_HEALTH: usize = 4;
pub const PF_SCRIPT: usize = 5;

/// 厂商表单：ID / 名称 / Base URL / 主页 URL / 探活路径 / 脚本路径。
/// 主页 URL 一般填该厂商的控制面板，TUI 选中厂商按 Enter 用默认浏览器打开。
/// 额度查询只走脚本；声明式 http recipe 属于手写 YAML 的地盘（内置 DeepSeek、
/// new-api 系），表单不再提供预设类型。
pub fn provider_add() -> Form {
    Form::new(
        "添加厂商",
        vec![
            Field::text("ID", ""),
            Field::text("名称", ""),
            Field::text("Base URL", ""),
            Field::text("主页 URL", ""),
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
        .and_then(|b| b.script())
        .and_then(|s| s.command.clone())
        .unwrap_or_default();
    Form::new(
        format!("编辑厂商 · {}", recipe.id),
        vec![
            Field::readonly("ID", recipe.id.clone()),
            Field::text("名称", recipe.name.clone()),
            Field::text("Base URL", recipe.base_url.clone()),
            Field::text("主页 URL", recipe.homepage.clone().unwrap_or_default()),
            Field::text("探活路径", health_path),
            Field::text("脚本路径", script_cmd),
        ],
        PF_NAME,
    )
}

#[cfg(test)]
mod tests;
