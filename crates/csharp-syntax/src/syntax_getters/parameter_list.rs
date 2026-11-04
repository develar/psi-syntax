//! `ParameterListSyntax`: its fields (`super::fields`) and `ParameterListSyntax.cs`'s computed property.

pub use super::fields::parameter_list::*;

use crate::tree::{NodeId, Tree};

/// `ParameterListSyntax.ParameterCount` (ParameterListSyntax.cs:13): the parameters but `__arglist`
/// (`super::parameter::is_arg_list`); 0 on another node.
pub fn parameter_count(tree: &Tree, list: NodeId) -> usize {
    parameters(tree, list).filter(|&p| !super::parameter::is_arg_list(tree, p)).count()
}
