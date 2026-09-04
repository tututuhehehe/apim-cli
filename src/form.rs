use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::KeyEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Provider,
    Alias,
    Group,
    Token,
}

impl FormField {
    fn next(self) -> Self {
        match self {
            Self::Provider => Self::Alias,
            Self::Alias => Self::Group,
            Self::Group => Self::Token,
            Self::Token => Self::Provider,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Provider => Self::Token,
            Self::Alias => Self::Provider,
            Self::Group => Self::Alias,
            Self::Token => Self::Group,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LineEdit {
    pub value: String,
    pub cursor: usize,
}

impl LineEdit {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self { value, cursor }
    }

    pub fn empty() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
        }
    }

    fn chars(&self) -> Vec<char> {
        self.value.chars().collect()
    }

    fn set_chars(&mut self, chars: Vec<char>, cursor: usize) {
        self.cursor = cursor.min(chars.len());
        self.value = chars.into_iter().collect();
    }

    pub fn insert(&mut self, text: &str) {
        let extra: Vec<char> = text
            .chars()
            .filter(|c| !matches!(c, '\n' | '\r' | '\t'))
            .collect();
        if extra.is_empty() {
            return;
        }
        let mut chars = self.chars();
        let i = self.cursor.min(chars.len());
        chars.splice(i..i, extra.iter().copied());
        self.set_chars(chars, i + extra.len());
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut chars = self.chars();
        chars.remove(self.cursor - 1);
        self.set_chars(chars, self.cursor - 1);
    }

    pub fn delete(&mut self) {
        let mut chars = self.chars();
        if self.cursor < chars.len() {
            chars.remove(self.cursor);
            self.set_chars(chars, self.cursor);
        }
    }

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn right(&mut self) {
        let n = self.value.chars().count();
        if self.cursor < n {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.value.chars().count();
    }
}

#[derive(Debug, Clone)]
pub enum FormMode {
    Add,
    Edit { original_id: String },
}

#[derive(Debug, Clone)]
pub struct KeyForm {
    pub mode: FormMode,
    pub provider_ids: Vec<String>,
    pub provider_idx: usize,
    pub alias: LineEdit,
    pub group: LineEdit,
    pub token: LineEdit,
    pub field: FormField,
    pub error: Option<String>,
}

pub enum FormEvent {
    None,
    Save,
    Cancel,
}

impl KeyForm {
    pub fn add(provider_ids: Vec<String>, provider_idx: usize) -> Self {
        Self {
            mode: FormMode::Add,
            provider_ids,
            provider_idx,
            alias: LineEdit::empty(),
            group: LineEdit::empty(),
            token: LineEdit::empty(),
            field: FormField::Alias,
            error: None,
        }
    }

    pub fn edit(provider_ids: Vec<String>, key: &KeyEntry) -> Self {
        let provider_idx = provider_ids
            .iter()
            .position(|id| id == &key.provider)
            .unwrap_or(0);
        Self {
            mode: FormMode::Edit {
                original_id: key.id(),
            },
            provider_ids,
            provider_idx,
            alias: LineEdit::new(key.alias.clone()),
            group: LineEdit::new(key.group.clone().unwrap_or_default()),
            token: LineEdit::new(key.token.clone()),
            field: FormField::Alias,
            error: None,
        }
    }

    pub fn title(&self) -> &'static str {
        match self.mode {
            FormMode::Add => "添加密钥",
            FormMode::Edit { .. } => "编辑密钥",
        }
    }

    pub fn provider_id(&self) -> Option<&str> {
        self.provider_ids.get(self.provider_idx).map(String::as_str)
    }

    pub fn handle_paste(&mut self, text: &str) {
        self.active_edit().insert(text);
        self.error = None;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> FormEvent {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return FormEvent::None;
        }
        match key.code {
            KeyCode::Esc => FormEvent::Cancel,
            KeyCode::Enter => FormEvent::Save,
            KeyCode::Tab => {
                self.field = self.field.next();
                FormEvent::None
            }
            KeyCode::BackTab => {
                self.field = self.field.prev();
                FormEvent::None
            }
            KeyCode::Left => {
                self.left();
                FormEvent::None
            }
            KeyCode::Right => {
                self.right();
                FormEvent::None
            }
            KeyCode::Home => {
                if self.field != FormField::Provider {
                    self.active_edit().home();
                }
                FormEvent::None
            }
            KeyCode::End => {
                if self.field != FormField::Provider {
                    self.active_edit().end();
                }
                FormEvent::None
            }
            KeyCode::Backspace => {
                if self.field != FormField::Provider {
                    self.active_edit().backspace();
                    self.error = None;
                }
                FormEvent::None
            }
            KeyCode::Delete => {
                if self.field != FormField::Provider {
                    self.active_edit().delete();
                    self.error = None;
                }
                FormEvent::None
            }
            KeyCode::Char(c) => {
                if self.field != FormField::Provider {
                    self.active_edit().insert(&c.to_string());
                    self.error = None;
                }
                FormEvent::None
            }
            _ => FormEvent::None,
        }
    }

    fn left(&mut self) {
        match self.field {
            FormField::Provider => {
                if !self.provider_ids.is_empty() {
                    if self.provider_idx == 0 {
                        self.provider_idx = self.provider_ids.len() - 1;
                    } else {
                        self.provider_idx -= 1;
                    }
                }
            }
            _ => self.active_edit().left(),
        }
    }

    fn right(&mut self) {
        match self.field {
            FormField::Provider => {
                if !self.provider_ids.is_empty() {
                    self.provider_idx = (self.provider_idx + 1) % self.provider_ids.len();
                }
            }
            _ => self.active_edit().right(),
        }
    }

    fn active_edit(&mut self) -> &mut LineEdit {
        match self.field {
            FormField::Provider | FormField::Alias => &mut self.alias,
            FormField::Group => &mut self.group,
            FormField::Token => &mut self.token,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace_keep_cursor() {
        let mut edit = LineEdit::empty();
        edit.insert("home");
        assert_eq!(edit.value, "home");
        assert_eq!(edit.cursor, 4);
        edit.left();
        edit.backspace();
        assert_eq!(edit.value, "hoe");
        assert_eq!(edit.cursor, 2);
    }
}
