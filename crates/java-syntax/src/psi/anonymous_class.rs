//! `PsiAnonymousClassImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiAnonymousClassImpl.java`) with the
//! child roles of `AnonymousClassElementBase` (`.../source/tree/java/AnonymousClassElementBase.java`), over an
//! `ANONYMOUS_CLASS`: its first child is the base class reference, then the arguments, then `{`. What it shares with
//! a named class (and overrides as `false` or `null`) is in `class`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getArgumentList()` (`PsiAnonymousClassImpl.java:63`, `findChildByRole(ARGUMENT_LIST)`
/// `AnonymousClassElementBase.java:43`): the `EXPRESSION_LIST` child.
pub fn get_argument_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    super::find_child_by_type(tree, class, NodeKind::EXPRESSION_LIST)
}

/// `getBaseClassReference()` (`PsiAnonymousClassImpl.java:68`): the first child when it is a `JAVA_CODE_REFERENCE`
/// (IntelliJ asserts it is). An enum constant's body has a light reference to its enum instead
/// (`PsiEnumConstantInitializerImpl.java:64`): `None`.
pub fn get_base_class_reference(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if tree.node_kind(class) != Some(NodeKind::ANONYMOUS_CLASS) {
        return None;
    }
    tree.first_child(class).filter(|&c| tree.node_kind(c) == Some(NodeKind::JAVA_CODE_REFERENCE))
}
