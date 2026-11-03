//! `JavaSharedImplUtil` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/JavaSharedImplUtil.java`): the
//! C-style array brackets after a variable's name or a method's parameter list.

use super::is_whitespace_or_comment;
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `collectAnnotations(anchor, null)` (`JavaSharedImplUtil.java:115`): from the sibling after the anchor (a
/// variable's name identifier, a method's parameter list) on, past whitespace, comments and annotations, each `[`
/// (a `]` is skipped), up to anything else (`=`, `;`, a throws list). IntelliJ returns the annotations before each
/// bracket, the last bracket first; this yields the `[` leaves in document order, the annotations of each being the
/// `ANNOTATION` siblings between it and the bracket before. `JavaSharedImplUtil.getType` (`:75`) makes the type an
/// array of one more dimension for each.
pub fn collect_annotations(tree: &Tree, anchor: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let mut next = tree.next_sibling(anchor);
    std::iter::from_fn(move || {
        while let Some(child) = next {
            next = tree.next_sibling(child);
            if is_whitespace_or_comment(tree, child) || tree.node_kind(child) == Some(NodeKind::ANNOTATION) {
                continue;
            }
            match tree.token_kind(child) {
                Some(TokenKind::LBRACKET) => return Some(child),
                Some(TokenKind::RBRACKET) => continue,
                _ => {
                    next = None;
                    return None;
                }
            }
        }
        None
    })
}

/// The number of brackets `collect_annotations` finds after the anchor: what `getType` adds to the type element's
/// dimensions (`int a[][]` has 2).
pub fn c_style_dimensions(tree: &Tree, anchor: NodeId) -> u32 {
    collect_annotations(tree, anchor).count() as u32
}
