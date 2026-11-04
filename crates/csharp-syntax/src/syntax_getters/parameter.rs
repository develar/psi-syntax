//! `ParameterSyntax`: its fields (`super::fields`) and `ParameterSyntax.cs`'s computed property.

pub use super::fields::parameter::*;

use crate::syntax_kind::{NodeKind, TokenKind};
use crate::tree::{NodeId, Tree};

/// `ParameterSyntax.IsArgList` (ParameterSyntax.cs:9): `__arglist` (a parameter without a type whose identifier is the
/// `ArgListKeyword`); false on another node. The `this` of an extension method's parameter is
/// `super::has_modifier(tree, parameter, TokenKind::ThisKeyword)`.
pub fn is_arg_list(tree: &Tree, parameter: NodeId) -> bool {
    tree.node_kind(parameter) == Some(NodeKind::Parameter)
        && type_(tree, parameter).is_none()
        && identifier(tree, parameter).is_some_and(|i| tree.token_kind(i) == Some(TokenKind::ArgListKeyword))
}
