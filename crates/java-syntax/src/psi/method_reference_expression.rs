//! `PsiMethodReferenceExpressionImpl`
//! (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiMethodReferenceExpressionImpl.java`) over a
//! `METHOD_REF_EXPRESSION`: `Foo::bar` (qualifier the expression `Foo`), `String[]::new` and `List<String>::new`
//! (qualifier a `TYPE`, name the `new` keyword).

use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getReferenceNameElement()` (`PsiMethodReferenceExpressionImpl.java:245`): the last child when it is an
/// `IDENTIFIER` or `new`.
pub fn get_reference_name_element(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    super::last_child(tree, reference)
        .filter(|&c| matches!(tree.token_kind(c), Some(TokenKind::IDENTIFIER | TokenKind::NEW_KEYWORD)))
}

/// `getQualifier()` (`PsiMethodReferenceExpressionImpl.java:279`): the first child when it is an expression or a type
/// element.
pub fn get_qualifier(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    tree.first_child(reference).filter(|&c| {
        EXPRESSION_BIT_SET.contains(tree.kind(c))
            || matches!(tree.node_kind(c), Some(NodeKind::TYPE | NodeKind::DIAMOND_TYPE))
    })
}

/// `isConstructor()` (`PsiMethodReferenceExpressionImpl.java:360`): the name is the `new` keyword.
pub fn is_constructor(tree: &Tree, reference: NodeId) -> bool {
    get_reference_name_element(tree, reference).is_some_and(|n| super::is_token(tree, n, TokenKind::NEW_KEYWORD))
}
