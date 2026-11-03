//! `PsiClassImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiClassImpl.java`) with the child roles of
//! its `ClassElement` (`.../source/tree/java/ClassElement.java`), over a `CLASS`, and the overrides of
//! `PsiAnonymousClassImpl` (`ANONYMOUS_CLASS`), `PsiEnumConstantInitializerImpl` (`ENUM_CONSTANT_INITIALIZER`) and
//! `PsiImplicitClassImpl` (`IMPLICIT_CLASS`) where they differ.

use super::modifier::Modifier;
use super::{find_child_by_set, find_child_by_type, skip_whitespace_and_comments};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::CLASS_KEYWORD_BIT_SET;
use crate::tokens::{TokenKind, TokenSet};
use crate::tree::{NodeId, Tree};

/// Whether the class is an anonymous class, an enum constant's body or an implicit class: the classes whose PSI
/// overrides the kind getters with `false` and the lists with `null`.
fn is_unnamed(tree: &Tree, class: NodeId) -> bool {
    matches!(
        tree.node_kind(class),
        Some(NodeKind::ANONYMOUS_CLASS | NodeKind::ENUM_CONSTANT_INITIALIZER | NodeKind::IMPLICIT_CLASS)
    )
}

/// `getNameIdentifier()` (`PsiClassImpl.java:146`, `ClassElement.findChildByRole(NAME)` `ClassElement.java:295`): the
/// first `IDENTIFIER` child; `None` for an anonymous class, an enum constant's body and an implicit class
/// (`PsiAnonymousClassImpl.java:123`, `PsiEnumConstantInitializerImpl.java:92`, `PsiImplicitClassImpl.java:224`).
pub fn get_name_identifier(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if is_unnamed(tree, class) {
        return None;
    }
    find_child_by_type(tree, class, TokenKind::IDENTIFIER)
}

/// `getModifierList()` (`PsiClassImpl.java:198`): the `MODIFIER_LIST` child; `None` for the unnamed classes
/// (`PsiAnonymousClassImpl.java:133`, `PsiEnumConstantInitializerImpl.java:102`, `PsiImplicitClassImpl.java:269`).
pub fn get_modifier_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if is_unnamed(tree, class) {
        return None;
    }
    find_child_by_type(tree, class, NodeKind::MODIFIER_LIST)
}

/// `hasModifierProperty(name)` (`PsiClassImpl.java:203`): see `psi::has_modifier_property`, which this is for a class.
pub fn has_modifier_property(tree: &Tree, class: NodeId, name: Modifier) -> bool {
    super::has_modifier_property(tree, class, name)
}

/// `getExtendsList()` (`PsiClassImpl.java:209`, `ClassElement.java:267`): the `EXTENDS_LIST` child (the parser gives
/// every named class one, maybe empty); `None` for the unnamed classes, whose lists are light or null
/// (`PsiAnonymousClassImpl.java:143`, `PsiEnumConstantInitializerImpl.java:112`, `PsiImplicitClassImpl.java:96`).
pub fn get_extends_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if is_unnamed(tree, class) {
        return None;
    }
    find_child_by_type(tree, class, NodeKind::EXTENDS_LIST)
}

/// `getImplementsList()` (`PsiClassImpl.java:214`, `ClassElement.java:270`): the `IMPLEMENTS_LIST` child; `None` for
/// the unnamed classes (`PsiAnonymousClassImpl.java:148`, `PsiEnumConstantInitializerImpl.java:117`,
/// `PsiImplicitClassImpl.java:101`).
pub fn get_implements_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if is_unnamed(tree, class) {
        return None;
    }
    find_child_by_type(tree, class, NodeKind::IMPLEMENTS_LIST)
}

/// `getPermitsList()` (`PsiClassImpl.java:229`, `ClassElement.java:273`): the `PERMITS_LIST` child, of any class.
pub fn get_permits_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, class, NodeKind::PERMITS_LIST)
}

/// `getRecordHeader()` (`PsiClassImpl.java:328`): the `RECORD_HEADER` child, of any class.
pub fn get_record_header(tree: &Tree, class: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, class, NodeKind::RECORD_HEADER)
}

/// `getTypeParameterList()` (`PsiClassImpl.java:408`): the `TYPE_PARAMETER_LIST` child; `None` for the unnamed classes
/// (`PsiAnonymousClassImpl.java:173`, `PsiEnumConstantInitializerImpl.java:142`, `PsiImplicitClassImpl.java:284`).
pub fn get_type_parameter_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if is_unnamed(tree, class) {
        return None;
    }
    find_child_by_type(tree, class, NodeKind::TYPE_PARAMETER_LIST)
}

/// `getContainingClass()` (`PsiClassImpl.java:254`): the parent when it is a class; `None` for a top-level or local
/// class and for the unnamed classes (`PsiAnonymousClassImpl.java:153`, `PsiImplicitClassImpl.java:244`).
pub fn get_containing_class(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if is_unnamed(tree, class) {
        return None;
    }
    tree.parent(class).filter(|&p| super::is_psi_class(tree, p))
}

