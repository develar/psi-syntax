//! `TypeDeclarationSyntax`: the fields it and `BaseTypeDeclarationSyntax`, `MemberDeclarationSyntax` declare
//! (`super::fields`), on a class, struct, union, record, record struct, interface or extension block, and
//! `TypeDeclarationSyntax.cs`'s computed properties.

pub use super::fields::type_declaration::*;

use super::{base_list, type_parameter_list};
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `TypeDeclarationSyntax.Arity` (TypeDeclarationSyntax.cs:13): the type parameter count, 0 without a type parameter
/// list and on another node.
pub fn arity(tree: &Tree, declaration: NodeId) -> usize {
    type_parameter_list(tree, declaration).map_or(0, |list| type_parameter_list::parameters(tree, list).count())
}

/// `TypeDeclarationSyntax.PrimaryConstructorBaseTypeIfClass` (TypeDeclarationSyntax.cs:33): of a class or record, the
/// first base type when it passes arguments (`record R(int X) : Base(X)`, a `PrimaryConstructorBaseType`); None
/// otherwise.
pub fn primary_constructor_base_type_if_class(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    if !matches!(tree.node_kind(declaration)?, NodeKind::RecordDeclaration | NodeKind::ClassDeclaration) {
        return None;
    }
    let first = base_list::types(tree, base_list(tree, declaration)?).next()?;
    (tree.node_kind(first) == Some(NodeKind::PrimaryConstructorBaseType)).then_some(first)
}
