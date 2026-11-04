//! `MethodDeclarationSyntax`: its fields (`super::fields`) and `MethodDeclarationSyntax.cs`'s computed property.

pub use super::fields::method_declaration::*;

use crate::tree::{NodeId, Tree};

/// `MethodDeclarationSyntax.Arity` (MethodDeclarationSyntax.cs:13): the type parameter count, 0 without a type
/// parameter list and on another node.
pub fn arity(tree: &Tree, method: NodeId) -> usize {
    type_parameter_list(tree, method).map_or(0, |list| super::type_parameter_list::parameters(tree, list).count())
}
