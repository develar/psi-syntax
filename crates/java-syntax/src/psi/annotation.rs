//! `PsiAnnotationImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiAnnotationImpl.java`) over
//! an `ANNOTATION`.

use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getNameReferenceElement()` (`PsiAnnotationImpl.java:60`): the first `JAVA_CODE_REFERENCE` child, the name of
/// `@org.junit.Test`. Its text as written, the tree-pure part of `getQualifiedName()` (which resolves), is
/// `java_source_util::get_reference_text`.
pub fn get_name_reference_element(tree: &Tree, annotation: NodeId) -> Option<NodeId> {
    super::find_child_by_type(tree, annotation, NodeKind::JAVA_CODE_REFERENCE)
}

/// `getParameterList()` (`PsiAnnotationImpl.java:87`): the `ANNOTATION_PARAMETER_LIST` child (IntelliJ requires one).
pub fn get_parameter_list(tree: &Tree, annotation: NodeId) -> Option<NodeId> {
    super::find_child_by_type(tree, annotation, NodeKind::ANNOTATION_PARAMETER_LIST)
}
