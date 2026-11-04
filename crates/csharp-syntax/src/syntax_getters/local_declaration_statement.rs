//! `LocalDeclarationStatementSyntax`: its fields (`super::fields`) and `DeclarationStatementSyntax.cs`'s computed
//! property.

pub use super::fields::local_declaration_statement::*;

use crate::syntax_kind::TokenKind;
use crate::tree::{NodeId, Tree};

/// `LocalDeclarationStatementSyntax.IsConst` (DeclarationStatementSyntax.cs:9): a `const` among its modifiers; false
/// on another node.
pub fn is_const(tree: &Tree, statement: NodeId) -> bool {
    modifiers(tree, statement).any(|m| tree.token_kind(m) == Some(TokenKind::ConstKeyword))
}
