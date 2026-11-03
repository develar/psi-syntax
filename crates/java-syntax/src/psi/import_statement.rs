//! `PsiImportStatementBaseImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiImportStatementBaseImpl.java`)
//! with the child roles of `ImportStatementBaseElement`, `ImportStatementElement` and `ImportStaticStatementElement`
//! (`.../source/tree/java/`), over an `IMPORT_STATEMENT`, an `IMPORT_STATIC_STATEMENT` or an
//! `IMPORT_MODULE_STATEMENT`.

use super::find_child_by_type;
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `isOnDemand()` (`PsiImportStatementBaseImpl.java:26`): a `DOT` child of the statement (the `IMPORT_ON_DEMAND_DOT`
/// role, `ImportStatementBaseElement.java:43`: `import a.b.*;` is a reference `a.b`, then `.` and `*`; `import a.b.;`
/// with its error is on demand too) or a `MODULE_REFERENCE` child; a module import always is
/// (`PsiImportModuleStatementImpl.isOnDemand`).
pub fn is_on_demand(tree: &Tree, statement: NodeId) -> bool {
    tree.node_kind(statement) == Some(NodeKind::IMPORT_MODULE_STATEMENT)
        || find_child_by_type(tree, statement, TokenKind::DOT).is_some()
        || find_child_by_type(tree, statement, NodeKind::MODULE_REFERENCE).is_some()
}

/// `getImportReference()` (`PsiImportStatementBaseImpl.java:37`, the `IMPORT_REFERENCE` role): of a static import the
/// `IMPORT_STATIC_REFERENCE` child (`import static a.B.c;`), else its `JAVA_CODE_REFERENCE` child (`import static
/// a.B.*;`, `ImportStaticStatementElement.java:31`); of an import the `JAVA_CODE_REFERENCE` child
/// (`ImportStatementElement.java:31`). `None` for a module import, whose reference is a `MODULE_REFERENCE`.
pub fn get_import_reference(tree: &Tree, statement: NodeId) -> Option<NodeId> {
    match tree.node_kind(statement)? {
        NodeKind::IMPORT_STATIC_STATEMENT => find_child_by_type(tree, statement, NodeKind::IMPORT_STATIC_REFERENCE)
            .or_else(|| find_child_by_type(tree, statement, NodeKind::JAVA_CODE_REFERENCE)),
        NodeKind::IMPORT_STATEMENT => find_child_by_type(tree, statement, NodeKind::JAVA_CODE_REFERENCE),
        _ => None,
    }
}

/// `PsiImportModuleStatementImpl.getModuleReference()`
/// (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiImportModuleStatementImpl.java:71`): of a module
/// import (`import module java.base;`) the first `MODULE_REFERENCE` child; `None` for any other statement.
pub fn get_module_reference(tree: &Tree, statement: NodeId) -> Option<NodeId> {
    if tree.node_kind(statement) != Some(NodeKind::IMPORT_MODULE_STATEMENT) {
        return None;
    }
    find_child_by_type(tree, statement, NodeKind::MODULE_REFERENCE)
}
