//! `PsiParameterListImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiParameterListImpl.java`) over a
//! `PARAMETER_LIST` (of a method or a lambda).

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getParameters()` (`PsiParameterListImpl.java:32`): the `PARAMETER` children, in order.
pub fn get_parameters(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_kind(tree, list, NodeKind::PARAMETER)
}
