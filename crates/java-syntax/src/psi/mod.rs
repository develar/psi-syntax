//! The getters of IntelliJ's Java PSI classes over the tree: what `PsiField.getTypeElement()`,
//! `PsiModifierList.hasModifierProperty(...)`, `PsiJavaCodeReferenceElement.getQualifier()` and the like answer, ported
//! from `java/java-psi-impl` of JetBrains/intellij-community at decc943674edfe7ecc620d2d60b61be38f876994 (the
//! revision of the parser, see AGENTS.md), so that the consumers stop reimplementing them on raw kinds.
//!
//! A module per PSI class (`field` is `PsiFieldImpl`, `class` is `PsiClassImpl` with its `ClassElement` roles, ...),
//! a free function per method, named after it in snake_case, over a `&Tree` and the `NodeId` of the element (a node
//! of the class's element type: `field::get_type_element` takes a `FIELD`). A PSI element is the node of its element
//! type, a `PsiIdentifier` or `PsiKeyword` the token leaf, so an answer is a `NodeId`; `null` is `None`. What PSI
//! returns as a light element that has no node (an implicit class's empty extends list, a compact constructor's
//! parameter list) is `None` too, as is what IntelliJ throws on or asserts against (`PsiFieldImpl.getModifierList` of
//! a field without one, a cast of the wrong class). The functions of this module (`get_modifier_list`,
//! `has_modifier_property`, `get_name_identifier`, `get_type_element`, `get_containing_class`) are the interface
//! methods (`PsiModifierListOwner`, `PsiNameIdentifierOwner`, `PsiVariable`, `PsiMember`), dispatched on the element
//! type to the override of each class.
//!
//! Each answer is a function of the tree alone: no resolve, no stubs, no `PsiAugmentProvider` (Lombok and the like
//! add modifiers and annotations that are not in the text; here the augment step is the identity, as in a core
//! environment without providers). Where a method resolves (`PsiTypeElement.getType()` of `var`, `getQualifiedName`),
//! the port keeps the part that does not (`type_element::calculate_type`, `java_source_util::get_reference_text`) and
//! says so. No getter panics or allocates on any tree; `tests/psi.rs` runs each over every node of the goldens and
//! the robustness inputs, and `ksdump --getters --check` compares them with the live PSI (`tools/java-oracle`).

pub mod annotation;
pub mod anonymous_class;
pub mod class;
pub mod code_reference;
pub mod enum_constant;
pub mod expression_list;
pub mod field;
pub mod getters;
pub mod import_statement;
pub mod import_static_reference;
pub mod instance_of_expression;
pub mod java_shared_impl_util;
pub mod java_source_util;
pub mod lambda_expression;
pub mod local_variable;
pub mod method;
pub mod method_call_expression;
pub mod method_reference_expression;
pub mod modifier;
pub mod modifier_list;
pub mod new_expression;
pub mod package_statement;
pub mod parameter;
pub mod parameter_list;
pub mod record_component;
pub mod record_header;
pub mod reference_expression;
pub mod reference_list;
pub mod super_expression;
pub mod this_expression;
pub mod type_cast_expression;
pub mod type_element;
pub mod type_parameter_list;

use crate::node_kind::NodeKind;
use crate::tokens::{ElementType, TokenKind, TokenSet};
use crate::tree::{NodeId, Tree};
use modifier::Modifier;

/// `CompositeElement.findChildByType(type)`: the first child of the element type.
#[inline]
pub(crate) fn find_child_by_type(tree: &Tree, id: NodeId, kind: impl Into<ElementType>) -> Option<NodeId> {
    tree.first_child_of_kind(id, kind)
}

/// `CompositeElement.findChildByType(TokenSet)`: the first child whose element type is in the set.
#[inline]
pub(crate) fn find_child_by_set(tree: &Tree, id: NodeId, set: TokenSet) -> Option<NodeId> {
    tree.children(id).find(|&c| set.contains(tree.kind(c)))
}

