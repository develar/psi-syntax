//! `PsiTypeParameterListImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiTypeParameterListImpl.java`)
//! over a `TYPE_PARAMETER_LIST`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getTypeParameters()` (`PsiTypeParameterListImpl.java:45`): the `TYPE_PARAMETER` children, in order.
pub fn get_type_parameters(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_kind(tree, list, NodeKind::TYPE_PARAMETER)
}
