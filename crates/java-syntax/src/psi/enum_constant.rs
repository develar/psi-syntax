//! `PsiEnumConstantImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiEnumConstantImpl.java`) with the
//! child roles of its `EnumConstantElement` (`.../source/tree/java/EnumConstantElement.java`), over an
//! `ENUM_CONSTANT`. Its modifiers are always `public static final` (`psi::has_modifier_property`); it has no type
//! element and no initializer (`PsiEnumConstantImpl.java:141`, `:146`).

use super::find_child_by_type;
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getArgumentList()` (`PsiEnumConstantImpl.java:83`, `EnumConstantElement.findChildByRole(ARGUMENT_LIST)`): the
/// `EXPRESSION_LIST` child (`A(1, 2)`; none for `A`).
pub fn get_argument_list(tree: &Tree, constant: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, constant, NodeKind::EXPRESSION_LIST)
}

/// `getInitializingClass()` (`PsiEnumConstantImpl.java:88`): the `ENUM_CONSTANT_INITIALIZER` child, the body of
/// `A { ... }`.
pub fn get_initializing_class(tree: &Tree, constant: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, constant, NodeKind::ENUM_CONSTANT_INITIALIZER)
}

/// `getNameIdentifier()` (`PsiEnumConstantImpl.java:185`, `EnumConstantElement.findChildByRole(NAME)`): the first
/// `IDENTIFIER` child.
pub fn get_name_identifier(tree: &Tree, constant: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, constant, TokenKind::IDENTIFIER)
}

/// `getContainingClass()` (`PsiEnumConstantImpl.java:111`): the parent when it is a class.
pub fn get_containing_class(tree: &Tree, constant: NodeId) -> Option<NodeId> {
    tree.parent(constant).filter(|&p| super::is_psi_class(tree, p))
}

/// `getModifierList()` (`PsiEnumConstantImpl.java:126`): the `MODIFIER_LIST` child (its annotations).
pub fn get_modifier_list(tree: &Tree, constant: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, constant, NodeKind::MODIFIER_LIST)
}