/// `getChildrenAsPsiElements(TokenSet)`, `getStubOrPsiChildren(type)`: the children whose element type is in the set,
/// in order.
#[inline]
pub(crate) fn children_of_set(tree: &Tree, id: NodeId, set: TokenSet) -> impl Iterator<Item = NodeId> + '_ {
    tree.children(id).filter(move |&c| set.contains(tree.kind(c)))
}

/// The children of the node type, in order.
#[inline]
pub(crate) fn children_of_kind(tree: &Tree, id: NodeId, kind: NodeKind) -> impl Iterator<Item = NodeId> + '_ {
    tree.children(id).filter(move |&c| tree.node_kind(c) == Some(kind))
}

/// `TreeUtil.findChildBackward(parent, type)`: the last child of the element type. A walk over the children, which
/// costs their number (`Tree::last_child` and `previous_sibling` climb from the last descendant instead, which in a
/// deeply nested expression costs its depth); for the few children of a reference or an expression.
pub(crate) fn find_child_backward(tree: &Tree, id: NodeId, kind: impl Into<ElementType>) -> Option<NodeId> {
    let kind = kind.into();
    tree.children(id).filter(|&c| tree.kind(c) == kind).last()
}

/// `getLastChildNode()` of an element with few children (a reference, an expression): a walk over the children,
/// whose cost is their number, where `Tree::last_child` climbs from the last descendant, whose cost is the depth of
/// the last child's subtree (a nested `new int[][] {{{...}}}` would make a pass over every node quadratic).
#[inline]
pub(crate) fn last_child(tree: &Tree, id: NodeId) -> Option<NodeId> {
    tree.children(id).last()
}

/// `TreeUtil.isWhitespaceOrComment(node)`: the PSI of the node is a `PsiWhiteSpace` or a `PsiComment` (a doc comment,
/// `PsiDocComment`, is one).
#[inline]
pub(crate) fn is_whitespace_or_comment(tree: &Tree, id: NodeId) -> bool {
    tree.kind(id).token().is_some_and(|t| t.is_whitespace_or_comment() || t.is_psi_white_space())
}

/// `TreeUtil.skipWhitespaceAndComments(node, true)`: `id` or the first sibling after it that is no whitespace or
/// comment.
pub(crate) fn skip_whitespace_and_comments(tree: &Tree, id: Option<NodeId>) -> Option<NodeId> {
    let mut node = id;
    while let Some(n) = node {
        if !is_whitespace_or_comment(tree, n) {
            return Some(n);
        }
        node = tree.next_sibling(n);
    }
    None
}

/// Whether the token leaf is of the type: `PsiUtil.isJavaToken(element, type)`.
#[inline]
pub(crate) fn is_token(tree: &Tree, id: NodeId, token: TokenKind) -> bool {
    tree.token_kind(id) == Some(token)
}

/// Whether the node is a `PsiClass` (`Constants.CLASS_BIT_SET` and a type parameter, `PsiTypeParameter extends
/// PsiClass`).
#[inline]
pub(crate) fn is_psi_class(tree: &Tree, id: NodeId) -> bool {
    matches!(
        tree.node_kind(id),
        Some(
            NodeKind::CLASS
                | NodeKind::ANONYMOUS_CLASS
                | NodeKind::ENUM_CONSTANT_INITIALIZER
                | NodeKind::IMPLICIT_CLASS
                | NodeKind::TYPE_PARAMETER
        )
    )
}

/// Whether the node is the `PsiFile` (the root: `java.FILE`, or the test file of a parser entry).
#[inline]
pub(crate) fn is_psi_file(tree: &Tree, id: NodeId) -> bool {
    matches!(tree.node_kind(id), Some(NodeKind::JAVA_FILE | NodeKind::TEST_JAVA_FILE))
}

