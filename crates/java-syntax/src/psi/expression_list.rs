//! `PsiExpressionListImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiExpressionListImpl.java`)
//! over an `EXPRESSION_LIST`.

use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tree::{NodeId, Tree};

/// `getExpressions()` (`PsiExpressionListImpl.java:35`): the children in `EXPRESSION_BIT_SET`, in order. An error
/// element is none and an `EMPTY_EXPRESSION` is one: `f(a,)` has two, `a` and the empty one.
pub fn get_expressions(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_set(tree, list, EXPRESSION_BIT_SET)
}

/// `getExpressionCount()` (`PsiExpressionListImpl.java:53`): how many `get_expressions` there are.
pub fn get_expression_count(tree: &Tree, list: NodeId) -> usize {
    get_expressions(tree, list).count()
}
