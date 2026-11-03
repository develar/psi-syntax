//! `PsiRecordComponentImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiRecordComponentImpl.java`) over a
//! `RECORD_COMPONENT`.

use super::{find_child_by_type, java_shared_impl_util, type_element};
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getContainingClass()` (`PsiRecordComponentImpl.java:62`): the grandparent (the class around the record header)
/// when it is a class.
pub fn get_containing_class(tree: &Tree, component: NodeId) -> Option<NodeId> {
    tree.parent(tree.parent(component)?).filter(|&g| super::is_psi_class(tree, g))
}

/// `getModifierList()` (`PsiRecordComponentImpl.java:70`): the `MODIFIER_LIST` child (IntelliJ requires one).
pub fn get_modifier_list(tree: &Tree, component: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, component, NodeKind::MODIFIER_LIST)
}

/// `getTypeElement()` (`PsiRecordComponentImpl.java:98`): the first `TYPE` child (IntelliJ requires one).
pub fn get_type_element(tree: &Tree, component: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, component, NodeKind::TYPE)
}

/// `getNameIdentifier()` (`PsiRecordComponentImpl.java:114`): the first `IDENTIFIER` child (IntelliJ requires one).
pub fn get_name_identifier(tree: &Tree, component: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, component, TokenKind::IDENTIFIER)
}

/// `isVarArgs()` (`PsiRecordComponentImpl.java:154`): `getType() instanceof PsiEllipsisType`, the type element's type
/// is (`int... xs`, also with brackets after the name, which wrap the ellipsis the other way round:
/// `JavaSharedImplUtil.getType` keeps the outermost an ellipsis). `false` without a type element or a name.
pub fn is_var_args(tree: &Tree, component: NodeId) -> bool {
    let (Some(type_element), Some(_)) = (get_type_element(tree, component), get_name_identifier(tree, component))
    else {
        return false;
    };
    type_element::calculate_type(tree, type_element).ellipsis
}

/// `getType().getArrayDimensions()` (`PsiRecordComponentImpl.java:79`): the type element's and the brackets after the
/// name; `None` without a type element or a name.
pub fn get_type_array_dimensions(tree: &Tree, component: NodeId) -> Option<u32> {
    let type_element = get_type_element(tree, component)?;
    let name = get_name_identifier(tree, component)?;
    Some(
        type_element::calculate_type(tree, type_element).array_dimensions
            + java_shared_impl_util::c_style_dimensions(tree, name),
    )
}