/// `PsiModifierListOwner.getModifierList()` of any owner, by its class's override: a `FIELD`'s own list or its
/// declaration's first field's (`field::get_modifier_list`), a local variable's from its declaration's first variable
/// (`local_variable::get_modifier_list`), a resource variable's, a parameter's, a record component's, a pattern
/// variable's, a receiver parameter's, a class's, a method's, an enum constant's and a class initializer's own
/// `MODIFIER_LIST` child (`PsiResourceVariableImpl.java:41`, `PsiParameterImpl.java:211`,
/// `PsiRecordComponentImpl.java:70`, `PsiPatternVariableImpl.java:121`, `PsiReceiverParameterImpl.java:34`,
/// `PsiClassImpl.java:198`, `PsiMethodImpl.java:221`, `PsiEnumConstantImpl.java:126`,
/// `PsiClassInitializerImpl.getModifierList`), the module statements' (`requires`) too; `None` for an anonymous class,
/// an enum constant's body, an implicit class (`PsiAnonymousClassImpl.java:133`, `PsiImplicitClassImpl.java:269`), a
/// type parameter (`PsiTypeParameterImpl.java:307`), a deconstruction pattern variable
/// (`PsiDeconstructionPatternVariableImpl.java:48`) and any other node.
pub fn get_modifier_list(tree: &Tree, owner: NodeId) -> Option<NodeId> {
    match tree.node_kind(owner)? {
        NodeKind::FIELD => field::get_modifier_list(tree, owner),
        NodeKind::LOCAL_VARIABLE => local_variable::get_modifier_list(tree, owner),
        NodeKind::RESOURCE_VARIABLE
        | NodeKind::PARAMETER
        | NodeKind::RECORD_COMPONENT
        | NodeKind::PATTERN_VARIABLE
        | NodeKind::RECEIVER_PARAMETER
        | NodeKind::CLASS
        | NodeKind::METHOD
        | NodeKind::ANNOTATION_METHOD
        | NodeKind::ENUM_CONSTANT
        | NodeKind::CLASS_INITIALIZER
        | NodeKind::MODULE
        | NodeKind::REQUIRES_STATEMENT => find_child_by_type(tree, owner, NodeKind::MODIFIER_LIST),
        _ => None,
    }
}

/// `PsiModifierListOwner.hasModifierProperty(name)` of any owner, by its class's override: an enum constant is
/// `public static final` whatever its list holds (`PsiEnumConstantImpl.java:131`), an anonymous class `final`
/// (`PsiAnonymousClassImpl.java:138`), an implicit class `final` and package-private
/// (`PsiImplicitClassImpl.java:274`), an enum constant's body, a type parameter and a deconstruction pattern variable
/// nothing (`PsiEnumConstantInitializerImpl.java:107`, `PsiTypeParameterImpl.java:312`,
/// `PsiDeconstructionPatternVariableImpl.java:53`); every other owner asks its `get_modifier_list`
/// (`modifier_list::has_modifier_property`), false without one (where IntelliJ throws for a field, a parameter, a
/// record component or a method without one, `getRequiredStubOrPsiChild`).
pub fn has_modifier_property(tree: &Tree, owner: NodeId, name: Modifier) -> bool {
    match tree.node_kind(owner) {
        Some(NodeKind::ENUM_CONSTANT) => matches!(name, Modifier::PUBLIC | Modifier::STATIC | Modifier::FINAL),
        Some(NodeKind::ANONYMOUS_CLASS) => name == Modifier::FINAL,
        Some(NodeKind::IMPLICIT_CLASS) => matches!(name, Modifier::FINAL | Modifier::PACKAGE_LOCAL),
        Some(
            NodeKind::ENUM_CONSTANT_INITIALIZER | NodeKind::TYPE_PARAMETER | NodeKind::DECONSTRUCTION_PATTERN_VARIABLE,
        ) => false,
        _ => get_modifier_list(tree, owner).is_some_and(|list| modifier_list::has_modifier_property(tree, list, name)),
    }
}

