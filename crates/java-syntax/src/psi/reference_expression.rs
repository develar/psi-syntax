//! `PsiReferenceExpressionImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiReferenceExpressionImpl.java`)
//! over a `REFERENCE_EXPRESSION`: `a.<T>b` (qualifier `a`, name `b`), a bare name (its first child the empty
//! `REFERENCE_PARAMETER_LIST`, no qualifier), the callee of `this(...)` and `super(...)` (named by the keyword).

use super::find_child_by_type;
use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getQualifierExpression()` (`PsiReferenceExpressionImpl.java:127`, `findChildByRole(QUALIFIER)` `:824`): the first
/// child when it is in `EXPRESSION_BIT_SET`.
pub fn get_qualifier_expression(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    tree.first_child(reference).filter(|&c| EXPRESSION_BIT_SET.contains(tree.kind(c)))
}

/// `getReferenceNameElement()` (`PsiReferenceExpressionImpl.java:647`, `findChildByRole(REFERENCE_NAME)` `:820`): the
/// last child when it is an `IDENTIFIER`, `this` or `super` (the name of a delegating call), else the first
/// `IDENTIFIER` child (`a.` with an error has none).
pub fn get_reference_name_element(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    let last = super::last_child(tree, reference)?;
    if matches!(tree.token_kind(last), Some(TokenKind::IDENTIFIER | TokenKind::THIS_KEYWORD | TokenKind::SUPER_KEYWORD))
    {
        return Some(last);
    }
    find_child_by_type(tree, reference, TokenKind::IDENTIFIER)
}

/// `getParameterList()` (`PsiReferenceExpressionImpl.java:652`): the first `REFERENCE_PARAMETER_LIST` child (the `<T>`
/// of `a.<T>b`, empty for a bare name).
pub fn get_parameter_list(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, reference, NodeKind::REFERENCE_PARAMETER_LIST)
}
