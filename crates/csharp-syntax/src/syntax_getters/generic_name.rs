//! `GenericNameSyntax`: its fields (`super::fields`) and `GenericNameSyntax.cs`'s computed property.

pub use super::fields::generic_name::*;

use super::type_argument_list;
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `GenericNameSyntax.IsUnboundGenericName` (GenericNameSyntax.cs:15): a type argument is omitted (`List<>`,
/// `Dictionary<,>`); false on another node.
pub fn is_unbound_generic_name(tree: &Tree, name: NodeId) -> bool {
    type_argument_list(tree, name).is_some_and(|list| {
        type_argument_list::arguments(tree, list).any(|a| tree.node_kind(a) == Some(NodeKind::OmittedTypeArgument))
    })
}
