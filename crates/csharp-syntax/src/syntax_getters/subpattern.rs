//! `SubpatternSyntax`: its fields (`super::fields`) and `SubpatternSyntax.cs`'s computed property.

pub use super::fields::subpattern::*;

use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `SubpatternSyntax.NameColon` (SubpatternSyntax.cs:11): `ExpressionColon` when it is a `NameColon` (`{ X: 1 }`, not
/// `{ X.Y: 1 }`); None otherwise.
pub fn name_colon(tree: &Tree, subpattern: NodeId) -> Option<NodeId> {
    expression_colon(tree, subpattern).filter(|&c| tree.node_kind(c) == Some(NodeKind::NameColon))
}
