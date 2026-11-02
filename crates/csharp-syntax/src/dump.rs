//! The dump `tools/csharp-oracle` prints of Roslyn's tree (its README, "The dump", is the normative format): one line
//! per node, token and trivia in `ChildNodesAndTokens()` order, two spaces per depth, UTF-8 byte offsets; a node
//! `Kind [full span)`, a token `Kind [span) 'text'` (`Kind [s..s) missing`), its trivia before and after it as
//! `L:Kind [s..e) 'text'` / `T:...`, a structured trivia `L:Kind [s..e)` with its structure's children one level deeper;
//! then `Diagnostics:` and `  CSnnnn [s..e) message` per diagnostic. Also the lexer dump of `ksdump --lex`
//! (`lex_to_string`) and the line of `index.jsonl` (`index_line`).

use crate::syntax::node_type;
use crate::syntax_kind::TokenKind;
use crate::tree::{NodeId, Tree};

/// The dump of a parsed tree, ending with one `\n`.
pub fn tree_to_string(tree: &Tree) -> String {
    let mut out = String::with_capacity(tree.source().len() * 8);
    for (id, depth) in tree.preorder(tree.root()) {
        line(tree, id, depth, &mut out);
    }
    diagnostics(tree, &mut out);
    out
}

/// The dump of a lexed text (`Parser::lex`): the tokens at depth 0 (their trivia beside them, a structured trivia's
/// children from depth 1), then the diagnostics.
pub fn lex_to_string(tree: &Tree) -> String {
    let mut out = String::with_capacity(tree.source().len() * 8);
    for (id, depth) in tree.preorder(tree.root()).skip(1) {
        line(tree, id, depth - 1, &mut out);
    }
    diagnostics(tree, &mut out);
    out
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
            escape(&String::from_utf8_lossy(tree.text(id)), out);
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
        let mut json = String::new();
        json_string("a\"\u{1}\\", &mut json);
        assert_eq!(json, "\"a\\\"\\u0001\\\\\"");
    }
}
