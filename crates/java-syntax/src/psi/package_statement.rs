//! `PsiPackageStatementImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiPackageStatementImpl.java`)
//! over a `PACKAGE_STATEMENT`.

use super::java_source_util;
use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `getPackageReference()` (`PsiPackageStatementImpl.java:38`, the `PACKAGE_REFERENCE` role): the
/// `JAVA_CODE_REFERENCE` child.
pub fn get_package_reference(tree: &Tree, statement: NodeId) -> Option<NodeId> {
    super::find_child_by_type(tree, statement, NodeKind::JAVA_CODE_REFERENCE)
}

/// `getPackageName()` (`PsiPackageStatementImpl.java:43`): the reference's text without whitespace, comments and
/// annotations (`java_source_util::get_reference_text`), `""` without a reference.
pub fn get_package_name(tree: &Tree, statement: NodeId) -> String {
    get_package_reference(tree, statement).map_or_else(String::new, |r| java_source_util::get_reference_text(tree, r))
}
