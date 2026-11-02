//! The dump `tools/csharp-oracle` prints of Roslyn's tree (its README, "The dump", is the normative format): one line
//! per node, token and trivia in `ChildNodesAndTokens()` order, two spaces per depth, UTF-8 byte offsets; a node
//! `Kind [full span)`, a token `Kind [span) 'text'` (`Kind [s..s) missing`), its trivia before and after it as
//! `L:Kind [s..e) 'text'` / `T:...`, a structured trivia `L:Kind [s..e)` with its structure's children one level deeper;
//! then `Diagnostics:` and `  CSnnnn [s..e) message` per diagnostic. Also the lexer dump of `ksdump --lex`
//! (`lex_to_string`) and the line of `index.jsonl` (`index_line`).

use crate::syntax::node_type;
use crate::syntax_kind::TokenKind;
use crate::tree::{NodeId, Tree};

/// The longest dump the oracle prints, in UTF-16 code units (`Printer.MaxChars`, 2^29): past it a file is a failure,
/// `dump over 512 MB`.
pub const MAX_CHARS: usize = 1 << 29;

/// The dump of a parsed tree, ending with one `\n`.
pub fn tree_to_string(tree: &Tree) -> String {
    tree_to_string_within(tree, usize::MAX).unwrap_or_default()
}

/// `tree_to_string`, or `None` when the dump passes `max_chars` UTF-16 code units before a line is printed, where the
/// oracle gives up (`dump over 512 MB` with `MAX_CHARS`).
pub fn tree_to_string_within(tree: &Tree, max_chars: usize) -> Option<String> {
    let mut out = String::with_capacity(tree.source().len() * 8);
    let mut chars = 0usize;
    for (id, depth) in tree.preorder(tree.root()) {
        if chars > max_chars {
            return None;
        }
        let before = out.len();
        line(tree, id, depth, &mut out);
        chars += utf16_len(&out[before..]);
    }
    diagnostics(tree, &mut out);
    Some(out)
}

/// The dump of a lexed text (`Parser::lex`): the tokens at depth 0 (their trivia beside them, a structured trivia's
/// children from depth 1), then the diagnostics.
pub fn lex_to_string(tree: &Tree) -> String {
    lex_to_string_within(tree, usize::MAX).unwrap_or_default()
}

/// `lex_to_string`, or `None` past `max_chars` as `tree_to_string_within`.
pub fn lex_to_string_within(tree: &Tree, max_chars: usize) -> Option<String> {
    let mut out = String::with_capacity(tree.source().len() * 8);
    let mut chars = 0usize;
    for (id, depth) in tree.preorder(tree.root()).skip(1) {
        if chars > max_chars {
            return None;
        }
        let before = out.len();
        line(tree, id, depth - 1, &mut out);
        chars += utf16_len(&out[before..]);
    }
    diagnostics(tree, &mut out);
    Some(out)
}

/// The length of `text` in UTF-16 code units.
fn utf16_len(text: &str) -> usize {
    text.bytes()
        .map(|b| {
            if b & 0xC0 == 0x80 {
                0
            } else if b >= 0xF0 {
                2
            } else {
                1
            }
        })
        .sum()
}

/// The number of tokens a lexed tree holds (the root's token children).
pub fn lexed_tokens(tree: &Tree) -> usize {
    tree.children(tree.root()).filter(|&c| tree.token_kind(c).is_some_and(|k| !is_trivia(k))).count()
}

/// Whether a leaf kind is an unstructured trivia (`L:`/`T:` lines).
pub fn is_trivia(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::EndOfLineTrivia
            | TokenKind::WhitespaceTrivia
            | TokenKind::SingleLineCommentTrivia
            | TokenKind::MultiLineCommentTrivia
            | TokenKind::DocumentationCommentExteriorTrivia
            | TokenKind::DisabledTextTrivia
            | TokenKind::PreprocessingMessageTrivia
            | TokenKind::ConflictMarkerTrivia
    )
}

fn line(tree: &Tree, id: NodeId, depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    let range = tree.range(id);
    let prefix = if tree.is_trailing(id) { "T:" } else { "L:" };
    if let Some(kind) = tree.token_kind(id) {
        if is_trivia(kind) {
            out.push_str(prefix);
        }
        out.push_str(kind.debug_name());
        span(range.start, range.end, out);
        if tree.is_missing(id) {
            out.push_str(" missing");
        } else {
            out.push_str(" '");
            escape_source(tree.source(), range.start, range.end, out);
            out.push('\'');
        }
    } else if let Some(kind) = tree.node_kind(id) {
        if node_type(kind).is_structured_trivia {
            out.push_str(prefix);
        }
        out.push_str(kind.debug_name());
        span(range.start, range.end, out);
    }
    out.push('\n');
}

