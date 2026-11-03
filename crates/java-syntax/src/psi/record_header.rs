//! `PsiRecordHeaderImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiRecordHeaderImpl.java`) over a
//! `RECORD_HEADER`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getRecordComponents()` (`PsiRecordHeaderImpl.java:48`): the `RECORD_COMPONENT` children, in order.
pub fn get_record_components(tree: &Tree, header: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_kind(tree, header, NodeKind::RECORD_COMPONENT)
}
