//! A file's text as IntelliJ's document holds it, at the file's own offsets, and the lines of a text. IntelliJ reads a
//! file without its UTF-8 byte order mark (`CharsetToolkit`) and with `\n` line ends (`convertLineSeparators`); the
//! lexers here read the bytes they are given, so a mark is a bad character to them and the declaration after it
//! misplaced (Kotlin's `package`, Java's "'class' or 'interface' expected"). A language whose lexer also misreads a
//! `\r` rewrites it in its own `document_text` (`kotlin_syntax::document_text`); both keep every offset, so every range
//! of the tree is a range of the file.

/// The UTF-8 byte order mark.
pub const BYTE_ORDER_MARK: &[u8] = b"\xEF\xBB\xBF";

/// Does `text` open with a UTF-8 byte order mark?
pub fn has_byte_order_mark(text: &[u8]) -> bool {
    text.starts_with(BYTE_ORDER_MARK)
}

/// The mark at the start of `text`, if any, as three spaces: what IntelliJ reads there, at the same offsets.
pub fn blank_byte_order_mark(text: &mut [u8]) {
    if has_byte_order_mark(text) {
        text[..BYTE_ORDER_MARK.len()].fill(b' ');
    }
}

/// The column of the byte at `offset`: the UTF-16 code units of its line before it, as an editor and the Language
/// Server Protocol count them. The line starts after the last `\n`, as `Lines` ends them, so the `\r` of a CRLF end is
/// a unit of the line it ends. Bytes that are no UTF-8 count as the replacement character a lossy read shows.
pub fn column(text: &[u8], offset: usize) -> u32 {
    let offset = offset.min(text.len());
    let start = text[..offset].iter().rposition(|&byte| byte == b'\n').map_or(0, |it| it + 1);
    let before = String::from_utf8_lossy(&text[start..offset]);
    u32::try_from(before.encode_utf16().count()).unwrap_or(u32::MAX)
}

/// The line of each byte offset of a text, from 1, by its `\n`s (a `\r` ends no line here: give it the document text
/// for that). A byte past the text is on its last line.
pub struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    /// A plain scan, no `memchr`: on the 173 MB of the community Kotlin files it costs 25 ms more than `memchr_iter`
    /// (80 against 55 ms), about 1 % of their parse, as pushing the line starts dominates both.
    pub fn new(text: &[u8]) -> Lines {
        let mut starts = vec![0];
        starts.extend(text.iter().enumerate().filter(|&(_, &b)| b == b'\n').map(|(i, _)| i + 1));
        Lines { starts }
    }

    /// The line of the byte at `offset`, from 1.
    pub fn line(&self, offset: usize) -> u32 {
        self.starts.partition_point(|&start| start <= offset) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blanked_mark_keeps_the_offsets() {
        let mut text = b"\xEF\xBB\xBFpackage a".to_vec();
        assert!(has_byte_order_mark(&text));
        blank_byte_order_mark(&mut text);
        assert_eq!(text, b"   package a");
        assert!(!has_byte_order_mark(&text));
        let mut short = b"\xEF\xBB".to_vec();
        blank_byte_order_mark(&mut short);
        assert_eq!(short, b"\xEF\xBB");
    }

    #[test]
    fn a_column_counts_the_utf16_units_of_its_line_before_it() {
        let text = "let \u{1F600} = 1;\r\nlet b = 2;".as_bytes();
        assert_eq!(column(text, 0), 0);
        // the emoji is one character and two UTF-16 units
        assert_eq!(column(text, text.iter().position(|&it| it == b'=').unwrap()), 7);
        // the `\r` is a unit of the line it ends
        assert_eq!(column(text, text.len() - "let b = 2;".len()), 0);
        assert_eq!(column(text, text.len() - 1), 9);
        // past the text, and a byte that is no UTF-8
        assert_eq!(column(text, text.len() + 10), 10);
        assert_eq!(column(b"a\xFFb", 3), 3);
    }

    #[test]
    fn lines_count_from_one_by_line_feeds() {
        let lines = Lines::new(b"a\nbc\r\n\nd");
        assert_eq!([0, 1, 2, 4, 5, 6, 7, 8, 100].map(|offset| lines.line(offset)), [1, 1, 2, 2, 2, 3, 4, 4, 4]);
        assert_eq!(Lines::new(b"").line(0), 1);
        assert_eq!(Lines::new(b"a\n").line(2), 2);
    }
}