/// `PsiNameIdentifierOwner.getNameIdentifier()` of any owner: the first `IDENTIFIER` child of a class
/// (`PsiClassImpl.java:146`, `ClassElement.findChildByRole(NAME)` `ClassElement.java:295`), a method
/// (`PsiMethodImpl.java:116`, `MethodElement.java:84`), a field (`PsiFieldImpl.java:124`, `FieldElement.java:61`), an
/// enum constant (`PsiEnumConstantImpl.java:185`), a local or resource variable (`PsiLocalVariableImpl.java:83`), a
/// parameter (`PsiParameterImpl.java:162`), a record component (`PsiRecordComponentImpl.java:114`), a pattern variable
/// (`PsiPatternVariableImpl.java:46`), a deconstruction pattern variable (`PsiDeconstructionPatternVariableImpl.java:98`)
/// and a type parameter (`PsiTypeParameterImpl.java:183`); the first child of a labeled statement when it is an
/// `IDENTIFIER` (`PsiLabeledStatementImpl.java:124`, its `LABEL_NAME` role `:53`); the first `MODULE_REFERENCE` child
/// of a module (`PsiJavaModuleImpl.java:113`, a `PsiJavaModuleReferenceElement`, not an identifier); `None` without
/// one (IntelliJ asserts or throws for most of them), for an anonymous class, an enum constant's body, an implicit
/// class and a receiver parameter (`PsiAnonymousClassImpl.java:123`, `PsiEnumConstantInitializerImpl.java:92`,
/// `PsiImplicitClassImpl.java:224`, `PsiReceiverParameterImpl.java:65`), and for any other node. Java has no
/// backticks: the name is the identifier's text as written, unicode escapes included.
pub fn get_name_identifier(tree: &Tree, owner: NodeId) -> Option<NodeId> {
    match tree.node_kind(owner)? {
        NodeKind::LABELED_STATEMENT => tree.first_child(owner).filter(|&c| is_token(tree, c, TokenKind::IDENTIFIER)),
        NodeKind::MODULE => find_child_by_type(tree, owner, NodeKind::MODULE_REFERENCE),
        NodeKind::CLASS
        | NodeKind::METHOD
        | NodeKind::ANNOTATION_METHOD
        | NodeKind::FIELD
        | NodeKind::ENUM_CONSTANT
        | NodeKind::LOCAL_VARIABLE
        | NodeKind::RESOURCE_VARIABLE
        | NodeKind::PARAMETER
        | NodeKind::RECORD_COMPONENT
        | NodeKind::PATTERN_VARIABLE
        | NodeKind::DECONSTRUCTION_PATTERN_VARIABLE
        | NodeKind::TYPE_PARAMETER => find_child_by_type(tree, owner, TokenKind::IDENTIFIER),
        _ => None,
    }
}

/// `PsiJavaCodeReferenceElement.getReferenceNameElement()` of any reference: the identifier a reference is named by,
/// by its class's override — a reference expression's last child when it names one (`this`, `super` with it,
/// `PsiReferenceExpressionImpl.java:119` through `ChildRole.REFERENCE_NAME`), a code reference's last `IDENTIFIER`
/// (`PsiJavaCodeReferenceElementImpl.java:620`), a static import reference's and a method reference's
/// (`PsiImportStaticReferenceElementImpl.java:70`, `PsiMethodReferenceExpressionImpl.java:88`); `None` for any other
/// node. A consumer asks this instead of matching the four classes itself.
pub fn get_reference_name_element(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    match tree.node_kind(reference)? {
        NodeKind::REFERENCE_EXPRESSION => reference_expression::get_reference_name_element(tree, reference),
        NodeKind::JAVA_CODE_REFERENCE => code_reference::get_reference_name_element(tree, reference),
        NodeKind::IMPORT_STATIC_REFERENCE => import_static_reference::get_reference_name_element(tree, reference),
        NodeKind::METHOD_REF_EXPRESSION => method_reference_expression::get_reference_name_element(tree, reference),
        _ => None,
    }
}

