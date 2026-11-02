//! The core of `DebugUtil.psiToString(element, showWhitespaces = true, showRanges = false).trim()`: the PSI debug
//! dump the parsing tests compare against. A language names its nodes (`label`); this writes the layout.
//!
//! Each node on its own line, indented two spaces per depth: a composite prints its label and `<empty list>` below it
//! when it has no children; a leaf prints its label and `('<text>')`. In texts only `\n`, `\r` and `\t` are escaped
//! (`DebugUtil.fixWhiteSpaces`). The whole dump is trimmed (`String.trim()`).

use crate::language::TreeLanguage;
use crate::tree::{NodeId, Tree};

/// The dump of the subtree at `node`, indented from 0. `label(tree, id, out)` appends what PSI's `toString()` prints
/// for the node: for Kotlin `PsiElement(<type>)`, `PsiWhiteSpace` or `PsiComment(<type>)` for a leaf, the element
/// type, `KtFile: <name>` or `PsiErrorElement:<message>` for a composite.
pub fn psi_to_string_with<L: TreeLanguage>(
    tree: &Tree<L>,
    node: NodeId,
    mut label: impl FnMut(&Tree<L>, NodeId, &mut String),
) -> String {
    let mut out = String::with_capacity(tree.range(node).len() * 4 + 64);
    for (id, depth) in tree.preorder(node) {
        push_indent(&mut out, depth);
        label(tree, id, &mut out);
        if tree.is_leaf(id) {
            out.push_str("('");
            fix_white_spaces(&mut out, tree.text(id));
            out.push_str("')\n");
        } else {
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
pub fn fix_white_spaces(out: &mut String, text: &[u8]) {
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
