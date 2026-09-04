//! 单行文本编辑：值 + 光标位置，字符级操作。

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace_keep_cursor() {
        let mut edit = LineEdit::new("");
        edit.insert("home");
        assert_eq!(edit.value, "home");
        assert_eq!(edit.cursor, 4);
        edit.left();
        edit.backspace();
        assert_eq!(edit.value, "hoe");
        assert_eq!(edit.cursor, 2);
    }

    #[test]
    fn paste_strips_newlines() {
        let mut edit = LineEdit::new("");
        edit.insert("sk-abc\r\ndef");
        assert_eq!(edit.value, "sk-abcdef");
    }
}