/// `PsiVariable.getTypeElement()` of any variable: a field's or its declaration's first field's
/// (`field::get_type_element`), a local variable's or its declaration's first variable's
/// (`local_variable::get_type_element`), and the first `TYPE` child of a resource variable
/// (`PsiResourceVariableImpl.java:36`), a parameter (`PsiParameterImpl.java:201`; none for an untyped lambda
/// parameter), a record component (`PsiRecordComponentImpl.java:98`), a pattern variable
/// (`PsiPatternVariableImpl.java:95`) and a receiver parameter (`PsiReceiverParameterImpl.java:50`), the pattern's
/// type of a deconstruction pattern variable (the `R` of `R(int a) r`, `PsiDeconstructionPatternVariableImpl.java:73`:
/// `getPattern().getTypeElement()`, the first `TYPE` child of the parent `DECONSTRUCTION_PATTERN`,
/// `PsiDeconstructionPatternImpl.java:45`); `None` for an enum constant (`PsiEnumConstantImpl.java:141`) and any other
/// node. (`PsiTypeElement` is also the class of a `DIAMOND_TYPE`, which no variable holds.)
pub fn get_type_element(tree: &Tree, variable: NodeId) -> Option<NodeId> {
    match tree.node_kind(variable)? {
        NodeKind::FIELD => field::get_type_element(tree, variable),
        NodeKind::LOCAL_VARIABLE => local_variable::get_type_element(tree, variable),
        NodeKind::RESOURCE_VARIABLE
        | NodeKind::PARAMETER
        | NodeKind::RECORD_COMPONENT
        | NodeKind::PATTERN_VARIABLE
        | NodeKind::RECEIVER_PARAMETER => type_element_child(tree, variable),
        NodeKind::DECONSTRUCTION_PATTERN_VARIABLE => tree
            .parent(variable)
            .filter(|&p| tree.node_kind(p) == Some(NodeKind::DECONSTRUCTION_PATTERN))
            .and_then(|pattern| find_child_by_type(tree, pattern, NodeKind::TYPE)),
        _ => None,
    }
}

/// `PsiTreeUtil.getChildOfType(element, PsiTypeElement.class)`: the first `TYPE` or `DIAMOND_TYPE` child.
#[inline]
pub(crate) fn type_element_child(tree: &Tree, id: NodeId) -> Option<NodeId> {
    tree.children(id).find(|&c| type_element_child_kind(tree, c))
}

/// The node is a `PsiTypeElement`: a `TYPE` or a `DIAMOND_TYPE`.
#[inline]
pub(crate) fn type_element_child_kind(tree: &Tree, id: NodeId) -> bool {
    matches!(tree.node_kind(id), Some(NodeKind::TYPE | NodeKind::DIAMOND_TYPE))
}

/// `PsiVariable.getInitializer()` of any variable: the first child in `EXPRESSION_BIT_SET` of a field
/// (`field::get_initializer`) or a local or resource variable (`local_variable::get_initializer`); `None` for every
/// other variable (`PsiParameterImpl.java:221`, `PsiRecordComponentImpl.java:144`, `PsiEnumConstantImpl.java:146`,
/// ...) and node.
pub fn get_initializer(tree: &Tree, variable: NodeId) -> Option<NodeId> {
    match tree.node_kind(variable)? {
        NodeKind::FIELD => field::get_initializer(tree, variable),
        NodeKind::LOCAL_VARIABLE | NodeKind::RESOURCE_VARIABLE => local_variable::get_initializer(tree, variable),
        _ => None,
    }
}

/// `PsiMember.getContainingClass()` of any member: the parent when it is a class for a field, a method and an enum
/// constant (`PsiFieldImpl.java:107`, `PsiMethodImpl.java:104`, `PsiEnumConstantImpl.java:111`) and a named class
/// (`PsiClassImpl.java:254`), the class around the record header of a record component
/// (`PsiRecordComponentImpl.java:62`); `None` for an anonymous class, an enum constant's body, an implicit class
/// (`PsiAnonymousClassImpl.java:153`, `PsiImplicitClassImpl.java:244`), a type parameter
/// (`PsiTypeParameterImpl.java:297`), a top-level or local declaration and any other node. (The `PsiSyntheticClass`
/// ancestor IntelliJ falls back on lives in JSP and JShell trees, not in a Java file's.)
pub fn get_containing_class(tree: &Tree, member: NodeId) -> Option<NodeId> {
    match tree.node_kind(member)? {
        NodeKind::FIELD
        | NodeKind::METHOD
        | NodeKind::ANNOTATION_METHOD
        | NodeKind::ENUM_CONSTANT
        | NodeKind::CLASS
        | NodeKind::CLASS_INITIALIZER => tree.parent(member).filter(|&p| is_psi_class(tree, p)),
        NodeKind::RECORD_COMPONENT => record_component::get_containing_class(tree, member),
        _ => None,
    }
}
