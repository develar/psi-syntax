//! `PsiTypeCastExpressionImpl`
//! (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiTypeCastExpressionImpl.java`) over a
//! `TYPE_CAST_EXPRESSION`. An intersection cast `(A & B) x` has one `TYPE` over the `TYPE` of each bound.

use super::{find_child_by_set, find_child_by_type};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tree::{NodeId, Tree};

/// `getCastType()` (`PsiTypeCastExpressionImpl.java:28`, `findChildByRole(TYPE)` `:51`): the `TYPE` child.
pub fn get_cast_type(tree: &Tree, cast: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, cast, NodeKind::TYPE)
}

/// `getOperand()` (`PsiTypeCastExpressionImpl.java:33`, `findChildByRole(OPERAND)` `:57`): the first child in
/// `EXPRESSION_BIT_SET` (an error element is none).
pub fn get_operand(tree: &Tree, cast: NodeId) -> Option<NodeId> {
    find_child_by_set(tree, cast, EXPRESSION_BIT_SET)
}
