//! `PsiThisExpressionImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiThisExpressionImpl.java`)
//! over a `THIS_EXPRESSION`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getQualifier()` (`PsiThisExpressionImpl.java:47`, `findChildByRole(QUALIFIER)` `:79`): the first child when it is
/// a `JAVA_CODE_REFERENCE`, the `Outer` of `Outer.this`; `None` for a bare `this`.
pub fn get_qualifier(tree: &Tree, this: NodeId) -> Option<NodeId> {
    tree.first_child(this).filter(|&c| tree.node_kind(c) == Some(NodeKind::JAVA_CODE_REFERENCE))
}
