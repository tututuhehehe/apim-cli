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

/// 厂商表单：ID / 名称 / Base URL / 探活路径 / 额度路径 / 额度取值路径
pub fn provider_add() -> Form {
    Form::new(
        "添加厂商",
        vec![
            Field::text("ID", ""),
            Field::text("名称", ""),
            Field::text("Base URL", ""),
            Field::text("探活路径", "/models"),
            Field::text("额度路径", ""),
            Field::text("额度取值", ""),
        ],
        0,
    )
}

pub fn provider_edit(
    id: &str,
    name: &str,
    base_url: &str,
    health_path: &str,
    balance_path: &str,
    balance_json: &str,
) -> Form {
    Form::new(
        format!("编辑厂商 · {id}"),
        vec![
            Field::readonly("ID", id),
            Field::text("名称", name),
            Field::text("Base URL", base_url),
            Field::text("探活路径", health_path),
            Field::text("额度路径", balance_path),
            Field::text("额度取值", balance_json),
        ],
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_form() -> Form {
        key_add(vec!["deepseek".into(), "relay".into()], 0)
    }

    #[test]
    fn tab_cycles_and_left_changes_select() {
        let mut form = sample_form();
        assert_eq!(form.active, 1);
        form.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(form.active, 2);
        form.handle_key(KeyEvent::from(KeyCode::BackTab));
        form.handle_key(KeyEvent::from(KeyCode::BackTab));
        assert_eq!(form.active, 0);
        form.handle_key(KeyEvent::from(KeyCode::Left));
        assert_eq!(form.select_value(0), "relay");
    }

    #[test]
    fn readonly_field_ignores_input() {
        let mut form = provider_edit("deepseek", "DeepSeek", "https://x", "", "", "");
        form.active = 0;
        form.handle_key(KeyEvent::from(KeyCode::Char('z')));
        assert_eq!(form.text(0), "deepseek");
    }

    #[test]
    fn enter_saves_and_esc_cancels() {
        let mut form = sample_form();
        assert!(matches!(
            form.handle_key(KeyEvent::from(KeyCode::Enter)),
            FormEvent::Save
        ));
        assert!(matches!(
            form.handle_key(KeyEvent::from(KeyCode::Esc)),
            FormEvent::Cancel
        ));
    }
}