fn span(start: usize, end: usize, out: &mut String) {
    out.push_str(" [");
    out.push_str(&start.to_string());
    out.push_str("..");
    out.push_str(&end.to_string());
    out.push(')');
}

fn diagnostics(tree: &Tree, out: &mut String) {
    if tree.diagnostics().is_empty() {
        return;
    }
    out.push_str("Diagnostics:\n");
    for d in tree.diagnostics() {
        out.push_str("  ");
        diagnostic_line(d, out);
        out.push('\n');
    }
}

/// `CSnnnn [s..e) message`, the message escaped.
fn diagnostic_line(d: &crate::tree::Diagnostic, out: &mut String) {
    out.push_str(&d.id());
    span(d.start as usize, d.end as usize, out);
    out.push(' ');
    escape(&d.message, out);
}

/// The text escapes: `\\ \' \n \r \t`, `\u{XXXX}` for the other controls below U+0020, U+007F, U+0085, U+2028, U+2029
/// and U+FEFF; any other char as itself.
pub fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || matches!(c, '\u{7F}' | '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{FEFF}') => {
                out.push_str(&format!("\\u{{{:04X}}}", c as u32));
            }
            c => out.push(c),
        }
    }
}

/// `escape` of the source's text `start..end`, whose ends may be inside a supplementary character (a token of half a
/// surrogate pair, as the lexer makes in a directive): such a half is a lone surrogate, `\u{XXXX}`.
pub fn escape_source(source: &[u8], start: usize, end: usize, out: &mut String) {
    let (mut start, mut end) = (start.min(source.len()), end.min(source.len()));
    let mut high = None;
    if start < end {
        // the text starts with the low half of the character 2 bytes before
        if let Some(cp) = start.checked_sub(2).and_then(|lead| four_byte_char(source, lead)) {
            out.push_str(&format!("\\u{{{:04X}}}", 0xDC00 + ((cp - 0x10000) & 0x3FF)));
            start += 2;
        }
        // it ends with the high half of a character that goes on 2 bytes after it
        if end >= start + 2
            && let Some(cp) = four_byte_char(source, end - 2)
        {
            high = Some(0xD800 + ((cp - 0x10000) >> 10));
            end -= 2;
        }
    }
    if start < end {
        escape(&String::from_utf8_lossy(&source[start..end]), out);
    }
    if let Some(high) = high {
        out.push_str(&format!("\\u{{{high:04X}}}"));
    }
}

/// The code point of a valid 4-byte UTF-8 sequence at `at`.
fn four_byte_char(source: &[u8], at: usize) -> Option<u32> {
    let bytes = source.get(at..at + 4)?;
    let text = std::str::from_utf8(bytes).ok()?;
    let c = text.chars().next()?;
    (c.len_utf8() == 4).then_some(c as u32)
}

/// The `index.jsonl` line of a dumped file: `{"path":…,"errors":N,"firstError":"CSnnnn [s..e) message"|null}`.
pub fn index_line(path: &str, tree: &Tree) -> String {
    let mut out = String::from("{\"path\":");
    json_string(path, &mut out);
    out.push_str(",\"errors\":");
    out.push_str(&tree.diagnostics().len().to_string());
    out.push_str(",\"firstError\":");
    match tree.diagnostics().first() {
        Some(d) => {
            let mut first = String::new();
            diagnostic_line(d, &mut first);
            json_string(&first, &mut out);
        }
        None => out.push_str("null"),
    }
    out.push('}');
    out
}

/// The `index.jsonl` line of a file that has no dump: `{"path":…,"failure":…}`.
pub fn failure_line(path: &str, failure: &str) -> String {
    let mut out = String::from("{\"path\":");
    json_string(path, &mut out);
    out.push_str(",\"failure\":");
    json_string(failure, &mut out);
    out.push('}');
    out
}

/// A JSON string as the oracle writes it: `\" \\ \n \r \t`, `\u00xx` below U+0020, anything else as itself.
fn json_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_as_the_oracle() {
        let mut out = String::new();
        escape("a'\\\n\r\t\u{0B}\u{7F}\u{85}\u{FEFF}é😀", &mut out);
        assert_eq!(out, "a\\'\\\\\\n\\r\\t\\u{000B}\\u{007F}\\u{0085}\\u{FEFF}é😀");
        let source = "a😀b".as_bytes();
        let mut halves = String::new();
        escape_source(source, 1, 3, &mut halves);
        halves.push('|');
        escape_source(source, 3, 6, &mut halves);
        assert_eq!(halves, "\\u{D83D}|\\u{DE00}b");
        let mut json = String::new();
        json_string("a\"\u{1}\\", &mut json);
        assert_eq!(json, "\"a\\\"\\u0001\\\\\"");
    }
}
