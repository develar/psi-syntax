//! `PsiLocalVariableImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiLocalVariableImpl.java`)
//! over a `LOCAL_VARIABLE`, and the overrides of its subclass `PsiResourceVariableImpl`
//! (`.../source/PsiResourceVariableImpl.java`) over a `RESOURCE_VARIABLE`.
//!
//! The variables of one declaration (`final int a = 1, b[];`) are sibling `LOCAL_VARIABLE`s of a
//! `DECLARATION_STATEMENT` (or of a `for`'s initialization): the first holds the `MODIFIER_LIST` and the `TYPE`, and a
//! later one reads them from the parent's first variable, not from the one before it.

use super::modifier::Modifier;
use super::{find_child_by_set, find_child_by_type, java_shared_impl_util, type_element, type_element_child};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getNameIdentifier()` (`PsiLocalVariableImpl.java:83`, `findChildByRole(NAME)` `:265`): the first `IDENTIFIER`
/// child (IntelliJ asserts there is one).
pub fn get_name_identifier(tree: &Tree, variable: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, variable, TokenKind::IDENTIFIER)
}

/// `getTypeElement()` (`PsiLocalVariableImpl.java:115`): its own first `PsiTypeElement` child, else that of the first
/// `LOCAL_VARIABLE` child of its parent; a resource variable's own (`PsiResourceVariableImpl.java:36`).
pub fn get_type_element(tree: &Tree, variable: NodeId) -> Option<NodeId> {
    if let Some(own) = type_element_child(tree, variable) {
        return Some(own);
    }
    if tree.node_kind(variable) == Some(NodeKind::RESOURCE_VARIABLE) {
        return None;
    }
    let first = find_child_by_type(tree, tree.parent(variable)?, NodeKind::LOCAL_VARIABLE)?;
    type_element_child(tree, first)
}

/// `getModifierList()` (`PsiLocalVariableImpl.java:129`): the `MODIFIER_LIST` child of the parent's first
/// `LOCAL_VARIABLE` (the variable itself for the first); a resource variable's own (`PsiResourceVariableImpl.java:41`).
pub fn get_modifier_list(tree: &Tree, variable: NodeId) -> Option<NodeId> {
    if tree.node_kind(variable) == Some(NodeKind::RESOURCE_VARIABLE) {
        return find_child_by_type(tree, variable, NodeKind::MODIFIER_LIST);
    }
    let first = find_child_by_type(tree, tree.parent(variable)?, NodeKind::LOCAL_VARIABLE)?;
    find_child_by_type(tree, first, NodeKind::MODIFIER_LIST)
}

/// `hasModifierProperty(name)` (`PsiLocalVariableImpl.java:137`): `get_modifier_list`'s, false without one.
pub fn has_modifier_property(tree: &Tree, variable: NodeId, name: Modifier) -> bool {
    get_modifier_list(tree, variable).is_some_and(|list| super::modifier_list::has_modifier_property(tree, list, name))
}

/// `getInitializer()` (`PsiLocalVariableImpl.java:143`, `findChildByRole(INITIALIZER)` `:271`): the first child in
/// `EXPRESSION_BIT_SET` (none for `int a = ;`, whose error element is no expression).
pub fn get_initializer(tree: &Tree, variable: NodeId) -> Option<NodeId> {
    find_child_by_set(tree, variable, EXPRESSION_BIT_SET)
}

/// `getType().getArrayDimensions()` (`PsiLocalVariableImpl.java:110`, `JavaSharedImplUtil.getType`): the written
/// dimensions of the type element and the brackets after the name. `None` without a type element or a name, and for
/// `var`, whose type IntelliJ infers from the initializer (`type_element::is_inferred_type`).
pub fn get_type_array_dimensions(tree: &Tree, variable: NodeId) -> Option<u32> {
    let type_element = get_type_element(tree, variable)?;
    if type_element::is_inferred_type(tree, type_element) {
        return None;
    }
    let name = get_name_identifier(tree, variable)?;
    Some(
        type_element::calculate_type(tree, type_element).array_dimensions
            + java_shared_impl_util::c_style_dimensions(tree, name),
    )
}
