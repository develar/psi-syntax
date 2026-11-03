//! `PsiInstanceOfExpressionImpl`
//! (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiInstanceOfExpressionImpl.java`) over an
//! `INSTANCE_OF_EXPRESSION`: `x instanceof T`, `x instanceof T t`, `x instanceof R(var a)`.

use super::{find_child_by_set, find_child_by_type};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tree::{NodeId, Tree};

/// `getOperand()` (`PsiInstanceOfExpressionImpl.java:36`, `findChildByRole(OPERAND)` `:54`): the first child in
/// `EXPRESSION_BIT_SET`.
pub fn get_operand(tree: &Tree, instance_of: NodeId) -> Option<NodeId> {
    find_child_by_set(tree, instance_of, EXPRESSION_BIT_SET)
}

/// `getCheckType()` (`PsiInstanceOfExpressionImpl.java:41`, `findChildByRole(TYPE)` `:60`): the `TYPE` child (none
/// when the right side is a pattern).
pub fn get_check_type(tree: &Tree, instance_of: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, instance_of, NodeKind::TYPE)
}

/// `getPattern()` (`PsiInstanceOfExpressionImpl.java:85`): the first `PsiPrimaryPattern` child, a `TYPE_TEST_PATTERN`,
/// a `DECONSTRUCTION_PATTERN` or an `UNNAMED_PATTERN`.
pub fn get_pattern(tree: &Tree, instance_of: NodeId) -> Option<NodeId> {
    tree.children(instance_of).find(|&c| {
        matches!(
            tree.node_kind(c),
            Some(NodeKind::TYPE_TEST_PATTERN | NodeKind::DECONSTRUCTION_PATTERN | NodeKind::UNNAMED_PATTERN)
        )
    })
}
