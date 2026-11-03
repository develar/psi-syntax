//! `PsiLambdaExpressionImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiLambdaExpressionImpl.java`)
//! over a `LAMBDA_EXPRESSION`.

use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tree::{NodeId, Tree};

/// `getParameterList()` (`PsiLambdaExpressionImpl.java:81`): the `PARAMETER_LIST` child (`x -> x` has one too).
pub fn get_parameter_list(tree: &Tree, lambda: NodeId) -> Option<NodeId> {
    super::find_child_by_type(tree, lambda, NodeKind::PARAMETER_LIST)
}

/// `getBody()` (`PsiLambdaExpressionImpl.java:86`): the last child when it is an expression or a `CODE_BLOCK`.
pub fn get_body(tree: &Tree, lambda: NodeId) -> Option<NodeId> {
    super::last_child(tree, lambda)
        .filter(|&c| EXPRESSION_BIT_SET.contains(tree.kind(c)) || tree.node_kind(c) == Some(NodeKind::CODE_BLOCK))
}
