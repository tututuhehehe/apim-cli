//! 跨模块的小工具。只放「同一件事在多个模块各写了一遍」的东西，别往这里加业务逻辑。

/// 把字符串截到 `max` 个**字符**（不是字节），超出加省略号，并把换行压成空格。
///
/// 给用户看的错误/输出摘要用：`probe`（探活与模型列表的错误体）与
/// `clients::codex::catalog`（codex 的 stderr）各写过一份，语义还不一样 ——
/// 这里统一到更严的那种（去首尾空白 + 压换行 + 按字符截断），
/// 两个调用点都是「一行提示」，压缩换行只会更好读。
pub(crate) fn truncate(text: &str, max: usize) -> String {
    let flattened = text.replace('\n', " ");
    let trimmed = flattened.trim();
    let mut out: String = trimmed.chars().take(max).collect();
    if trimmed.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_counts_chars_not_bytes_and_flattens_newlines() {
        assert_eq!(truncate("  hi  ", 10), "hi");
        // 按字符算：不会把多字节字符切一半
        assert_eq!(truncate("中文中文", 2), "中文…");
        assert_eq!(truncate("中文中文", 4), "中文中文");
        // 多行的 HTTP/错误体压成一行（两个调用点都是一行提示）
        assert_eq!(truncate("a\nb\n", 10), "a b");
        assert_eq!(truncate("a\nb", 2), "a …");
    }
}
