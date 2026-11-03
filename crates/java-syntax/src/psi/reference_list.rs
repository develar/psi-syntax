//! `PsiReferenceListImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiReferenceListImpl.java`) over an
//! `EXTENDS_LIST`, `IMPLEMENTS_LIST`, `PERMITS_LIST`, `THROWS_LIST`, `EXTENDS_BOUND_LIST` or `PROVIDES_WITH_LIST`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getReferenceElements()` (`PsiReferenceListImpl.java:27`): the `JAVA_CODE_REFERENCE` children, in order. A type
/// annotation (`extends @A Base`) is a child of its reference, not of the list.
pub fn get_reference_elements(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_kind(tree, list, NodeKind::JAVA_CODE_REFERENCE)
}
