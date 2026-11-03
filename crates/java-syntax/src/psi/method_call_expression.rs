//! `PsiMethodCallExpressionImpl`
//! (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiMethodCallExpressionImpl.java`) over a
//! `METHOD_CALL_EXPRESSION`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getMethodExpression()` (`PsiMethodCallExpressionImpl.java:99`, `findChildByRole(METHOD_EXPRESSION)` `:116`): the
/// first child, a `REFERENCE_EXPRESSION` (`None` when it is not, where IntelliJ's cast throws).
pub fn get_method_expression(tree: &Tree, call: NodeId) -> Option<NodeId> {
    tree.first_child(call).filter(|&c| tree.node_kind(c) == Some(NodeKind::REFERENCE_EXPRESSION))
}

/// `getArgumentList()` (`PsiMethodCallExpressionImpl.java:104`, `findChildByRole(ARGUMENT_LIST)` `:119`): the
/// `EXPRESSION_LIST` child.
pub fn get_argument_list(tree: &Tree, call: NodeId) -> Option<NodeId> {
    super::find_child_by_type(tree, call, NodeKind::EXPRESSION_LIST)
}
