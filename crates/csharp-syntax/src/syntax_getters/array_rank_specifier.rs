//! `ArrayRankSpecifierSyntax`: its fields (`super::fields`) and `ArrayRankSpecifierSyntax.cs`'s computed property.

pub use super::fields::array_rank_specifier::*;

use crate::tree::{NodeId, Tree};

/// `ArrayRankSpecifierSyntax.Rank` (ArrayRankSpecifierSyntax.cs:9): the sizes, one per dimension (`[,]` is 2, each
/// size an `OmittedArraySizeExpression`); 0 on another node.
pub fn rank(tree: &Tree, specifier: NodeId) -> usize {
    sizes(tree, specifier).count()
}
