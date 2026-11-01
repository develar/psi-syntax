//! `DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false).trim()`: the PSI debug dump the Kotlin
//! parsing tests compare against, with Kotlin's labels over `psi_builder::dump`'s layout.
//!
//! A composite prints its element type (`KtFile: <name>` for the file, `PsiErrorElement:<message>` for an error); a
//! leaf prints `PsiElement(<type>)`, `PsiWhiteSpace` or `PsiComment(<type>)` before its text.

use psi_builder::dump::psi_to_string_with;

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
    psi_to_string_with(tree, node, |tree, id, out| label(tree, id, file_name, out))
}

/// What PSI's `toString()` prints for the node.
fn label(tree: &Tree, id: NodeId, file_name: &str, out: &mut String) {
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
    }
}
