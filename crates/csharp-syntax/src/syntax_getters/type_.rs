//! `TypeSyntax` (`Syntax/TypeSyntax.cs` over `InternalSyntax/TypeSyntax.cs`): on a node of a type deriving from it
//! (`super::is_type_syntax`), whether it is an `IdentifierName` whose identifier's text (`ToString()`, so `@var` is
//! none) is a contextual type name. False on any other node.

use super::identifier_name;
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `IsIdentifierName(id)` (InternalSyntax/TypeSyntax.cs:17): an `IdentifierName` whose identifier's text is `id`.
fn is_identifier_name(tree: &Tree, ty: NodeId, id: &str) -> bool {
    tree.node_kind(ty) == Some(NodeKind::IdentifierName)
        && identifier_name::identifier(tree, ty).is_some_and(|i| tree.text(i) == id.as_bytes())
}

/// `TypeSyntax.IsVar` (TypeSyntax.cs:9, InternalSyntax/TypeSyntax.cs:11): `var`.
pub fn is_var(tree: &Tree, ty: NodeId) -> bool {
    is_identifier_name(tree, ty, "var")
}

/// `TypeSyntax.IsUnmanaged` (TypeSyntax.cs:11, InternalSyntax/TypeSyntax.cs:12): `unmanaged`.
pub fn is_unmanaged(tree: &Tree, ty: NodeId) -> bool {
    is_identifier_name(tree, ty, "unmanaged")
}

/// `TypeSyntax.IsNotNull` (TypeSyntax.cs:13, InternalSyntax/TypeSyntax.cs:13): `notnull`.
pub fn is_not_null(tree: &Tree, ty: NodeId) -> bool {
    is_identifier_name(tree, ty, "notnull")
}

/// `TypeSyntax.IsNint` (TypeSyntax.cs:15, InternalSyntax/TypeSyntax.cs:14): `nint`.
pub fn is_nint(tree: &Tree, ty: NodeId) -> bool {
    is_identifier_name(tree, ty, "nint")
}

/// `TypeSyntax.IsNuint` (TypeSyntax.cs:17, InternalSyntax/TypeSyntax.cs:15): `nuint`.
pub fn is_nuint(tree: &Tree, ty: NodeId) -> bool {
    is_identifier_name(tree, ty, "nuint")
}
