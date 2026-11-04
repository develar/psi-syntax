//! `DelegateDeclarationSyntax`: its fields (`super::fields`) and `DelegateDeclarationSyntax.cs`'s computed property.

pub use super::fields::delegate_declaration::*;

use crate::tree::{NodeId, Tree};

/// `DelegateDeclarationSyntax.Arity` (DelegateDeclarationSyntax.cs:13): the type parameter count, 0 without a type
/// parameter list and on another node.
pub fn arity(tree: &Tree, delegate: NodeId) -> usize {
    type_parameter_list(tree, delegate).map_or(0, |list| super::type_parameter_list::parameters(tree, list).count())
}
