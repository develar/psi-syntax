//! `DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false).trim()`: the PSI debug dump the Kotlin
//! parsing tests compare against.
//!
//! Each node on its own line, indented two spaces per depth: a composite prints its element type (`KtFile: <name>` for
//! the file, `PsiErrorElement:<message>` for an error) and `<empty list>` below it when it has no children; a leaf
//! prints `PsiElement(<type>)('<text>')`, `PsiWhiteSpace('<text>')` or `PsiComment(<type>)('<text>')`. In texts only
//! `\n`, `\r` and `\t` are escaped (`DebugUtil.fixWhiteSpaces`).

use crate::node_kind::NodeKind;
use crate::tokens::{ElementType, TokenKind};
use crate::tree::{NodeId, Tree};

/// The dump of `tree` as the file `file_name`.
pub fn psi_to_string(tree: &Tree, file_name: &str) -> String {
    subtree(tree, tree.root(), file_name)
}

/// The dump of the subtree at `node`, indented from 0 (`DebugUtil.psiToString(element, true, false).trim()`).
pub fn subtree_to_string(tree: &Tree, node: NodeId) -> String {
    subtree(tree, node, "")
}

fn subtree(tree: &Tree, node: NodeId, file_name: &str) -> String {
    let mut out = String::with_capacity(tree.range(node).len() * 4 + 64);
    for (id, depth) in tree.preorder(node) {
        push_indent(&mut out, depth);
        let kind = tree.kind(id);
        if tree.is_leaf(id) {
            match kind {
                ElementType::Token(TokenKind::WHITE_SPACE) => out.push_str("PsiWhiteSpace"),
                ElementType::Token(
                    t @ (TokenKind::EOL_COMMENT | TokenKind::BLOCK_COMMENT | TokenKind::SHEBANG_COMMENT),
                ) => {
                    out.push_str("PsiComment(");
                    out.push_str(t.debug_name());
                    out.push(')');
                }
                _ => {
                    out.push_str("PsiElement(");
                    out.push_str(kind.debug_name());
                    out.push(')');
                }
            }
            out.push_str("('");
            fix_white_spaces(&mut out, tree.text(id));
            out.push_str("')\n");
        } else {
            match kind {
                ElementType::Node(NodeKind::KT_FILE) if id == tree.root() => {
                    out.push_str("KtFile: ");
                    out.push_str(file_name);
                }
                ElementType::Node(NodeKind::ERROR_ELEMENT) => {
                    out.push_str("PsiErrorElement:");
                    out.push_str(tree.error_message(id).unwrap_or(""));
                }
                _ => out.push_str(kind.debug_name()),
            }
            out.push('\n');
            if tree.first_child(id).is_none() {
                push_indent(&mut out, depth + 1);
                out.push_str("<empty list>\n");
            }
        }
    }
    // String.trim(): the characters up to ' ' at both ends.
    let trimmed = out.trim_matches(|c: char| c <= ' ');
    if trimmed.len() != out.len() {
        out = trimmed.to_owned();
    }
    out
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// `DebugUtil.fixWhiteSpaces`: `\n`, `\r`, `\t` escaped; the text decoded as UTF-8 (an invalid byte is U+FFFD).
fn fix_white_spaces(out: &mut String, text: &[u8]) {
    for chunk in text.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => out.push(c),
            }
        }
        if !chunk.invalid().is_empty() {
            out.push('\u{FFFD}');
        }
    }
}
