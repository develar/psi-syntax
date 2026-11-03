//! `PsiParameterImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiParameterImpl.java`) over a `PARAMETER`
//! (of a method, a lambda, a catch section, a `for` each).

use super::{find_child_by_type, java_shared_impl_util, type_element, type_element_child};
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getNameIdentifier()` (`PsiParameterImpl.java:162`): the first `IDENTIFIER` child (IntelliJ requires one).
pub fn get_name_identifier(tree: &Tree, parameter: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, parameter, TokenKind::IDENTIFIER)
}

/// `getTypeElement()` (`PsiParameterImpl.java:201`): the first `PsiTypeElement` child; `None` for a lambda parameter
/// without a type (`x -> x`).
pub fn get_type_element(tree: &Tree, parameter: NodeId) -> Option<NodeId> {
    type_element_child(tree, parameter)
}

/// `getModifierList()` (`PsiParameterImpl.java:211`): the `MODIFIER_LIST` child (IntelliJ requires one).
pub fn get_modifier_list(tree: &Tree, parameter: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, parameter, crate::node_kind::NodeKind::MODIFIER_LIST)
}

/// `isVarArgs()` (`PsiParameterImpl.java:287`): the type element has an `ELLIPSIS` child (`int... xs`).
pub fn is_var_args(tree: &Tree, parameter: NodeId) -> bool {
    get_type_element(tree, parameter).is_some_and(|t| find_child_by_type(tree, t, TokenKind::ELLIPSIS).is_some())
}

/// `getType().getArrayDimensions()` (`PsiParameterImpl.java:171`, `JavaSharedImplUtil.getType(typeElement,
/// getNameIdentifier())`): the written dimensions of the type element and the brackets after the name
/// (`String... a[]` has 2). `None` without a type element (an untyped lambda parameter, whose type IntelliJ infers) or
/// a name, and for `var`.
pub fn get_type_array_dimensions(tree: &Tree, parameter: NodeId) -> Option<u32> {
    let type_element = get_type_element(tree, parameter)?;
    if type_element::is_inferred_type(tree, type_element) {
        return None;
    }
    let name = get_name_identifier(tree, parameter)?;
    Some(
        type_element::calculate_type(tree, type_element).array_dimensions
            + java_shared_impl_util::c_style_dimensions(tree, name),
    )
}
