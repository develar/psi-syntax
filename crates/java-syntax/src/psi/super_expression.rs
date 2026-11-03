//! `PsiSuperExpressionImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiSuperExpressionImpl.java`)
//! over a `SUPER_EXPRESSION`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getQualifier()` (`PsiSuperExpressionImpl.java:36`, `findChildByRole(QUALIFIER)` `:113`): the first child when it
/// is a `JAVA_CODE_REFERENCE`, the `Iface` of `Iface.super`; `None` for a bare `super`.
pub fn get_qualifier(tree: &Tree, sup: NodeId) -> Option<NodeId> {
    tree.first_child(sup).filter(|&c| tree.node_kind(c) == Some(NodeKind::JAVA_CODE_REFERENCE))
}
