//! `PsiFieldImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiFieldImpl.java`) with the child roles of
//! its `FieldElement` (`.../source/tree/java/FieldElement.java`), over a `FIELD`.
//!
//! The fields of one declaration (`@Inject private int a, b[], c;`) are sibling `FIELD`s: the parser gives the first
//! the `MODIFIER_LIST` and the `TYPE`, the later ones neither, and a later field reads them from the first. To ask
//! that of every field of a class, `declaration_heads` walks its children once and the `_with_head` getters take its
//! answer; `find_first_field_in_declaration` walks back from one field, as IntelliJ does.

use super::modifier::Modifier;
use super::{find_child_by_set, find_child_by_type, java_shared_impl_util, type_element};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::EXPRESSION_BIT_SET;
use crate::tokens::TokenKind;
use crate::tree::{NodeId, Tree};

/// `getContainingClass()` (`PsiFieldImpl.java:107`): the parent when it is a class.
pub fn get_containing_class(tree: &Tree, field: NodeId) -> Option<NodeId> {
    tree.parent(field).filter(|&p| super::is_psi_class(tree, p))
}

/// `getNameIdentifier()` (`PsiFieldImpl.java:124`, `FieldElement.findChildByRole(NAME)`): the first `IDENTIFIER`
/// child.
pub fn get_name_identifier(tree: &Tree, field: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, field, TokenKind::IDENTIFIER)
}

/// `getTypeElement()` (`PsiFieldImpl.java:168`): the `TYPE` child of `find_first_field_in_declaration` (`FieldElement`'s
/// `TYPE` role, its first `TYPE` child). `int a, b[];` gives `b` the `int` of `a`; its own brackets are
/// `java_shared_impl_util::collect_annotations` of its name.
pub fn get_type_element(tree: &Tree, field: NodeId) -> Option<NodeId> {
    get_type_element_with_head(tree, find_first_field_in_declaration(tree, field))
}

/// `getModifierList()` (`PsiFieldImpl.java:178`): its own `MODIFIER_LIST` child (`getSelfModifierList`, `:198`), else
/// that of `find_first_field_in_declaration`; `None` when no field before it has one, where IntelliJ throws
/// `IllegalStateException("Missing modifier list for sequence of fields")`.
pub fn get_modifier_list(tree: &Tree, field: NodeId) -> Option<NodeId> {
    get_modifier_list_with_head(tree, find_first_field_in_declaration(tree, field))
}

/// `hasModifierProperty(name)` (`PsiFieldImpl.java:203`): `get_modifier_list`'s, false without one.
pub fn has_modifier_property(tree: &Tree, field: NodeId, name: Modifier) -> bool {
    get_modifier_list(tree, field).is_some_and(|list| super::modifier_list::has_modifier_property(tree, list, name))
}

/// `findFirstFieldInDeclaration()` (`PsiFieldImpl.java:207`) as `findFirstFieldByTree()` (`:226`) finds it in a tree
/// without stubs: the field itself when it has its own `MODIFIER_LIST`, else, past every sibling before it that is no
/// `FIELD` (commas, comments, any other member), the previous `FIELD`'s; the first field of the walk when none has a
/// list. A step costs the climb of `Tree::previous_sibling`, so asking it of the n-th field of a declaration walks
/// back n fields; `declaration_heads` answers every field of a parent in one pass.
pub fn find_first_field_in_declaration(tree: &Tree, field: NodeId) -> NodeId {
    let mut first = field;
    loop {
        if find_child_by_type(tree, first, NodeKind::MODIFIER_LIST).is_some() {
            return first;
        }
        let mut previous = tree.previous_sibling(first);
        while let Some(p) = previous {
            if tree.node_kind(p) == Some(NodeKind::FIELD) {
                break;
            }
            previous = tree.previous_sibling(p);
        }
        match previous {
            Some(p) => first = p,
            None => return first,
        }
    }
}

/// `find_first_field_in_declaration` of every `FIELD` child of `parent`, in order, as `(field, first)` pairs: one
/// walk over the children, constant work a field (the fast path for a whole class; `tests/psi.rs` checks it equal to
/// the per-field walk).
pub fn declaration_heads(tree: &Tree, parent: NodeId) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
    let mut last_head = None;
    tree.children(parent).filter(|&c| tree.node_kind(c) == Some(NodeKind::FIELD)).map(move |field| {
        let own = find_child_by_type(tree, field, NodeKind::MODIFIER_LIST).is_some();
        // The walk back from a field without a list stops at the nearest field before it with one, else at the first
        // field of the parent, which is then the first head of all.
        let head = if own { field } else { *last_head.get_or_insert(field) };
        if own {
            last_head = Some(field);
        }
        (field, head)
    })
}

/// `get_type_element` of a field whose `find_first_field_in_declaration` is `head` (from `declaration_heads`): the
/// head's `TYPE` child.
pub fn get_type_element_with_head(tree: &Tree, head: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, head, NodeKind::TYPE)
}

/// `get_modifier_list` of a field whose `find_first_field_in_declaration` is `head`: the head's `MODIFIER_LIST` child.
pub fn get_modifier_list_with_head(tree: &Tree, head: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, head, NodeKind::MODIFIER_LIST)
}

/// `getInitializer()` (`PsiFieldImpl.java:244`, `FieldElement.findChildByRole(INITIALIZER)` `FieldElement.java:66`): the
/// first child in `EXPRESSION_BIT_SET`, wherever it is (an error element after `=` is none: `int a = ;` has no
/// initializer).
pub fn get_initializer(tree: &Tree, field: NodeId) -> Option<NodeId> {
    find_child_by_set(tree, field, EXPRESSION_BIT_SET)
}

/// `getType().getArrayDimensions()` (`PsiFieldImpl.java:145`, `JavaSharedImplUtil.getType(typeElement,
/// getNameIdentifier())`): the dimensions of the type element's type (`type_element::calculate_type`) and one for each
/// bracket after the name (`int a, b[];` gives `b` one). `None` without a type element or a name.
pub fn get_type_array_dimensions(tree: &Tree, field: NodeId) -> Option<u32> {
    get_type_array_dimensions_with_head(tree, field, find_first_field_in_declaration(tree, field))
}

/// `get_type_array_dimensions` of a field whose `find_first_field_in_declaration` is `head`.
pub fn get_type_array_dimensions_with_head(tree: &Tree, field: NodeId, head: NodeId) -> Option<u32> {
    let type_element = get_type_element_with_head(tree, head)?;
    let name = get_name_identifier(tree, field)?;
    Some(
        type_element::calculate_type(tree, type_element).array_dimensions
            + java_shared_impl_util::c_style_dimensions(tree, name),
    )
}
