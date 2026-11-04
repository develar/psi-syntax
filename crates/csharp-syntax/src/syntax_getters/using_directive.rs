//! `UsingDirectiveSyntax`: its fields (`super::fields`: `global_keyword`, `using_keyword`, `static_keyword`,
//! `unsafe_keyword`, `alias`, `namespace_or_type`, `semicolon_token`) and `UsingDirectiveSyntax.cs`'s computed
//! property. An alias's name is `super::name_equals::name` of `alias`; an `extern alias` is
//! `super::extern_alias_directive`.

pub use super::fields::using_directive::*;

use crate::tree::{NodeId, Tree};

/// `UsingDirectiveSyntax.Name` (UsingDirectiveSyntax.cs:19): `NamespaceOrType` when it is a name
/// (`super::is_name_syntax`), None for another type (`using P = (int, int);`, `using unsafe X = int*;`) and on another
/// node.
pub fn name(tree: &Tree, directive: NodeId) -> Option<NodeId> {
    namespace_or_type(tree, directive).filter(|&n| tree.node_kind(n).is_some_and(super::is_name_syntax))
}
