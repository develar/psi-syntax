//! `DebugUtil.psiToString(file, showWhitespaces = true, showRanges = false).trim()` for Starlark.

use psi_builder::dump::psi_to_string_with;
use psi_builder::{ElementType, NodeId};

use crate::NodeKind;
use crate::TokenKind;
use crate::Tree;

pub fn psi_to_string(tree: &Tree) -> String {
    psi_to_string_with(tree, tree.root(), label)
}

fn label(tree: &Tree, id: NodeId, out: &mut String) {
    match tree.kind(id) {
        ElementType::Node(NodeKind::FILE) if id == tree.root() => out.push_str("FILE"),
        ElementType::Node(NodeKind::ERROR_ELEMENT) => {
            out.push_str("PsiErrorElement:");
            out.push_str(tree.error_message(id).unwrap_or(""));
        }
        ElementType::Node(kind) => {
            out.push_str(kind.psi_class());
            out.push('(');
            out.push_str(kind.debug_name());
            out.push(')');
            if kind.is_named() {
                out.push_str("('");
                out.push_str(named_element_name(tree, id).unwrap_or("null"));
                out.push_str("')");
            }
        }
        ElementType::Token(TokenKind::COMMENT) => out.push_str("PsiComment(Starlark:COMMENT)"),
        ElementType::Token(token) if token.is_whitespace() => out.push_str("PsiWhiteSpace"),
        ElementType::Token(token) => {
            out.push_str("PsiElement(Starlark:");
            out.push_str(token.debug_name());
            out.push(')');
        }
    }
}

fn named_element_name(tree: &Tree, id: NodeId) -> Option<&str> {
    let mut child = tree.first_child(id);
    while let Some(current) = child {
        if tree.kind(current) == ElementType::Token(TokenKind::IDENTIFIER) {
            return std::str::from_utf8(tree.text(current)).ok();
        }
        child = tree.next_sibling(current);
    }
    None
}
