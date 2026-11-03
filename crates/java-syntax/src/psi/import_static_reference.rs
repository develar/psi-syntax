//! `PsiImportStaticReferenceElementImpl`
//! (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiImportStaticReferenceElementImpl.java`) over an
//! `IMPORT_STATIC_REFERENCE`: the `a.B.c` of `import static a.B.c;`, its qualifier the class reference `a.B`, its name
//! the member `c`.

use super::find_child_by_type;
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getReferenceNameElement()` (`findChildByRole(REFERENCE_NAME)` `PsiImportStaticReferenceElementImpl.java:81`): the
/// first `IDENTIFIER` child.
pub fn get_reference_name_element(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, reference, TokenKind::IDENTIFIER)
}

/// `getClassReference()` (`PsiImportStaticReferenceElementImpl.java:135`, `findChildByRole(QUALIFIER)` `:84`): the
/// first child when it is a `JAVA_CODE_REFERENCE`. `getQualifier()` (`:130`) is the same node.
pub fn get_class_reference(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    tree.first_child(reference).filter(|&c| tree.node_kind(c) == Some(NodeKind::JAVA_CODE_REFERENCE))
}
