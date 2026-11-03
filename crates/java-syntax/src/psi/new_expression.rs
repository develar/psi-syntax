//! `PsiNewExpressionImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/java/PsiNewExpressionImpl.java`)
//! over a `NEW_EXPRESSION`: `new p.Q<T>(1)`, `new p.Q<T>(1) { }` (the reference and arguments then children of the
//! `ANONYMOUS_CLASS`), `o.new I()`, `new int[n][]`, `new int[] { 1 }`.

use super::{anonymous_class, children_of_set, find_child_by_type};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::{ARRAY_DIMENSION_BIT_SET, EXPRESSION_BIT_SET};
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getArgumentList()` (`PsiNewExpressionImpl.java:130`): its own `EXPRESSION_LIST` child, else its anonymous class's.
pub fn get_argument_list(tree: &Tree, new: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, new, NodeKind::EXPRESSION_LIST)
        .or_else(|| get_anonymous_class(tree, new).and_then(|class| anonymous_class::get_argument_list(tree, class)))
}

/// `getArrayDimensions()` (`PsiNewExpressionImpl.java:141`): the children in `ARRAY_DIMENSION_BIT_SET` (the `n` of
/// `new int[n]`), but the qualifier when it is one.
pub fn get_array_dimensions(tree: &Tree, new: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let qualifier = get_qualifier(tree, new).filter(|&q| ARRAY_DIMENSION_BIT_SET.contains(tree.kind(q)));
    children_of_set(tree, new, ARRAY_DIMENSION_BIT_SET).filter(move |&c| Some(c) != qualifier)
}

/// `getArrayInitializer()` (`PsiNewExpressionImpl.java:156`, `findChildByRole(ARRAY_INITIALIZER)` `:339`): the last
/// child when it is an `ARRAY_INITIALIZER_EXPRESSION`.
pub fn get_array_initializer(tree: &Tree, new: NodeId) -> Option<NodeId> {
    super::last_child(tree, new).filter(|&c| tree.node_kind(c) == Some(NodeKind::ARRAY_INITIALIZER_EXPRESSION))
}

/// `PsiNewExpression.isArrayCreation()` (`java/java-psi-api/src/com/intellij/psi/PsiNewExpression.java:92`): it has
/// array dimensions or an array initializer. `new int[]` and `new A[]` without either (an error) are none, though
/// they hold brackets.
pub fn is_array_creation(tree: &Tree, new: NodeId) -> bool {
    get_array_dimensions(tree, new).next().is_some() || get_array_initializer(tree, new).is_some()
}

/// `findChildByRole(QUALIFIER)` (`PsiNewExpressionImpl.java:307`): the first child unless it is `new`, whatever it is
/// (`getKindEnum` reads this role).
pub fn find_qualifier(tree: &Tree, new: NodeId) -> Option<NodeId> {
    tree.first_child(new).filter(|&c| !super::is_token(tree, c, TokenKind::NEW_KEYWORD))
}

/// `getQualifier()` (`PsiNewExpressionImpl.java:247`): `find_qualifier` as an expression, the `o` of `o.new I()`;
/// `None` when it is none (where IntelliJ's cast throws).
pub fn get_qualifier(tree: &Tree, new: NodeId) -> Option<NodeId> {
    find_qualifier(tree, new).filter(|&q| EXPRESSION_BIT_SET.contains(tree.kind(q)))
}

/// `getClassReference()` (`PsiNewExpressionImpl.java:267`, the `TYPE_REFERENCE` role): the `JAVA_CODE_REFERENCE` child
/// (none for `new int[1]` and for an anonymous class, whose reference is its own).
pub fn get_class_reference(tree: &Tree, new: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, new, NodeKind::JAVA_CODE_REFERENCE)
}

/// `getAnonymousClass()` (`PsiNewExpressionImpl.java:272`): the `ANONYMOUS_CLASS` child.
pub fn get_anonymous_class(tree: &Tree, new: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, new, NodeKind::ANONYMOUS_CLASS)
}

/// `getClassOrAnonymousClassReference()` (`PsiNewExpressionImpl.java:280`): the first child that is a
/// `JAVA_CODE_REFERENCE` or an `ANONYMOUS_CLASS`, the latter's base class reference.
pub fn get_class_or_anonymous_class_reference(tree: &Tree, new: NodeId) -> Option<NodeId> {
    let child = tree
        .children(new)
        .find(|&c| matches!(tree.node_kind(c), Some(NodeKind::JAVA_CODE_REFERENCE | NodeKind::ANONYMOUS_CLASS)))?;
    match tree.node_kind(child) {
        Some(NodeKind::ANONYMOUS_CLASS) => anonymous_class::get_base_class_reference(tree, child),
        _ => Some(child),
    }
}

/// `getTypeArgumentList()` (`PsiNewExpressionImpl.java:252`): the `REFERENCE_PARAMETER_LIST` child (`new <T>Foo()`).
pub fn get_type_argument_list(tree: &Tree, new: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, new, NodeKind::REFERENCE_PARAMETER_LIST)
}