/// `getOwnFields()` (`PsiClassImpl.java:333`): the `FIELD` and `ENUM_CONSTANT` children (`Constants.FIELD_BIT_SET`).
pub fn get_own_fields(tree: &Tree, class: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    const FIELD_BIT_SET: TokenSet = TokenSet::create_nodes(&[NodeKind::FIELD, NodeKind::ENUM_CONSTANT]);
    super::children_of_set(tree, class, FIELD_BIT_SET)
}

/// `getOwnMethods()` (`PsiClassImpl.java:338`): the `METHOD` and `ANNOTATION_METHOD` children
/// (`Constants.METHOD_BIT_SET`).
pub fn get_own_methods(tree: &Tree, class: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    const METHOD_BIT_SET: TokenSet = TokenSet::create_nodes(&[NodeKind::METHOD, NodeKind::ANNOTATION_METHOD]);
    super::children_of_set(tree, class, METHOD_BIT_SET)
}

/// `getOwnInnerClasses()` (`PsiClassImpl.java:343`): the `CLASS` children.
pub fn get_own_inner_classes(tree: &Tree, class: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_kind(tree, class, NodeKind::CLASS)
}

/// `ClassElement.findChildByRole(CLASS_OR_INTERFACE_KEYWORD)` (`ClassElement.java:279`): the first child in
/// `CLASS_KEYWORD_BIT_SET` (`class`, `interface`, `enum`, `record`), wherever it is (on valid code before the body's
/// `{`, so the scan stops there); `None` for an implicit class, and for a class the parser left without one (where
/// IntelliJ logs an error).
pub fn class_or_interface_keyword(tree: &Tree, class: NodeId) -> Option<NodeId> {
    if tree.node_kind(class) == Some(NodeKind::IMPLICIT_CLASS) {
        return None;
    }
    find_child_by_set(tree, class, CLASS_KEYWORD_BIT_SET)
}

/// `ClassElement.findChildByRole(AT)` (`ClassElement.java:304`): the `@` of `@interface`, the first sibling after the
/// first `MODIFIER_LIST` child that is no whitespace or comment, when it is an `AT`.
pub fn at(tree: &Tree, class: NodeId) -> Option<NodeId> {
    let modifier_list = find_child_by_type(tree, class, NodeKind::MODIFIER_LIST)?;
    skip_whitespace_and_comments(tree, tree.next_sibling(modifier_list))
        .filter(|&n| super::is_token(tree, n, TokenKind::AT))
}

/// `isInterface()` (`PsiClassImpl.java:441`): the class keyword is `interface`, which an annotation type's is too;
/// false for the unnamed classes (`PsiAnonymousClassImpl.java:158`, `PsiEnumConstantInitializerImpl.java:127`,
/// `PsiImplicitClassImpl.java:81`).
pub fn is_interface(tree: &Tree, class: NodeId) -> bool {
    !is_unnamed(tree, class) && keyword_is(tree, class, TokenKind::INTERFACE_KEYWORD)
}

/// `isAnnotationType()` (`PsiClassImpl.java:452`): an `AT` role (`at`); false for the unnamed classes
/// (`PsiAnonymousClassImpl.java:163`, `PsiEnumConstantInitializerImpl.java:132`, `PsiImplicitClassImpl.java:86`).
pub fn is_annotation_type(tree: &Tree, class: NodeId) -> bool {
    !is_unnamed(tree, class) && at(tree, class).is_some()
}

/// `isEnum()` (`PsiClassImpl.java:462`): the class keyword is `enum`; false for the unnamed classes
/// (`PsiAnonymousClassImpl.java:168`, `PsiEnumConstantInitializerImpl.java:137`, `PsiImplicitClassImpl.java:91`).
pub fn is_enum(tree: &Tree, class: NodeId) -> bool {
    !is_unnamed(tree, class) && keyword_is(tree, class, TokenKind::ENUM_KEYWORD)
}

/// `isRecord()` (`PsiClassImpl.java:473`): the class keyword is `record`. No unnamed class overrides it: an anonymous
/// class or an enum constant's body asks its children too (and has no such keyword on valid code), an implicit class
/// has no keyword.
pub fn is_record(tree: &Tree, class: NodeId) -> bool {
    keyword_is(tree, class, TokenKind::RECORD_KEYWORD)
}

/// `isValueClass()` (`PsiClassImpl.java:485`): `hasModifierProperty(VALUE)`, which no implicit modifier gives, so a
/// `value` keyword in the class's own modifier list; false for the unnamed classes, whose `hasModifierProperty`
/// never holds `value`.
pub fn is_value_class(tree: &Tree, class: NodeId) -> bool {
    get_modifier_list(tree, class)
        .is_some_and(|list| tree.first_child_of_kind(list, TokenKind::VALUE_KEYWORD).is_some())
}

fn keyword_is(tree: &Tree, class: NodeId, keyword: TokenKind) -> bool {
    class_or_interface_keyword(tree, class).is_some_and(|k| tree.token_kind(k) == Some(keyword))
}
