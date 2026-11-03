//! `PsiTypeElementImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiTypeElementImpl.java`) over a
//! `TYPE`.
//!
//! PSI keeps an array type as one `TYPE` around the element's `TYPE` with every `[` `]` and `...` of that level:
//! `int[]... xs` is `TYPE(TYPE(TYPE(int) [ ]) ...)`, and a disjunction `A | B` or an intersection `A & B` is one `TYPE`
//! over the `TYPE` of each part.

use super::{find_child_by_type, is_whitespace_or_comment};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::PRIMITIVE_TYPE_BIT_SET;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// What `calculate_type` finds a type element's `PsiType` to be, without resolving it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TypeKind {
    /// `PsiTypes.nullType()`: an element with no type in it (an error, an empty `TYPE`; the implicit type of an
    /// unnamed pattern's empty `TYPE`, which IntelliJ infers from the record, is this too).
    Null,
    /// A `PsiPrimitiveType`: the keyword leaf (`int`, `void`).
    Primitive(NodeId),
    /// A `PsiClassReferenceType`: the `JAVA_CODE_REFERENCE`.
    ClassReference(NodeId),
    /// `var`: the type IntelliJ infers from the initializer or the iterated value (`inferVarType`); the leaf.
    Inferred(NodeId),
    /// A `PsiWildcardType`: `?`, `? extends T`, `? super T`.
    Wildcard,
    /// A `PsiIntersectionType`: `A & B`.
    Intersection,
    /// A `PsiDisjunctionType`: `A | B` of a multi-catch.
    Disjunction,
    /// A `PsiDiamondType`: the `<>` of a `DIAMOND_TYPE` child.
    Diamond,
}

/// The shape of a type element's type (`calculateType()`): the innermost type and the array levels around it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Type {
    pub kind: TypeKind,
    /// `getArrayDimensions()`: the `[` and `...` of every level from this element down to the innermost type.
    pub array_dimensions: u32,
    /// The outermost array is a `PsiEllipsisType` (`int...`, `int[]...`).
    pub ellipsis: bool,
}

/// `calculateType()` (`PsiTypeElementImpl.java:108`) without what resolves: each level's children in order, the last
/// primitive keyword, `var`, reference or `TYPE` child giving the type (the inner `TYPE` read the same way), each `[`
/// or `...` after it one array level, a `...` making the outermost an ellipsis, and a `?`, `&`, `|` or diamond ending
/// the level as a wildcard, intersection, disjunction or diamond type (an intersection or disjunction of one type
/// element being that element's type). A level without a type is `TypeKind::Null`, and
/// its brackets add nothing. Iterative over the levels, so any depth of nesting is safe.
pub fn calculate_type(tree: &Tree, type_element: NodeId) -> Type {
    let mut dimensions = 0u32;
    let mut ellipsis = None;
    let mut level = type_element;
    loop {
        let mut kind = None;
        let mut inner = None;
        let mut level_dimensions = 0u32;
        let mut level_ellipsis = false;
        for child in tree.children(level) {
            if is_whitespace_or_comment(tree, child) {
                continue;
            }
            match tree.node_kind(child) {
                Some(NodeKind::DIAMOND_TYPE) => {
                    kind = Some(TypeKind::Diamond);
                    inner = None;
                    break;
                }
                Some(NodeKind::TYPE) => {
                    kind = None;
                    inner = Some(child);
                    continue;
                }
                Some(NodeKind::JAVA_CODE_REFERENCE) => {
                    kind = Some(TypeKind::ClassReference(child));
                    inner = None;
                    continue;
                }
                _ => {}
            }
            let Some(token) = tree.token_kind(child) else { continue };
            if PRIMITIVE_TYPE_BIT_SET.contains(token) {
                kind = Some(TypeKind::Primitive(child));
                inner = None;
            } else if token == TokenKind::VAR_KEYWORD {
                kind = Some(TypeKind::Inferred(child));
                inner = None;
            } else if token == TokenKind::LBRACKET {
                level_dimensions += 1;
            } else if token == TokenKind::ELLIPSIS {
                level_dimensions += 1;
                level_ellipsis = true;
            } else if token == TokenKind::QUEST {
                kind = Some(TypeKind::Wildcard);
                inner = None;
                break;
            } else if token == TokenKind::AND || token == TokenKind::OR {
                // `createIntersection(false, types)`, `createDisjunction(types)` over every type element child: one
                // is that type itself (`A |` with an error after it), more (or none) the intersection or disjunction.
                let mut types = tree.children(level).filter(|&c| super::type_element_child_kind(tree, c));
                match (types.next(), types.next()) {
                    (Some(only), None) if tree.node_kind(only) == Some(NodeKind::TYPE) => {
                        kind = None;
                        inner = Some(only);
                    }
                    (Some(_), None) => {
                        kind = Some(TypeKind::Diamond);
                        inner = None;
                    }
                    _ => {
                        kind =
                            Some(if token == TokenKind::AND { TypeKind::Intersection } else { TypeKind::Disjunction });
                        inner = None;
                    }
                }
                break;
            }
        }
        if kind.is_none() && inner.is_none() {
            // `if (type == null) return PsiTypes.nullType()`: this level's brackets are dropped.
            return Type { kind: TypeKind::Null, array_dimensions: dimensions, ellipsis: ellipsis.unwrap_or(false) };
        }
        dimensions += level_dimensions;
        if level_dimensions > 0 && ellipsis.is_none() {
            ellipsis = Some(level_ellipsis);
        }
        match inner {
            Some(inner) if kind.is_none() => level = inner,
            _ => {
                let kind = kind.unwrap_or(TypeKind::Null);
                return Type { kind, array_dimensions: dimensions, ellipsis: ellipsis.unwrap_or(false) };
            }
        }
    }
}

/// `getInnermostComponentReferenceElement()` (`PsiTypeElementImpl.java:530`): down the first child while it is a
/// `TYPE`, then that level's `JAVA_CODE_REFERENCE` child.
pub fn get_innermost_component_reference_element(tree: &Tree, type_element: NodeId) -> Option<NodeId> {
    let mut level = type_element;
    loop {
        let first = tree.first_child(level)?;
        if tree.node_kind(first) != Some(NodeKind::TYPE) {
            return find_child_by_type(tree, level, NodeKind::JAVA_CODE_REFERENCE);
        }
        level = first;
    }
}

/// `isInferredType()` (`PsiTypeElementImpl.java:344`): the first child is the `var` keyword (no
/// `PsiAugmentProvider.isInferredType`).
pub fn is_inferred_type(tree: &Tree, type_element: NodeId) -> bool {
    tree.first_child(type_element).is_some_and(|c| super::is_token(tree, c, TokenKind::VAR_KEYWORD))
}
