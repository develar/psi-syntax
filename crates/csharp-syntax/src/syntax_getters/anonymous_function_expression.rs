//! `AnonymousFunctionExpressionSyntax`: its fields (`super::fields`) on an anonymous method, a parenthesized or a
//! simple lambda, and `AnonymousFunctionExpressionSyntax.cs`'s computed properties.

pub use super::fields::anonymous_function_expression::*;

use crate::syntax_kind::TokenKind;
use crate::tree::{NodeId, Tree};

/// `AnonymousFunctionExpressionSyntax.Body` (AnonymousFunctionExpressionSyntax.cs:15): the `Block`, else the
/// `ExpressionBody`; None on another node.
pub fn body(tree: &Tree, function: NodeId) -> Option<NodeId> {
    block(tree, function).or_else(|| expression_body(tree, function))
}

/// `AsyncKeyword` (AnonymousMethodExpressionSyntax.cs:24, ParenthesizedLambdaExpressionSyntax.cs:21,
/// SimpleLambdaExpressionSyntax.cs:21: `Modifiers.FirstOrDefault(SyntaxKind.AsyncKeyword)`); None without one.
pub fn async_keyword(tree: &Tree, function: NodeId) -> Option<NodeId> {
    modifiers(tree, function).find(|&m| tree.token_kind(m) == Some(TokenKind::AsyncKeyword))
}
