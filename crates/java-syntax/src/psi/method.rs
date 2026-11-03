//! `PsiMethodImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiMethodImpl.java`) with the child roles of
//! its `MethodElement` (`.../source/tree/java/MethodElement.java`), over a `METHOD` or an `ANNOTATION_METHOD`
//! (`PsiAnnotationMethodImpl extends PsiMethodImpl`).

use super::{find_child_by_type, java_shared_impl_util, parameter, parameter_list, record_component, record_header};
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getContainingClass()` (`PsiMethodImpl.java:104`): the parent when it is a class.
pub fn get_containing_class(tree: &Tree, method: NodeId) -> Option<NodeId> {
    tree.parent(method).filter(|&p| super::is_psi_class(tree, p))
}

/// `getNameIdentifier()` (`PsiMethodImpl.java:116`, `MethodElement.findChildByRole(NAME)` `MethodElement.java:84`):
/// the first `IDENTIFIER` child.
pub fn get_name_identifier(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, TokenKind::IDENTIFIER)
}

/// `getReturnTypeElement()` (`PsiMethodImpl.java:181`): the `TYPE` child; `None` for a constructor, which has none.
pub fn get_return_type_element(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, NodeKind::TYPE)
}

/// `getReturnType().getArrayDimensions()` (`PsiMethodImpl.java:201`, `JavaSharedImplUtil.getType(typeElement,
/// getParameterList())`): the written dimensions of the return type element and the brackets after the parameter list
/// (`String n()[]` returns `String[]`). `None` for a constructor; a method without a parameter list has no brackets
/// after it.
pub fn get_return_type_array_dimensions(tree: &Tree, method: NodeId) -> Option<u32> {
    let type_element = get_return_type_element(tree, method)?;
    let dimensions = super::type_element::calculate_type(tree, type_element).array_dimensions;
    Some(
        dimensions
            + get_parameter_list(tree, method).map_or(0, |list| java_shared_impl_util::c_style_dimensions(tree, list)),
    )
}

/// `getTypeParameterList()` (`PsiMethodImpl.java:187`): the `TYPE_PARAMETER_LIST` child.
pub fn get_type_parameter_list(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, NodeKind::TYPE_PARAMETER_LIST)
}

/// `getModifierList()` (`PsiMethodImpl.java:221`): the `MODIFIER_LIST` child (IntelliJ requires one).
pub fn get_modifier_list(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, NodeKind::MODIFIER_LIST)
}

/// `getParameterList()` (`PsiMethodImpl.java:231`): the `PARAMETER_LIST` child; `None` for a compact canonical
/// constructor (`record R(int a) { R { } }`), whose list IntelliJ makes up of the record's components without a node.
pub fn get_parameter_list(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, NodeKind::PARAMETER_LIST)
}

/// `getThrowsList()` (`PsiMethodImpl.java:257`): the `THROWS_LIST` child (the parser gives every method one, maybe
/// empty; IntelliJ throws without it).
pub fn get_throws_list(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, NodeKind::THROWS_LIST)
}

/// `getBody()` (`PsiMethodImpl.java:269`, `MethodElement.findChildByRole(METHOD_BODY)`): the `CODE_BLOCK` child.
pub fn get_body(tree: &Tree, method: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, method, NodeKind::CODE_BLOCK)
}

/// `isConstructor()` (`PsiMethodImpl.java:292`): no `TYPE` child (`MethodElement`'s `TYPE` role).
pub fn is_constructor(tree: &Tree, method: NodeId) -> bool {
    get_return_type_element(tree, method).is_none()
}

/// `isVarArgs()` (`PsiMethodImpl.java:302`, `PsiImplUtil.isVarArgs` `PsiImplUtil.java:752`): its last parameter is
/// (`parameter::is_var_args`). A compact constructor's parameters are the containing class's record components
/// (`PsiMethodImpl.java:241`), so it is when the last component is (`record_component::is_var_args`).
pub fn is_var_args(tree: &Tree, method: NodeId) -> bool {
    match get_parameter_list(tree, method) {
        Some(list) => {
            parameter_list::get_parameters(tree, list).last().is_some_and(|p| parameter::is_var_args(tree, p))
        }
        None => get_containing_class(tree, method)
            .and_then(|class| super::class::get_record_header(tree, class))
            .and_then(|header| record_header::get_record_components(tree, header).last())
            .is_some_and(|component| record_component::is_var_args(tree, component)),
    }
}

/// `JavaPsiRecordUtil.isCompactConstructor(method)` (`java/java-psi-impl/src/com/intellij/psi/util/JavaPsiRecordUtil.java:122`):
/// a constructor without a `PARAMETER_LIST` node (`R { ... }`), in a record or not.
pub fn is_compact_constructor(tree: &Tree, method: NodeId) -> bool {
    is_constructor(tree, method) && get_parameter_list(tree, method).is_none()
}
