//! The getters of Kotlin's PSI classes over the tree: what `KtModifierListOwner.hasModifier`, `KtNamedDeclaration.getName`,
//! `KtTypeReference.getTypeElement`, `KtProperty.isLocal` and their kin answer, ported from Kotlin 2.4.21's
//! `compiler/psi/psi-api/src/org/jetbrains/kotlin/psi/` (9180ce6a), so that a consumer reads a declaration as the
//! compiler does instead of walking `NodeKind`s on its own.
//!
//! Each function is one Java or Kotlin method, its name in snake_case, its source in the doc comment (paths under
//! `compiler/psi/psi-api/src/org/jetbrains/kotlin/psi/`). A method of an interface or of a base class that several
//! element types share (`KtModifierListOwner`, `KtAnnotated`, `KtNamedDeclaration`, `KtCallableDeclaration`,
//! `KtTypeParameterListOwner`, `KtCallElement`, `KtValVarKeywordOwner`, `KtDeclarationWithInitializer`,
//! `KtSimpleNameExpression`, and the extensions of `psiUtil/ktPsiUtil.kt`) has the bare name and dispatches on the
//! node's kind as the virtual call does on its class (`get_type_reference` answers `KtNamedFunction`'s rule for a
//! `FUN`, `KtParameter`'s for a `VALUE_PARAMETER`); a method of one class carries the class (`kt_class_is_interface`,
//! `kt_nullable_type_get_inner_type`). A node whose class has no such method answers `None`, `false` or nothing.
//!
//! The PSI's own answers, not the stub's: these are the "by tree" branches (`getStubOrPsiChild` is the first child of
//! the type, `findChildByClass(KtExpression)` the first child whose PSI is a `KtExpression`, `is_kt_expression`).
//! Where the Java throws (a `!!`, `getRequiredStubOrPsiChild`, an assertion), the port answers `None`. A list is an
//! iterator over the children; nothing allocates but `kt_import_directive_get_imported_fq_name`, which builds the
//! name as `FqName` does. Names are the identifier's text with the backticks removed as `KtPsiUtil.unquoteIdentifier`
//! removes them, borrowed from the tree.
//!
//! `tools/kotlin-psi-getters/GetterOracle.java` prints what the PSI answers for every element of a file, and the
//! printer of `tests/common/getters.rs` the same lines from these functions; `tests/psi_getters.rs` compares them on
//! the cases of `tests/psi/`.

use std::borrow::Cow;

use crate::node_kind::NodeKind;
use crate::node_kind::NodeKind::*;
use crate::tokens::TokenKind::{self, *};
use crate::tokens::{OPERATIONS, TokenSet};
use crate::{ElementType, NodeId, Tree};

// ---- the platform's lookups ---------------------------------------------------------------------------------------

/// `findChildByType(type)` / `getStubOrPsiChild(type)`: the first child of that element type.
#[inline]
fn child(tree: &Tree, id: NodeId, kind: impl Into<ElementType>) -> Option<NodeId> {
    tree.first_child_of_kind(id, kind)
}

/// `findChildByType(TokenSet)`: the first child whose element type is in the set.
fn child_in(tree: &Tree, id: NodeId, set: &TokenSet) -> Option<NodeId> {
    tree.children(id).find(|&c| set.contains(tree.kind(c)))
}

/// `getStubOrPsiChildrenAsList(type)`: the children of that node type of `parent`, none without a parent.
fn children_of(tree: &Tree, parent: Option<NodeId>, kind: NodeKind) -> impl Iterator<Item = NodeId> + '_ {
    parent.into_iter().flat_map(move |p| tree.children(p)).filter(move |&c| tree.node_kind(c) == Some(kind))
}

/// `findChildByClass(KtExpression.class)`: the first child whose PSI is a `KtExpression`.
fn child_expression(tree: &Tree, id: NodeId) -> Option<NodeId> {
    tree.children(id).find(|&c| is_kt_expression(tree, c))
}

/// `PsiTreeUtil.getNextSiblingOfType(element, KtExpression.class)`: the first sibling after `element` whose PSI is a
/// `KtExpression`, None for a null element.
fn next_sibling_expression(tree: &Tree, element: Option<NodeId>) -> Option<NodeId> {
    let mut sibling = tree.next_sibling(element?);
    while let Some(s) = sibling {
        if is_kt_expression(tree, s) {
            return Some(s);
        }
        sibling = tree.next_sibling(s);
    }
    None
}

fn is_kind(tree: &Tree, id: NodeId, kind: NodeKind) -> bool {
    tree.node_kind(id) == Some(kind)
}

/// `psiUtil/ktPsiUtil.kt:903 isKtFile`: the element is the file.
fn is_kt_file(tree: &Tree, element: Option<NodeId>) -> bool {
    element.is_some_and(|e| is_kind(tree, e, KT_FILE))
}

/// `instanceof KtClassOrObject`: `KtClass` (with `KtEnumEntry`) and `KtObjectDeclaration`.
fn is_class_or_object(tree: &Tree, id: NodeId) -> bool {
    matches!(tree.node_kind(id), Some(CLASS | ENUM_ENTRY | OBJECT_DECLARATION))
}

/// `instanceof KtExpression`: the node types whose PSI class implements `KtExpression`, the declarations among them
/// (`KtDeclaration` extends it). An error element, a leaf, a container (`CONDITION`, `THEN`, `BODY`, ...), a type and
/// a list are none; `GetterOracle.java --classes` prints the classes of every element type.
pub fn is_kt_expression(tree: &Tree, id: NodeId) -> bool {
    matches!(
        tree.node_kind(id),
        Some(
            ANNOTATED_EXPRESSION
                | ARRAY_ACCESS_EXPRESSION
                | BACKING_FIELD
                | BINARY_EXPRESSION
                | BINARY_WITH_TYPE
                | BLOCK
                | BOOLEAN_CONSTANT
                | BREAK
                | CALLABLE_REFERENCE_EXPRESSION
                | CALL_EXPRESSION
                | CHARACTER_CONSTANT
                | CLASS
                | CLASS_INITIALIZER
                | CLASS_LITERAL_EXPRESSION
                | COLLECTION_LITERAL_EXPRESSION
                | CONSTRUCTOR_CALLEE
                | CONSTRUCTOR_DELEGATION_REFERENCE
                | CONTINUE
                | DESTRUCTURING_DECLARATION
                | DESTRUCTURING_DECLARATION_ENTRY
                | DOT_QUALIFIED_EXPRESSION
                | DO_WHILE
                | ENUM_ENTRY
                | ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION
                | FLOAT_CONSTANT
                | FOR
                | FUN
                | FUNCTION_LITERAL
                | IF
                | INTEGER_CONSTANT
                | IS_EXPRESSION
                | LABEL
                | LABELED_EXPRESSION
                | LAMBDA_EXPRESSION
                | NULL
                | OBJECT_DECLARATION
                | OBJECT_LITERAL
                | OPERATION_REFERENCE
                | PARENTHESIZED
                | POSTFIX_EXPRESSION
                | PREFIX_EXPRESSION
                | PRIMARY_CONSTRUCTOR
                | PROPERTY
                | PROPERTY_ACCESSOR
                | REFERENCE_EXPRESSION
                | RETURN
                | SAFE_ACCESS_EXPRESSION
                | SCRIPT
                | SCRIPT_INITIALIZER
                | SECONDARY_CONSTRUCTOR
                | STRING_TEMPLATE
                | SUPER_EXPRESSION
                | THIS_EXPRESSION
                | THROW
                | TRY
                | TYPEALIAS
                | TYPE_PARAMETER
                | VALUE_PARAMETER
                | WHEN
                | WHILE
        )
    )
}

// ---- KtPsiUtil's names --------------------------------------------------------------------------------------------

/// `KtPsiUtil.unquoteIdentifier` (KtPsiUtil.java:138): the text without one pair of surrounding backticks, removed
/// only when both are there.
pub fn unquote_identifier(quoted: &str) -> &str {
    if !quoted.contains('`') {
        return quoted;
    }
    if quoted.len() >= 2 && quoted.starts_with('`') && quoted.ends_with('`') {
        &quoted[1..quoted.len() - 1]
    } else {
        quoted
    }
}

/// `KtPsiUtil.unquoteIdentifierOrFieldReference` (KtPsiUtil.java:152): `unquote_identifier`, keeping a leading `$`.
pub fn unquote_identifier_or_field_reference(quoted: &str) -> Cow<'_, str> {
    if !quoted.contains('`') {
        return Cow::Borrowed(quoted);
    }
    match quoted.strip_prefix('$') {
        Some(rest) => {
            let unquoted = unquote_identifier(rest);
            if unquoted.len() == rest.len() { Cow::Borrowed(quoted) } else { Cow::Owned(format!("${unquoted}")) }
        }
        None => Cow::Borrowed(unquote_identifier(quoted)),
    }
}

/// `SpecialNames.NO_NAME_PROVIDED`, what `KtPsiUtil.safeName` gives a declaration without a name.
pub const NO_NAME_PROVIDED: &str = "<no name provided>";

/// `SpecialNames.DEFAULT_NAME_FOR_COMPANION_OBJECT`.
pub const DEFAULT_NAME_FOR_COMPANION_OBJECT: &str = "Companion";

/// `SpecialNames.ANONYMOUS_STRING`, `KtFunctionLiteral.getName`.
pub const ANONYMOUS_STRING: &str = "<anonymous>";

// ---- KtModifierListOwner, KtModifierList --------------------------------------------------------------------------

/// `KtModifierListOwnerStub.getModifierList` (KtModifierListOwnerStub.java:42): the first `MODIFIER_LIST` child. The
/// same rule is `KtDeclarationImpl.getModifierList`, `KtTypeReference`'s, `KtTypeProjection`'s and
/// `KtNullableType.getModifierList` (KtNullableType.java:63): a type reference, a nullable type `(@A Foo)?`, a primary
/// constructor, an accessor, a parameter and a type parameter each have their own list.
pub fn get_modifier_list(tree: &Tree, owner: NodeId) -> Option<NodeId> {
    child(tree, owner, MODIFIER_LIST)
}

/// `KtModifierListOwnerStub.hasModifier` (KtModifierListOwnerStub.java:47): the owner's modifier list has the keyword
/// as a child; only the first list counts, and the keyword is compared by token type.
pub fn has_modifier(tree: &Tree, owner: NodeId, modifier: TokenKind) -> bool {
    get_modifier_list(tree, owner).is_some_and(|list| kt_modifier_list_has_modifier(tree, list, modifier))
}

/// `KtModifierList.hasModifier` (KtModifierList.java:103).
pub fn kt_modifier_list_has_modifier(tree: &Tree, list: NodeId, modifier: TokenKind) -> bool {
    kt_modifier_list_get_modifier(tree, list, modifier).is_some()
}

/// `KtModifierList.getModifier(KtModifierKeywordToken)` (KtModifierList.java:112): the first child of that token type.
pub fn kt_modifier_list_get_modifier(tree: &Tree, list: NodeId, modifier: TokenKind) -> Option<NodeId> {
    child(tree, list, modifier)
}

/// `KtModifierList.getAnnotations` (KtModifierList.java:42): the `ANNOTATION` children (`@[A B]`, `@get:[A B]`).
pub fn kt_modifier_list_get_annotations(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, Some(list), ANNOTATION)
}

/// `KtModifierList.getAnnotationEntries` (KtModifierList.java:99), `collectAnnotationEntriesFromPsi`
/// (psiUtil/ktPsiUtil.kt:246): the `ANNOTATION_ENTRY` children and the entries of each `ANNOTATION` child, in document
/// order. The same for every `KtAnnotationsContainer`: a `FILE_ANNOTATION_LIST`, an `ANNOTATED_EXPRESSION`, a
/// `TYPE_CONSTRAINT`.
pub fn kt_modifier_list_get_annotation_entries(tree: &Tree, list: NodeId) -> AnnotationEntries<'_> {
    AnnotationEntries::of(tree, Some(list))
}

/// The entries of an annotations container: its `ANNOTATION_ENTRY` children and the entries of its `ANNOTATION`
/// children, in document order.
pub struct AnnotationEntries<'t> {
    tree: &'t Tree,
    outer: Option<crate::tree::Children<'t>>,
    inner: Option<crate::tree::Children<'t>>,
}

impl<'t> AnnotationEntries<'t> {
    fn of(tree: &'t Tree, container: Option<NodeId>) -> AnnotationEntries<'t> {
        AnnotationEntries { tree, outer: container.map(|c| tree.children(c)), inner: None }
    }
}

impl Iterator for AnnotationEntries<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        loop {
            if let Some(inner) = &mut self.inner {
                match inner.find(|&c| is_kind(self.tree, c, ANNOTATION_ENTRY)) {
                    Some(entry) => return Some(entry),
                    None => self.inner = None,
                }
            }
            let c = self.outer.as_mut()?.next()?;
            match self.tree.node_kind(c) {
                Some(ANNOTATION_ENTRY) => return Some(c),
                Some(ANNOTATION) => self.inner = Some(self.tree.children(c)),
                _ => {}
            }
        }
    }
}

/// `KtAnnotated.getAnnotationEntries`: a modifier list owner's are its modifier list's
/// (KtModifierListOwnerStub.java:85, KtTypeReference.kt:48, KtNullableType.java:68), a file's are its file annotation
/// list's (KtCommonFile.kt:307), and an annotations container (`MODIFIER_LIST`, `FILE_ANNOTATION_LIST`,
/// `ANNOTATED_EXPRESSION` KtAnnotatedExpression.java:48, `TYPE_CONSTRAINT` KtTypeConstraint.java:61) collects its own.
pub fn get_annotation_entries(tree: &Tree, annotated: NodeId) -> AnnotationEntries<'_> {
    let container = match tree.node_kind(annotated) {
        Some(KT_FILE) => kt_file_get_file_annotation_list(tree, annotated),
        Some(MODIFIER_LIST | FILE_ANNOTATION_LIST | ANNOTATED_EXPRESSION | TYPE_CONSTRAINT) => Some(annotated),
        _ => get_modifier_list(tree, annotated),
    };
    AnnotationEntries::of(tree, container)
}

/// `KtCommonFile.fileAnnotationList` (KtCommonFile.kt:69, `findChildBeforeFirstDeclarationInclusiveByType` :157): the
/// file's `FILE_ANNOTATION_LIST` child, looked for only before the first declaration or script.
pub fn kt_file_get_file_annotation_list(tree: &Tree, file: NodeId) -> Option<NodeId> {
    for c in tree.children(file) {
        match tree.node_kind(c) {
            Some(FILE_ANNOTATION_LIST) => return Some(c),
            Some(
                CLASS
                | OBJECT_DECLARATION
                | FUN
                | PROPERTY
                | DESTRUCTURING_DECLARATION
                | TYPEALIAS
                | CLASS_INITIALIZER
                | SECONDARY_CONSTRUCTOR
                | ENUM_ENTRY
                | SCRIPT,
            ) => return None,
            _ => {}
        }
    }
    None
}

/// `KtTokens.VISIBILITY_MODIFIERS` in `TokenSet.getTypes` order, the order of the token ids.
const VISIBILITY_MODIFIERS_TYPES: [TokenKind; 4] =
    [PRIVATE_KEYWORD, PUBLIC_KEYWORD, INTERNAL_KEYWORD, PROTECTED_KEYWORD];

/// `KtTokens.MODALITY_MODIFIERS` in `TokenSet.getTypes` order.
const MODALITY_MODIFIERS_TYPES: [TokenKind; 4] = [ABSTRACT_KEYWORD, OPEN_KEYWORD, SEALED_KEYWORD, FINAL_KEYWORD];

/// `KtModifierList.modifierFromTokenSet` (psiUtil/ktPsiUtil.kt:487): the first keyword of the set, in the set's order
/// (not the document's), that the list has.
fn modifier_from_token_set(tree: &Tree, list: NodeId, set: &[TokenKind]) -> Option<NodeId> {
    set.iter().find_map(|&t| kt_modifier_list_get_modifier(tree, list, t))
}

/// `KtModifierList.visibilityModifier` (psiUtil/ktPsiUtil.kt:496): the visibility keyword of the list, `private`
/// before `public`, `internal` and `protected` whatever their order in the text.
pub fn kt_modifier_list_visibility_modifier(tree: &Tree, list: NodeId) -> Option<NodeId> {
    modifier_from_token_set(tree, list, &VISIBILITY_MODIFIERS_TYPES)
}

/// `KtModifierList.visibilityModifierType` (psiUtil/ktPsiUtil.kt:498).
pub fn kt_modifier_list_visibility_modifier_type(tree: &Tree, list: NodeId) -> Option<TokenKind> {
    kt_modifier_list_visibility_modifier(tree, list).and_then(|m| tree.token_kind(m))
}

/// `KtModifierListOwner.visibilityModifier` (psiUtil/ktPsiUtil.kt:502).
pub fn visibility_modifier(tree: &Tree, owner: NodeId) -> Option<NodeId> {
    kt_modifier_list_visibility_modifier(tree, get_modifier_list(tree, owner)?)
}

/// `KtModifierListOwner.visibilityModifierType` (psiUtil/ktPsiUtil.kt:511): `PRIVATE_KEYWORD` for `private internal`,
/// `INTERNAL_KEYWORD` for `internal protected`, None without a visibility keyword.
pub fn visibility_modifier_type(tree: &Tree, owner: NodeId) -> Option<TokenKind> {
    visibility_modifier(tree, owner).and_then(|m| tree.token_kind(m))
}

/// `KtDeclaration.modalityModifier` (psiUtil/ktPsiUtil.kt:517): `abstract`, `open`, `sealed`, `final`, in that order.
pub fn modality_modifier(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    modifier_from_token_set(tree, get_modifier_list(tree, declaration)?, &MODALITY_MODIFIERS_TYPES)
}

/// `KtDeclaration.modalityModifierType` (psiUtil/ktPsiUtil.kt:519).
pub fn modality_modifier_type(tree: &Tree, declaration: NodeId) -> Option<TokenKind> {
    modality_modifier(tree, declaration).and_then(|m| tree.token_kind(m))
}

/// `KtModifierListOwner.isPrivate` (psiUtil/ktPsiUtil.kt:388).
pub fn is_private(tree: &Tree, owner: NodeId) -> bool {
    has_modifier(tree, owner, PRIVATE_KEYWORD)
}

/// `KtModifierListOwner.isProtected` (psiUtil/ktPsiUtil.kt:390).
pub fn is_protected(tree: &Tree, owner: NodeId) -> bool {
    has_modifier(tree, owner, PROTECTED_KEYWORD)
}

// ---- KtAnnotation, KtAnnotationEntry, KtAnnotationUseSiteTarget ---------------------------------------------------

/// `KtAnnotation.getEntries` (KtAnnotation.java:53): the `ANNOTATION_ENTRY` children of `@[A B]`.
pub fn kt_annotation_get_entries(tree: &Tree, annotation: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, Some(annotation), ANNOTATION_ENTRY)
}

/// `KtAnnotation.getUseSiteTarget` (KtAnnotation.java:59): the `ANNOTATION_TARGET` child of `@get:[A B]`.
pub fn kt_annotation_get_use_site_target(tree: &Tree, annotation: NodeId) -> Option<NodeId> {
    child(tree, annotation, ANNOTATION_TARGET)
}

/// `KtAnnotationEntry.getTypeReference` (KtAnnotationEntry.java:49): the `TYPE_REFERENCE` of its `CONSTRUCTOR_CALLEE`.
pub fn kt_annotation_entry_get_type_reference(tree: &Tree, entry: NodeId) -> Option<NodeId> {
    child(tree, child(tree, entry, CONSTRUCTOR_CALLEE)?, TYPE_REFERENCE)
}

/// `KtAnnotationEntry.getAtSymbol` (KtAnnotationEntry.java:109): the `AT` child (an entry inside `@[...]` has none).
pub fn kt_annotation_entry_get_at_symbol(tree: &Tree, entry: NodeId) -> Option<NodeId> {
    child(tree, entry, AT)
}

/// `KtAnnotationEntry.getUseSiteTarget` (KtAnnotationEntry.java:118): the entry's own `ANNOTATION_TARGET` child, else
/// the target of the `ANNOTATION` it is in (`@get:[A B]`).
pub fn kt_annotation_entry_get_use_site_target(tree: &Tree, entry: NodeId) -> Option<NodeId> {
    child(tree, entry, ANNOTATION_TARGET).or_else(|| {
        let parent = tree.parent(entry)?;
        if is_kind(tree, parent, ANNOTATION) { kt_annotation_get_use_site_target(tree, parent) } else { None }
    })
}

/// `KtAnnotationEntry.getShortName` (KtAnnotationEntry.java:132): the referenced name of the entry's type when it is
/// a user type, its own (last) segment, backticks removed: `JvmName` for `` @`JvmName` `` and for `@kotlin.jvm.JvmName`.
pub fn kt_annotation_entry_get_short_name<'t>(tree: &'t Tree, entry: NodeId) -> Option<Cow<'t, str>> {
    let type_element = kt_type_reference_get_type_element(tree, kt_annotation_entry_get_type_reference(tree, entry)?)?;
    if is_kind(tree, type_element, USER_TYPE) { get_referenced_name(tree, type_element) } else { None }
}

/// `AnnotationUseSiteTarget` (core/language.model/.../annotations/AnnotationUseSiteTarget.kt:19).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum AnnotationUseSiteTarget {
    All,
    Field,
    File,
    Property,
    PropertyGetter,
    PropertySetter,
    Receiver,
    ConstructorParameter,
    SetterParameter,
    PropertyDelegateField,
}

impl AnnotationUseSiteTarget {
    /// The enum constant's name (`PROPERTY_GETTER`), as `toString()` prints it.
    pub const fn name(self) -> &'static str {
        match self {
            AnnotationUseSiteTarget::All => "ALL",
            AnnotationUseSiteTarget::Field => "FIELD",
            AnnotationUseSiteTarget::File => "FILE",
            AnnotationUseSiteTarget::Property => "PROPERTY",
            AnnotationUseSiteTarget::PropertyGetter => "PROPERTY_GETTER",
            AnnotationUseSiteTarget::PropertySetter => "PROPERTY_SETTER",
            AnnotationUseSiteTarget::Receiver => "RECEIVER",
            AnnotationUseSiteTarget::ConstructorParameter => "CONSTRUCTOR_PARAMETER",
            AnnotationUseSiteTarget::SetterParameter => "SETTER_PARAMETER",
            AnnotationUseSiteTarget::PropertyDelegateField => "PROPERTY_DELEGATE_FIELD",
        }
    }

    /// `renderName`: the keyword of the target (`get`, `param`, `delegate`, `field`).
    pub const fn render_name(self) -> &'static str {
        match self {
            AnnotationUseSiteTarget::All => "all",
            AnnotationUseSiteTarget::Field => "field",
            AnnotationUseSiteTarget::File => "file",
            AnnotationUseSiteTarget::Property => "property",
            AnnotationUseSiteTarget::PropertyGetter => "get",
            AnnotationUseSiteTarget::PropertySetter => "set",
            AnnotationUseSiteTarget::Receiver => "receiver",
            AnnotationUseSiteTarget::ConstructorParameter => "param",
            AnnotationUseSiteTarget::SetterParameter => "setparam",
            AnnotationUseSiteTarget::PropertyDelegateField => "delegate",
        }
    }
}

/// `KtAnnotationUseSiteTarget.getAnnotationUseSiteTarget` (KtAnnotationUseSiteTarget.kt:33): the target its first
/// child's token names; None where the Java throws ("Unknown annotation target").
pub fn kt_annotation_use_site_target_get_annotation_use_site_target(
    tree: &Tree,
    target: NodeId,
) -> Option<AnnotationUseSiteTarget> {
    Some(match tree.kind(tree.first_child(target)?).token()? {
        ALL_KEYWORD => AnnotationUseSiteTarget::All,
        FIELD_KEYWORD => AnnotationUseSiteTarget::Field,
        FILE_KEYWORD => AnnotationUseSiteTarget::File,
        PROPERTY_KEYWORD => AnnotationUseSiteTarget::Property,
        GET_KEYWORD => AnnotationUseSiteTarget::PropertyGetter,
        SET_KEYWORD => AnnotationUseSiteTarget::PropertySetter,
        RECEIVER_KEYWORD => AnnotationUseSiteTarget::Receiver,
        PARAM_KEYWORD => AnnotationUseSiteTarget::ConstructorParameter,
        SETPARAM_KEYWORD => AnnotationUseSiteTarget::SetterParameter,
        DELEGATE_KEYWORD => AnnotationUseSiteTarget::PropertyDelegateField,
        _ => return None,
    })
}

// ---- KtNamedDeclaration -------------------------------------------------------------------------------------------

/// `KtNamedDeclaration.getNameIdentifier`: the first `IDENTIFIER` child (KtNamedDeclarationStub.java:69,
/// KtNamedDeclarationNotStubbed.java:59, KtImportAlias.kt:46) of a class, object, enum entry, function, property,
/// parameter, type parameter, type alias, destructuring entry or import alias; None for a constructor
/// (KtConstructor.kt:93), a function literal (KtFunctionLiteral.java:43) and any other node.
pub fn get_name_identifier(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    match tree.node_kind(declaration)? {
        CLASS
        | OBJECT_DECLARATION
        | ENUM_ENTRY
        | FUN
        | PROPERTY
        | VALUE_PARAMETER
        | TYPE_PARAMETER
        | TYPEALIAS
        | DESTRUCTURING_DECLARATION_ENTRY
        | IMPORT_ALIAS => child(tree, declaration, IDENTIFIER),
        _ => None,
    }
}

/// `KtNamedDeclaration.getName`: the name identifier's text without its backticks (KtNamedDeclarationStub.java:40,
/// `` class `my class` `` is `my class`), None without an identifier. Overridden: an object without a name is
/// `Companion` when it is a companion that is not top-level (KtObjectDeclaration.kt:36); a constructor is named after
/// its class (KtConstructor.kt:85); a function literal is `<anonymous>` (KtFunctionLiteral.java:38); an import
/// alias's name is its identifier's text as it is (KtImportAlias.kt:41). A script's name (from the file name) is not
/// ported: None.
pub fn get_name(tree: &Tree, declaration: NodeId) -> Option<&str> {
    match tree.node_kind(declaration)? {
        OBJECT_DECLARATION => {
            let name = get_name_identifier(tree, declaration).and_then(|i| tree.utf8_text(i)).map(unquote_identifier);
            if name.is_some() {
                return name;
            }
            let top_level = kt_class_or_object_is_top_level(tree, declaration);
            (kt_object_declaration_is_companion(tree, declaration) && !top_level)
                .then_some(DEFAULT_NAME_FOR_COMPANION_OBJECT)
        }
        PRIMARY_CONSTRUCTOR | SECONDARY_CONSTRUCTOR => {
            get_name(tree, kt_constructor_get_containing_class_or_object(tree, declaration)?)
        }
        FUNCTION_LITERAL => Some(ANONYMOUS_STRING),
        IMPORT_ALIAS => tree.utf8_text(get_name_identifier(tree, declaration)?),
        _ => get_name_identifier(tree, declaration).and_then(|i| tree.utf8_text(i)).map(unquote_identifier),
    }
}

/// `KtConstructor.getContainingClassOrObject` (KtConstructor.kt:26): a primary constructor's parent
/// (KtPrimaryConstructor.kt:31), a secondary constructor's `containingClassOrObject` (KtSecondaryConstructor.kt:34);
/// None where the Java throws.
pub fn kt_constructor_get_containing_class_or_object(tree: &Tree, constructor: NodeId) -> Option<NodeId> {
    match tree.node_kind(constructor)? {
        PRIMARY_CONSTRUCTOR => tree.parent(constructor).filter(|&p| is_class_or_object(tree, p)),
        SECONDARY_CONSTRUCTOR => containing_class_or_object(tree, constructor),
        _ => None,
    }
}

/// `KtDeclaration.containingClassOrObject` (psiUtil/ktPsiUtil.kt:526): the class or object a member is declared in,
/// through a class body (and a companion block), a primary constructor's parameter list, a destructuring declaration;
/// None for a local, a parameter of a function, an accessor.
pub fn containing_class_or_object(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    let parent = tree.parent(declaration)?;
    match tree.node_kind(parent)? {
        CLASS_BODY => kt_class_body_containing_class_or_object(tree, parent),
        CLASS | ENUM_ENTRY | OBJECT_DECLARATION => Some(parent),
        VALUE_PARAMETER_LIST => {
            let constructor = tree.parent(parent).filter(|&c| is_kind(tree, c, PRIMARY_CONSTRUCTOR))?;
            kt_constructor_get_containing_class_or_object(tree, constructor)
        }
        DESTRUCTURING_DECLARATION if is_kind(tree, declaration, DESTRUCTURING_DECLARATION_ENTRY) => {
            containing_class_or_object(tree, parent)
        }
        _ => None,
    }
}

/// `KtClassBody.containingClassOrObject` (psiUtil/ktPsiUtil.kt:618): the body's class or object, through a companion
/// block (KtCompanionBlock.containingClassOrObject :627).
pub fn kt_class_body_containing_class_or_object(tree: &Tree, body: NodeId) -> Option<NodeId> {
    let parent = tree.parent(body)?;
    match tree.node_kind(parent)? {
        CLASS | ENUM_ENTRY | OBJECT_DECLARATION => Some(parent),
        COMPANION_BLOCK => {
            let outer = tree.parent(parent).filter(|&b| is_kind(tree, b, CLASS_BODY))?;
            kt_class_body_containing_class_or_object(tree, outer)
        }
        _ => None,
    }
}

/// `KtDeclaration.containingScript` (psiUtil/ktPsiUtil.kt:568): the script of a top-level declaration of a `.kts`
/// (its parent is the script's `BLOCK`), through a destructuring declaration for its entry.
pub fn containing_script(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    let parent = tree.parent(declaration)?;
    match tree.node_kind(parent)? {
        BLOCK => tree.parent(parent).filter(|&s| is_kind(tree, s, SCRIPT)),
        DESTRUCTURING_DECLARATION if is_kind(tree, declaration, DESTRUCTURING_DECLARATION_ENTRY) => {
            containing_script(tree, parent)
        }
        _ => None,
    }
}

/// `KtDeclaration.containingClassOrScript` (psiUtil/ktPsiUtil.kt:583).
pub fn containing_class_or_script(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    containing_class_or_object(tree, declaration).or_else(|| containing_script(tree, declaration))
}

// ---- KtClassOrObject, KtClass, KtObjectDeclaration ----------------------------------------------------------------

/// `KtClassOrObject.isTopLevel` (KtClassOrObject.kt:85): the parent is the file (in a script it is the script's block).
pub fn kt_class_or_object_is_top_level(tree: &Tree, class: NodeId) -> bool {
    is_kt_file(tree, tree.parent(class))
}

/// `KtClassOrObject.isData` (KtClassOrObject.kt:108).
pub fn kt_class_or_object_is_data(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, DATA_KEYWORD)
}

/// `KtClassOrObject.isAnnotation` (KtClassOrObject.kt:132).
pub fn kt_class_or_object_is_annotation(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, ANNOTATION_KEYWORD)
}

/// `KtClassOrObject.getDeclarationKeyword` (KtClassOrObject.kt:134): the first `class`, `interface` or `object` child.
pub fn kt_class_or_object_get_declaration_keyword(tree: &Tree, class: NodeId) -> Option<NodeId> {
    const CLASS_INTERFACE_OBJECT: TokenSet = TokenSet::create(&[CLASS_KEYWORD, INTERFACE_KEYWORD, OBJECT_KEYWORD]);
    child_in(tree, class, &CLASS_INTERFACE_OBJECT)
}

/// `KtClassOrObject.getSuperTypeList` (KtClassOrObject.kt:31).
pub fn kt_class_or_object_get_super_type_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    child(tree, class, SUPER_TYPE_LIST)
}

fn is_super_type_list_entry(tree: &Tree, id: NodeId) -> bool {
    matches!(tree.node_kind(id), Some(DELEGATED_SUPER_TYPE_ENTRY | SUPER_TYPE_CALL_ENTRY | SUPER_TYPE_ENTRY))
}

/// `KtSuperTypeList.getEntries` (KtSuperTypeList.java:68), `KtInitializerList.getInitializers`
/// (KtInitializerList.java:44): the `SUPER_TYPE_ENTRY`, `SUPER_TYPE_CALL_ENTRY` and `DELEGATED_SUPER_TYPE_ENTRY`
/// children.
pub fn kt_super_type_list_get_entries(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    tree.children(list).filter(move |&c| is_super_type_list_entry(tree, c))
}

/// `KtClassOrObject.getSuperTypeListEntries` (KtClassOrObject.kt:35): the entries of the super type list; an enum
/// entry's are those of its `INITIALIZER_LIST` (KtEnumEntry.java:46), the `A(1)` of `enum class E(i: Int) { A(1) }`.
pub fn kt_class_or_object_get_super_type_list_entries(tree: &Tree, class: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let list = match tree.node_kind(class) {
        Some(ENUM_ENTRY) => child(tree, class, INITIALIZER_LIST),
        Some(CLASS | OBJECT_DECLARATION) => kt_class_or_object_get_super_type_list(tree, class),
        _ => None,
    };
    list.into_iter().flat_map(move |l| kt_super_type_list_get_entries(tree, l))
}

/// `KtClassOrObject.getBody` (KtClassOrObject.kt:54): the `CLASS_BODY` child.
pub fn kt_class_or_object_get_body(tree: &Tree, class: NodeId) -> Option<NodeId> {
    child(tree, class, CLASS_BODY)
}

/// `KtClassOrObject.getPrimaryConstructor` (KtClassOrObject.kt:114).
pub fn kt_class_or_object_get_primary_constructor(tree: &Tree, class: NodeId) -> Option<NodeId> {
    child(tree, class, PRIMARY_CONSTRUCTOR)
}

/// `KtClassOrObject.getPrimaryConstructorParameterList` (KtClassOrObject.kt:120).
pub fn kt_class_or_object_get_primary_constructor_parameter_list(tree: &Tree, class: NodeId) -> Option<NodeId> {
    get_value_parameter_list(tree, kt_class_or_object_get_primary_constructor(tree, class)?)
}

/// `KtClassOrObject.getPrimaryConstructorParameters` (KtClassOrObject.kt:122).
pub fn kt_class_or_object_get_primary_constructor_parameters(
    tree: &Tree,
    class: NodeId,
) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, kt_class_or_object_get_primary_constructor_parameter_list(tree, class), VALUE_PARAMETER)
}

/// `KtClassOrObject.getSecondaryConstructors` (KtClassOrObject.kt:130): the `SECONDARY_CONSTRUCTOR` children of the
/// body (KtClassBody.kt:55).
pub fn kt_class_or_object_get_secondary_constructors(tree: &Tree, class: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, kt_class_or_object_get_body(tree, class), SECONDARY_CONSTRUCTOR)
}

/// `KtClass.isInterface` (KtClass.kt:45): an `interface` keyword child, independent of the modifiers (`enum interface`
/// is both an enum and an interface).
pub fn kt_class_is_interface(tree: &Tree, class: NodeId) -> bool {
    child(tree, class, INTERFACE_KEYWORD).is_some()
}

/// `KtClass.isEnum` (KtClass.kt:48).
pub fn kt_class_is_enum(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, ENUM_KEYWORD)
}

/// `KtClass.isSealed` (KtClass.kt:49).
pub fn kt_class_is_sealed(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, SEALED_KEYWORD)
}

/// `KtClass.isInner` (KtClass.kt:50).
pub fn kt_class_is_inner(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, INNER_KEYWORD)
}

/// `KtClass.isInline` (KtClass.kt:51).
pub fn kt_class_is_inline(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, INLINE_KEYWORD)
}

/// `KtClass.isValue` (KtClass.kt:52).
pub fn kt_class_is_value(tree: &Tree, class: NodeId) -> bool {
    has_modifier(tree, class, VALUE_KEYWORD)
}

/// `KtClass.getFunKeyword` (KtClass.kt:60): the `fun` of `fun interface`, in the modifier list.
pub fn kt_class_get_fun_keyword(tree: &Tree, class: NodeId) -> Option<NodeId> {
    kt_modifier_list_get_modifier(tree, get_modifier_list(tree, class)?, FUN_KEYWORD)
}

/// `KtObjectDeclaration.isCompanion` (KtObjectDeclaration.kt:50).
pub fn kt_object_declaration_is_companion(tree: &Tree, object: NodeId) -> bool {
    has_modifier(tree, object, COMPANION_KEYWORD)
}

/// `KtObjectDeclaration.isObjectLiteral` (KtObjectDeclaration.kt:59): the parent is an `OBJECT_LITERAL`.
pub fn kt_object_declaration_is_object_literal(tree: &Tree, object: NodeId) -> bool {
    tree.parent(object).is_some_and(|p| is_kind(tree, p, OBJECT_LITERAL))
}

/// `KtObjectDeclaration.getObjectKeyword` (KtObjectDeclaration.kt:61).
pub fn kt_object_declaration_get_object_keyword(tree: &Tree, object: NodeId) -> Option<NodeId> {
    child(tree, object, OBJECT_KEYWORD)
}

/// `KtSuperTypeListEntry.getTypeAsUserType` (KtSuperTypeListEntry.java:53): the entry's type element when it is a
/// `USER_TYPE`; None for `Base?` and `(Int) -> Unit`.
pub fn kt_super_type_list_entry_get_type_as_user_type(tree: &Tree, entry: NodeId) -> Option<NodeId> {
    let element = kt_type_reference_get_type_element(tree, get_type_reference(tree, entry)?)?;
    is_kind(tree, element, USER_TYPE).then_some(element)
}

// ---- KtCallableDeclaration and the other typed declarations -------------------------------------------------------

/// `getReceiverTypeReference`: a function's is its first `TYPE_REFERENCE` child before an `LPAR` or `COLON` child
/// (KtNamedFunction.kt:112, `receiverTypeRefByTree` :120), a property's its first before a `COLON` child
/// (KtProperty.java:106, :120), a function type's the type of its `FUNCTION_TYPE_RECEIVER` (KtFunctionType.java:88).
/// No `DOT` is needed: the parser builds a receiver only up to the last dot. `fun (Int?).g()` answers `(Int?)`.
/// Parameters, constructors, destructuring entries and function literals have none (KtParameter.java:256, ...).
pub fn get_receiver_type_reference(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    let stop_at_lpar = match tree.node_kind(declaration)? {
        FUN => true,
        PROPERTY => false,
        FUNCTION_TYPE => return get_type_reference(tree, kt_function_type_get_receiver(tree, declaration)?),
        _ => return None,
    };
    for c in tree.children(declaration) {
        match tree.kind(c) {
            ElementType::Token(COLON) => return None,
            ElementType::Token(LPAR) if stop_at_lpar => return None,
            ElementType::Node(TYPE_REFERENCE) => return Some(c),
            _ => {}
        }
    }
    None
}

/// `TypeRefHelpers.getTypeReference` (TypeRefHelpers.kt:27): the first `TYPE_REFERENCE` among the children from the
/// first `COLON` on, whatever lies between them.
fn type_reference_after_colon(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    tree.children(declaration)
        .skip_while(|&c| tree.kind(c) != ElementType::Token(COLON))
        .find(|&c| is_kind(tree, c, TYPE_REFERENCE))
}

/// `getTypeReference`, the declared or return type: a function's, property's and destructuring entry's is the first
/// `TYPE_REFERENCE` from the first `COLON` child on (TypeRefHelpers.kt:27, KtNamedFunction.kt:135, KtProperty.java:137,
/// KtDestructuringDeclarationEntry.java:45); a parameter's, type alias's, accessor's, type projection's, function type
/// receiver's and constructor callee's is its first `TYPE_REFERENCE` child (KtParameter.java:91, KtTypeAlias.kt:42,
/// KtPropertyAccessor.kt:102, KtTypeProjection.java:60, KtFunctionTypeReceiver.java, KtConstructorCalleeExpression.java);
/// a super type entry's is its `TYPE_REFERENCE` child (KtSuperTypeListEntry.java:48), a super type call entry's and an
/// annotation entry's the one of their `CONSTRUCTOR_CALLEE` (KtSuperTypeCallEntry.java:68, KtAnnotationEntry.java:49).
/// Constructors, enum entries and function literals have none.
pub fn get_type_reference(tree: &Tree, element: NodeId) -> Option<NodeId> {
    match tree.node_kind(element)? {
        FUN | PROPERTY | DESTRUCTURING_DECLARATION_ENTRY => type_reference_after_colon(tree, element),
        VALUE_PARAMETER
        | TYPEALIAS
        | PROPERTY_ACCESSOR
        | TYPE_PROJECTION
        | FUNCTION_TYPE_RECEIVER
        | CONSTRUCTOR_CALLEE
        | SUPER_TYPE_ENTRY
        | DELEGATED_SUPER_TYPE_ENTRY => child(tree, element, TYPE_REFERENCE),
        SUPER_TYPE_CALL_ENTRY | ANNOTATION_ENTRY => {
            child(tree, child(tree, element, CONSTRUCTOR_CALLEE)?, TYPE_REFERENCE)
        }
        _ => None,
    }
}

/// `getValueParameterList`: the `VALUE_PARAMETER_LIST` child of a function (KtNamedFunction.kt:75), a constructor
/// (KtConstructor.kt:30), a function literal (KtFunctionNotStubbed.java:39) and an accessor (`parameterList`,
/// KtPropertyAccessor.kt:52); None for a property, parameter or destructuring entry.
pub fn get_value_parameter_list(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    match tree.node_kind(declaration)? {
        FUN | PRIMARY_CONSTRUCTOR | SECONDARY_CONSTRUCTOR | FUNCTION_LITERAL | PROPERTY_ACCESSOR => {
            child(tree, declaration, VALUE_PARAMETER_LIST)
        }
        _ => None,
    }
}

/// `getValueParameters`: the `VALUE_PARAMETER`s of the value parameter list (KtNamedFunction.kt:79,
/// KtConstructor.kt:34, KtFunctionNotStubbed.java:45, KtParameterList.getParameters :58); an accessor's is its first
/// parameter only (KtPropertyAccessor.kt:60); a lambda's are its function literal's (KtLambdaExpression.java:47).
pub fn get_value_parameters(tree: &Tree, declaration: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let (list, limit) = match tree.node_kind(declaration) {
        Some(PROPERTY_ACCESSOR) => (get_value_parameter_list(tree, declaration), 1),
        Some(LAMBDA_EXPRESSION) => (
            kt_lambda_expression_get_function_literal(tree, declaration)
                .and_then(|f| get_value_parameter_list(tree, f)),
            usize::MAX,
        ),
        _ => (get_value_parameter_list(tree, declaration), usize::MAX),
    };
    children_of(tree, list, VALUE_PARAMETER).take(limit)
}

/// `KtTypeParameterListOwner.getTypeParameterList` (KtTypeParameterListOwnerStub.java:31,
/// KtTypeParameterListOwnerNotStubbed.java:37): the `TYPE_PARAMETER_LIST` child of a class, object, enum entry,
/// function, property, type alias or function literal; None for a constructor, parameter or destructuring entry.
pub fn get_type_parameter_list(tree: &Tree, owner: NodeId) -> Option<NodeId> {
    match tree.node_kind(owner)? {
        CLASS | OBJECT_DECLARATION | ENUM_ENTRY | FUN | PROPERTY | TYPEALIAS | FUNCTION_LITERAL => {
            child(tree, owner, TYPE_PARAMETER_LIST)
        }
        _ => None,
    }
}

/// `KtTypeParameterListOwner.getTypeParameters` (KtTypeParameterListOwnerStub.java:54): the `TYPE_PARAMETER` children
/// of the list (KtTypeParameterList.java:38).
pub fn get_type_parameters(tree: &Tree, owner: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, get_type_parameter_list(tree, owner), TYPE_PARAMETER)
}

/// `KtTypeParameterListOwner.getTypeConstraintList` (KtTypeParameterListOwnerStub.java:38): the `where` list.
pub fn get_type_constraint_list(tree: &Tree, owner: NodeId) -> Option<NodeId> {
    match tree.node_kind(owner)? {
        CLASS | OBJECT_DECLARATION | ENUM_ENTRY | FUN | PROPERTY | TYPEALIAS | FUNCTION_LITERAL => {
            child(tree, owner, TYPE_CONSTRAINT_LIST)
        }
        _ => None,
    }
}

/// `KtTypeParameterListOwner.getTypeConstraints` (KtTypeParameterListOwnerStub.java:44): the `TYPE_CONSTRAINT`
/// children of the list (KtTypeConstraintList.java:39).
pub fn get_type_constraints(tree: &Tree, owner: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, get_type_constraint_list(tree, owner), TYPE_CONSTRAINT)
}

/// `PsiElement.isExtensionDeclaration` (psiUtil/ktPsiUtil.kt:333): a function or property with a receiver type, an
/// accessor of one (its nearest `PROPERTY` ancestor).
pub fn is_extension_declaration(tree: &Tree, element: NodeId) -> bool {
    let callable = match tree.node_kind(element) {
        Some(FUN | PROPERTY) => Some(element),
        Some(PROPERTY_ACCESSOR) => tree.ancestor_or_self(element, |n| is_kind(tree, n, PROPERTY)),
        _ => None,
    };
    callable.and_then(|c| get_receiver_type_reference(tree, c)).is_some()
}

/// `KtNamedFunction.isLocal` (KtNamedFunction.kt:154): false when the parent is the file, a class body, or the
/// parent's parent is a script (a top-level function of a `.kts`), true otherwise.
pub fn kt_named_function_is_local(tree: &Tree, function: NodeId) -> bool {
    let Some(parent) = tree.parent(function) else { return false };
    if is_kt_file(tree, Some(parent)) || is_kind(tree, parent, CLASS_BODY) {
        return false;
    }
    !tree.parent(parent).is_some_and(|g| is_kind(tree, g, SCRIPT))
}

/// `KtNamedFunction.isAnonymous` (KtNamedFunction.kt:168): no name and local, `fun() {}` in an expression.
pub fn kt_named_function_is_anonymous(tree: &Tree, function: NodeId) -> bool {
    get_name(tree, function).is_none() && kt_named_function_is_local(tree, function)
}

/// `KtNamedFunction.isTopLevel` (KtNamedFunction.kt:171): the parent is the file.
pub fn kt_named_function_is_top_level(tree: &Tree, function: NodeId) -> bool {
    is_kt_file(tree, tree.parent(function))
}

/// `KtProperty.isTopLevel` (KtProperty.java:82): the parent is the file.
pub fn kt_property_is_top_level(tree: &Tree, property: NodeId) -> bool {
    is_kt_file(tree, tree.parent(property))
}

/// `KtProperty.isMember` (KtProperty.java:78): it has a containing class, object or script
/// (`containing_class_or_script`); a member of a local class is a member.
pub fn kt_property_is_member(tree: &Tree, property: NodeId) -> bool {
    containing_class_or_script(tree, property).is_some()
}

/// `KtProperty.isLocal` (KtProperty.java:74): neither top-level nor a member.
pub fn kt_property_is_local(tree: &Tree, property: NodeId) -> bool {
    !kt_property_is_top_level(tree, property) && !kt_property_is_member(tree, property)
}

/// `KtVariableDeclaration.isVar`: a `var` child of a property (KtProperty.java:60) or destructuring declaration
/// (KtDestructuringDeclaration.java:98); a destructuring entry's own `var` or its declaration's
/// (KtDestructuringDeclarationEntry.java:114). A parameter's is `kt_parameter_is_mutable`.
pub fn is_var(tree: &Tree, declaration: NodeId) -> bool {
    match tree.node_kind(declaration) {
        Some(PROPERTY | DESTRUCTURING_DECLARATION) => child(tree, declaration, VAR_KEYWORD).is_some(),
        Some(DESTRUCTURING_DECLARATION_ENTRY) => {
            child(tree, declaration, VAR_KEYWORD).is_some()
                || destructuring_entry_parent_node(tree, declaration)
                    .is_some_and(|p| child(tree, p, VAR_KEYWORD).is_some())
        }
        _ => false,
    }
}

/// `KtDestructuringDeclarationEntry.getParentNode` (KtDestructuringDeclarationEntry.java:138): the parent, past the
/// `ERROR_ELEMENT` a top-level destructuring wraps its entries in (KT-58563).
fn destructuring_entry_parent_node(tree: &Tree, entry: NodeId) -> Option<NodeId> {
    let parent = tree.parent(entry)?;
    if is_kind(tree, parent, ERROR_ELEMENT) { tree.parent(parent) } else { Some(parent) }
}

/// `KtValVarKeywordOwner.getValOrVarKeyword`: the first `val` or `var` child of a property (KtProperty.java:301),
/// parameter (KtParameter.java:162) or destructuring declaration (KtDestructuringDeclaration.java:109); a
/// destructuring entry's is its declaration's (KtDestructuringDeclarationEntry.java:151).
pub fn get_val_or_var_keyword(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    const VAL_VAR: TokenSet = TokenSet::create(&[VAL_KEYWORD, VAR_KEYWORD]);
    match tree.node_kind(declaration)? {
        PROPERTY | VALUE_PARAMETER | DESTRUCTURING_DECLARATION => child_in(tree, declaration, &VAL_VAR),
        DESTRUCTURING_DECLARATION_ENTRY => {
            child_in(tree, destructuring_entry_parent_node(tree, declaration)?, &VAL_VAR)
        }
        _ => None,
    }
}

/// `KtDeclarationWithInitializer.getInitializer`: the first `KtExpression` sibling after the `EQ` child of a property
/// (KtProperty.java:265), function (KtNamedFunction.kt:66), accessor (KtPropertyAccessor.kt:109) or destructuring
/// declaration (KtDestructuringDeclaration.java:75), so an error element is skipped and `val w =` has none; a
/// destructuring entry's is the next `REFERENCE_EXPRESSION` after its `EQ` (KtDestructuringDeclarationEntry.java:120).
pub fn get_initializer(tree: &Tree, declaration: NodeId) -> Option<NodeId> {
    match tree.node_kind(declaration)? {
        PROPERTY | FUN | PROPERTY_ACCESSOR | DESTRUCTURING_DECLARATION => {
            next_sibling_expression(tree, child(tree, declaration, EQ))
        }
        DESTRUCTURING_DECLARATION_ENTRY => {
            let mut sibling = tree.next_sibling(child(tree, declaration, EQ)?);
            while let Some(s) = sibling {
                if is_kind(tree, s, REFERENCE_EXPRESSION) {
                    return Some(s);
                }
                sibling = tree.next_sibling(s);
            }
            None
        }
        _ => None,
    }
}

/// `KtProperty.getDelegate` (KtProperty.java:220): the `PROPERTY_DELEGATE` child.
pub fn kt_property_get_delegate(tree: &Tree, property: NodeId) -> Option<NodeId> {
    child(tree, property, PROPERTY_DELEGATE)
}

/// `KtProperty.hasDelegate` (KtProperty.java:210).
pub fn kt_property_has_delegate(tree: &Tree, property: NodeId) -> bool {
    kt_property_get_delegate(tree, property).is_some()
}

/// `KtProperty.getDelegateExpression` (KtProperty.java:239): the delegate's expression
/// (`KtPropertyDelegate.getExpression`, KtPropertyDelegate.java:29, its first `KtExpression` child).
pub fn kt_property_get_delegate_expression(tree: &Tree, property: NodeId) -> Option<NodeId> {
    child_expression(tree, kt_property_get_delegate(tree, property)?)
}

/// `KtProperty.getAccessors` (KtProperty.java:179): the `PROPERTY_ACCESSOR` children.
pub fn kt_property_get_accessors(tree: &Tree, property: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, Some(property), PROPERTY_ACCESSOR)
}

/// `KtProperty.getGetter` (KtProperty.java:184): the first accessor that is a getter.
pub fn kt_property_get_getter(tree: &Tree, property: NodeId) -> Option<NodeId> {
    kt_property_get_accessors(tree, property).find(|&a| kt_property_accessor_is_getter(tree, a))
}

/// `KtProperty.getSetter` (KtProperty.java:193): the first accessor that is a setter.
pub fn kt_property_get_setter(tree: &Tree, property: NodeId) -> Option<NodeId> {
    kt_property_get_accessors(tree, property).find(|&a| kt_property_accessor_is_setter(tree, a))
}

/// `KtPropertyAccessor.isGetter` (KtPropertyAccessor.kt:36): a `get` child.
pub fn kt_property_accessor_is_getter(tree: &Tree, accessor: NodeId) -> bool {
    child(tree, accessor, GET_KEYWORD).is_some()
}

/// `KtPropertyAccessor.isSetter` (KtPropertyAccessor.kt:44): a `set` child.
pub fn kt_property_accessor_is_setter(tree: &Tree, accessor: NodeId) -> bool {
    child(tree, accessor, SET_KEYWORD).is_some()
}

/// `KtParameter.getDefaultValue` (KtParameter.java:126): the first `KtExpression` sibling after the `EQ` child, so
/// `fun f(x: Int = )` has none.
pub fn kt_parameter_get_default_value(tree: &Tree, parameter: NodeId) -> Option<NodeId> {
    next_sibling_expression(tree, Some(child(tree, parameter, EQ)?))
}

/// `KtParameter.hasDefaultValue` (KtParameter.java:117).
pub fn kt_parameter_has_default_value(tree: &Tree, parameter: NodeId) -> bool {
    kt_parameter_get_default_value(tree, parameter).is_some()
}

/// `KtParameter.isMutable` (KtParameter.java:138): a `var` child.
pub fn kt_parameter_is_mutable(tree: &Tree, parameter: NodeId) -> bool {
    child(tree, parameter, VAR_KEYWORD).is_some()
}

/// `KtParameter.isVarArg` (KtParameter.java:147): the `vararg` modifier.
pub fn kt_parameter_is_var_arg(tree: &Tree, parameter: NodeId) -> bool {
    has_modifier(tree, parameter, VARARG_KEYWORD)
}

/// `KtParameter.hasValOrVar` (KtParameter.java:152): a `val` or `var` child (a property declared in a constructor).
pub fn kt_parameter_has_val_or_var(tree: &Tree, parameter: NodeId) -> bool {
    get_val_or_var_keyword(tree, parameter).is_some()
}

/// `KtParameter.getDestructuringDeclaration` (KtParameter.java:171): the `DESTRUCTURING_DECLARATION` child of a lambda
/// parameter `{ (a, b) -> }`.
pub fn kt_parameter_get_destructuring_declaration(tree: &Tree, parameter: NodeId) -> Option<NodeId> {
    child(tree, parameter, DESTRUCTURING_DECLARATION)
}

/// `KtParameter.isLoopParameter` (KtParameter.java:189): the parent is a `FOR`.
pub fn kt_parameter_is_loop_parameter(tree: &Tree, parameter: NodeId) -> bool {
    tree.parent(parameter).is_some_and(|p| is_kind(tree, p, FOR))
}

/// `KtParameter.checkParentOfParentType` (KtParameter.java:193): the parent's parent is of that kind.
fn parameter_list_owner_is(tree: &Tree, parameter: NodeId, kind: NodeKind) -> bool {
    tree.parent(parameter).and_then(|p| tree.parent(p)).is_some_and(|g| is_kind(tree, g, kind))
}

/// `KtParameter.isCatchParameter` (KtParameter.java:202): the list's parent is a `CATCH`.
pub fn kt_parameter_is_catch_parameter(tree: &Tree, parameter: NodeId) -> bool {
    parameter_list_owner_is(tree, parameter, CATCH)
}

/// `KtParameter.isContextParameter` (KtParameter.java:216): the parent is a `CONTEXT_PARAMETER_LIST`.
pub fn kt_parameter_is_context_parameter(tree: &Tree, parameter: NodeId) -> bool {
    tree.parent(parameter).is_some_and(|p| is_kind(tree, p, CONTEXT_PARAMETER_LIST))
}

/// `KtParameter.isLambdaParameter` (KtParameter.java:228): the list's parent is a `FUNCTION_LITERAL`.
pub fn kt_parameter_is_lambda_parameter(tree: &Tree, parameter: NodeId) -> bool {
    parameter_list_owner_is(tree, parameter, FUNCTION_LITERAL)
}

/// `KtParameter.isFunctionTypeParameter` (KtParameter.java:238): the list's parent is a `FUNCTION_TYPE`.
pub fn kt_parameter_is_function_type_parameter(tree: &Tree, parameter: NodeId) -> bool {
    parameter_list_owner_is(tree, parameter, FUNCTION_TYPE)
}

/// `KtTypeAlias.isTopLevel` (KtTypeAlias.kt:35): the parent is the file.
pub fn kt_type_alias_is_top_level(tree: &Tree, alias: NodeId) -> bool {
    is_kt_file(tree, tree.parent(alias))
}

/// `KtTypeParameter.getExtendsBound` (KtTypeParameter.java:65): the `TYPE_REFERENCE` child of `T : Bound`.
pub fn kt_type_parameter_get_extends_bound(tree: &Tree, parameter: NodeId) -> Option<NodeId> {
    child(tree, parameter, TYPE_REFERENCE)
}

/// `KtTypeConstraint.getSubjectTypeParameterName` (KtTypeConstraint.java:43): the `REFERENCE_EXPRESSION` child.
pub fn kt_type_constraint_get_subject_type_parameter_name(tree: &Tree, constraint: NodeId) -> Option<NodeId> {
    child(tree, constraint, REFERENCE_EXPRESSION)
}

/// `KtTypeConstraint.getBoundTypeReference` (KtTypeConstraint.java:49): the `TYPE_REFERENCE` child.
pub fn kt_type_constraint_get_bound_type_reference(tree: &Tree, constraint: NodeId) -> Option<NodeId> {
    child(tree, constraint, TYPE_REFERENCE)
}

/// `KtDestructuringDeclaration.getEntries` (KtDestructuringDeclaration.java:54): the `DESTRUCTURING_DECLARATION_ENTRY`
/// children and those inside an `ERROR_ELEMENT` child, where the parser wraps a top-level `val (a, b) = p` (KT-58563).
pub fn kt_destructuring_declaration_get_entries(tree: &Tree, declaration: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    tree.children(declaration)
        .flat_map(move |c| {
            let (inner, own) = match tree.node_kind(c) {
                Some(ERROR_ELEMENT) => (Some(c), None),
                _ => (None, Some(c)),
            };
            own.into_iter().chain(inner.into_iter().flat_map(move |e| tree.children(e)))
        })
        .filter(move |&c| is_kind(tree, c, DESTRUCTURING_DECLARATION_ENTRY))
}

// ---- KtTypeReference, KtTypeElement, KtNullableType, KtUserType, KtFunctionType -----------------------------------

/// `KtTokenSets.TYPE_ELEMENT_TYPES` (psi/stubs/elements/KtTokenSets.java:25).
fn is_type_element(tree: &Tree, id: NodeId) -> bool {
    matches!(tree.node_kind(id), Some(USER_TYPE | NULLABLE_TYPE | FUNCTION_TYPE | DYNAMIC_TYPE | INTERSECTION_TYPE))
}

/// `KtTypeReference.typeElement` (KtTypeReference.kt:41): the first `USER_TYPE`, `NULLABLE_TYPE`, `FUNCTION_TYPE`,
/// `DYNAMIC_TYPE` or `INTERSECTION_TYPE` child, past the reference's modifier list, the parentheses of `(Foo)` and an
/// error element; None for `fun h(): @A`.
pub fn kt_type_reference_get_type_element(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    tree.children(reference).find(|&c| is_type_element(tree, c))
}

/// `KtTypeReference.hasParentheses` (KtTypeReference.kt:52): an `LPAR` and an `RPAR` child.
pub fn kt_type_reference_has_parentheses(tree: &Tree, reference: NodeId) -> bool {
    child(tree, reference, LPAR).is_some() && child(tree, reference, RPAR).is_some()
}

/// `KtNullableType.getInnerType` (KtNullableType.java:57): the first type element child, past `(`, `)` and the
/// nullable type's own modifier list (`(@A Foo)?`).
pub fn kt_nullable_type_get_inner_type(tree: &Tree, nullable: NodeId) -> Option<NodeId> {
    tree.children(nullable).find(|&c| is_type_element(tree, c))
}

/// `KtTypeElement.unwrapNullability` (psiUtil/ktPsiUtil.kt:895): the type element under every nullable wrapper:
/// `Foo` for `Foo??` and `(Foo?)?`; None when a nullable type has no inner type.
pub fn unwrap_nullability(tree: &Tree, type_element: NodeId) -> Option<NodeId> {
    let mut element = type_element;
    while is_kind(tree, element, NULLABLE_TYPE) {
        element = kt_nullable_type_get_inner_type(tree, element)?;
    }
    Some(element)
}

/// `KtUserType.getReferenceExpression` (KtUserType.java:91): the `REFERENCE_EXPRESSION` child, else the
/// `ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION` of an enum entry's `A(1)`.
pub fn kt_user_type_get_reference_expression(tree: &Tree, user_type: NodeId) -> Option<NodeId> {
    child(tree, user_type, REFERENCE_EXPRESSION)
        .or_else(|| child(tree, user_type, ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION))
}

/// `KtUserType.getQualifier` (KtUserType.java:98): the `USER_TYPE` child, `a.b` of `a.b.C`.
pub fn kt_user_type_get_qualifier(tree: &Tree, user_type: NodeId) -> Option<NodeId> {
    child(tree, user_type, USER_TYPE)
}

/// `KtUserType.getTypeArguments` (KtUserType.java:73): the `TYPE_PROJECTION` children of its own type argument list
/// (each segment of `Outer<A>.Inner<B>` has its own).
pub fn kt_user_type_get_type_arguments(tree: &Tree, user_type: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, child(tree, user_type, TYPE_ARGUMENT_LIST), TYPE_PROJECTION)
}

/// `KtFunctionType.getReceiver` (KtFunctionType.java:82): the `FUNCTION_TYPE_RECEIVER` child of `B.() -> Unit`.
pub fn kt_function_type_get_receiver(tree: &Tree, function_type: NodeId) -> Option<NodeId> {
    child(tree, function_type, FUNCTION_TYPE_RECEIVER)
}

/// `KtFunctionType.getParameterList` (KtFunctionType.java:70): the `VALUE_PARAMETER_LIST` child.
pub fn kt_function_type_get_parameter_list(tree: &Tree, function_type: NodeId) -> Option<NodeId> {
    child(tree, function_type, VALUE_PARAMETER_LIST)
}

/// `KtFunctionType.getParameters` (KtFunctionType.java:76).
pub fn kt_function_type_get_parameters(tree: &Tree, function_type: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, kt_function_type_get_parameter_list(tree, function_type), VALUE_PARAMETER)
}

/// `KtFunctionType.getReturnTypeReference` (KtFunctionType.java:119): the first `TYPE_REFERENCE` child.
pub fn kt_function_type_get_return_type_reference(tree: &Tree, function_type: NodeId) -> Option<NodeId> {
    child(tree, function_type, TYPE_REFERENCE)
}

// ---- KtSimpleNameExpression ---------------------------------------------------------------------------------------

/// `KtSimpleNameExpression.getReferencedNameElement`: a name reference's first `IDENTIFIER`, `this` or `super` child,
/// else the reference itself (KtNameReferenceExpression.kt:83); an enum entry's superclass reference's is the enum
/// class (`KtEnumEntrySuperclassReferenceExpression.getReferencedNameElement` :62: the nearest `ENUM_ENTRY` ancestor's
/// parent's parent, when it is a `CLASS` or `ENUM_ENTRY`; None where the Java's `!!` throws).
pub fn get_referenced_name_element(tree: &Tree, expression: NodeId) -> Option<NodeId> {
    const NAME_REFERENCE_EXPRESSIONS: TokenSet = TokenSet::create(&[IDENTIFIER, THIS_KEYWORD, SUPER_KEYWORD]);
    match tree.node_kind(expression)? {
        REFERENCE_EXPRESSION => Some(child_in(tree, expression, &NAME_REFERENCE_EXPRESSIONS).unwrap_or(expression)),
        ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION => {
            let entry = tree.ancestor_or_self(tree.parent(expression)?, |n| is_kind(tree, n, ENUM_ENTRY))?;
            let class = tree.parent(tree.parent(entry)?)?;
            matches!(tree.node_kind(class), Some(CLASS | ENUM_ENTRY)).then_some(class)
        }
        _ => None,
    }
}

/// `getReferencedName`: a name reference's referenced name element's text without its backticks
/// (`KtSimpleNameExpressionImpl.getReferencedNameImpl`, KtSimpleNameExpression.kt:62, KtPsiUtil.java:152); an enum
/// entry's superclass reference's is the enum class's name or `<no name provided>`
/// (KtEnumEntrySuperclassReferenceExpression.kt:49); a user type's is its reference expression's
/// (KtUserType.java:112): `` b c `` for the middle segment of `` a.`b c`.D ``.
pub fn get_referenced_name<'t>(tree: &'t Tree, element: NodeId) -> Option<Cow<'t, str>> {
    match tree.node_kind(element)? {
        REFERENCE_EXPRESSION => {
            Some(unquote_identifier_or_field_reference(tree.utf8_text(get_referenced_name_element(tree, element)?)?))
        }
        ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION => {
            let class = get_referenced_name_element(tree, element)?;
            Some(unquote_identifier_or_field_reference(get_name(tree, class).unwrap_or(NO_NAME_PROVIDED)))
        }
        USER_TYPE => get_referenced_name(tree, kt_user_type_get_reference_expression(tree, element)?),
        _ => None,
    }
}

// ---- KtImportDirective --------------------------------------------------------------------------------------------

/// `KtImportDirective.getImportedReference` (KtImportDirective.java:49): the first `REFERENCE_EXPRESSION` or
/// `DOT_QUALIFIED_EXPRESSION` child.
pub fn kt_import_directive_get_imported_reference(tree: &Tree, directive: NodeId) -> Option<NodeId> {
    tree.children(directive)
        .find(|&c| matches!(tree.node_kind(c), Some(REFERENCE_EXPRESSION | DOT_QUALIFIED_EXPRESSION)))
}

/// `KtImportDirective.getAlias` (KtImportDirective.java:59): the `IMPORT_ALIAS` child.
pub fn kt_import_directive_get_alias(tree: &Tree, directive: NodeId) -> Option<NodeId> {
    child(tree, directive, IMPORT_ALIAS)
}

/// `KtImportDirective.getAliasName` (KtImportDirective.java:65): the alias's identifier as written, backticks kept.
pub fn kt_import_directive_get_alias_name(tree: &Tree, directive: NodeId) -> Option<&str> {
    get_name(tree, kt_import_directive_get_alias(tree, directive)?)
}

/// `KtImportDirective.isAllUnder` (KtImportDirective.java:71): a `*` child.
pub fn kt_import_directive_is_all_under(tree: &Tree, directive: NodeId) -> bool {
    child(tree, directive, MUL).is_some()
}

/// `KtImportDirective.getImportedFqName` (KtImportDirective.java:90, `fqNameFromExpression` :139): the imported name
/// with each segment's backticks removed, `a.b c.D` for `` import a.`b c`.D ``; a qualified expression without a
/// selector (`import a.`) is its receiver's name; None without a reference or where the Java throws.
pub fn kt_import_directive_get_imported_fq_name(tree: &Tree, directive: NodeId) -> Option<String> {
    fn fq_name_from_expression(tree: &Tree, expression: NodeId, out: &mut String) -> Option<()> {
        // A chain `a.b.c` nests to the left: walk down the receivers, then append the names on the way back up.
        let mut chain = Vec::new();
        let mut e = expression;
        while is_kind(tree, e, DOT_QUALIFIED_EXPRESSION) {
            chain.push(e);
            e = kt_qualified_expression_get_receiver_expression(tree, e)?;
        }
        if !is_kind(tree, e, REFERENCE_EXPRESSION) {
            return None;
        }
        out.push_str(&get_referenced_name(tree, e)?);
        for &q in chain.iter().rev() {
            if let Some(selector) = kt_qualified_expression_get_selector_expression(tree, q) {
                if !matches!(tree.node_kind(selector), Some(REFERENCE_EXPRESSION)) {
                    return None;
                }
                out.push('.');
                out.push_str(&get_referenced_name(tree, selector)?);
            }
        }
        Some(())
    }
    let reference = kt_import_directive_get_imported_reference(tree, directive)?;
    let mut name = String::new();
    fq_name_from_expression(tree, reference, &mut name)?;
    Some(name)
}

// ---- expressions: KtQualifiedExpression, KtCallElement, KtValueArgument, KtLambdaExpression -----------------------

/// `KtQualifiedExpression.operationTokenNodeOrNull` (KtQualifiedExpression.kt:64): the first child in
/// `KtTokens.OPERATIONS` (`.` or `?.`).
fn operation_token_node(tree: &Tree, qualified: NodeId) -> Option<NodeId> {
    child_in(tree, qualified, &OPERATIONS)
}

/// `KtQualifiedExpression.receiverExpressionOrNull` (KtQualifiedExpression.kt:53, `getExpression(false)` :70): the
/// nearest `KtExpression` before the operation token.
pub fn kt_qualified_expression_get_receiver_expression(tree: &Tree, qualified: NodeId) -> Option<NodeId> {
    let mut sibling = tree.previous_sibling(operation_token_node(tree, qualified)?);
    while let Some(s) = sibling {
        if is_kt_expression(tree, s) {
            return Some(s);
        }
        sibling = tree.previous_sibling(s);
    }
    None
}

/// `KtQualifiedExpression.selectorExpression` (KtQualifiedExpression.kt:56): the first `KtExpression` after the
/// operation token, so the error element of an incomplete `a.` is none.
pub fn kt_qualified_expression_get_selector_expression(tree: &Tree, qualified: NodeId) -> Option<NodeId> {
    next_sibling_expression(tree, operation_token_node(tree, qualified))
}

/// `KtCallElement.getCalleeExpression`: a call's `REFERENCE_EXPRESSION` child, else its first `KtExpression` child
/// (KtCallExpression.kt:35); an annotation entry's and a super type call entry's `CONSTRUCTOR_CALLEE` child
/// (KtAnnotationEntry.java:59, KtSuperTypeCallEntry.java:43); a constructor delegation call's
/// `CONSTRUCTOR_DELEGATION_REFERENCE` child (KtConstructorDelegationCall.java:69).
pub fn get_callee_expression(tree: &Tree, call: NodeId) -> Option<NodeId> {
    match tree.node_kind(call)? {
        CALL_EXPRESSION => child(tree, call, REFERENCE_EXPRESSION).or_else(|| child_expression(tree, call)),
        ANNOTATION_ENTRY | SUPER_TYPE_CALL_ENTRY => child(tree, call, CONSTRUCTOR_CALLEE),
        CONSTRUCTOR_DELEGATION_CALL => child(tree, call, CONSTRUCTOR_DELEGATION_REFERENCE),
        _ => None,
    }
}

/// `KtCallElement.getValueArgumentList`: the `VALUE_ARGUMENT_LIST` child of a call (KtCallExpression.kt:40), an
/// annotation entry (KtAnnotationEntry.java:65), a super type call entry (KtSuperTypeCallEntry.java:50) or a
/// constructor delegation call (KtConstructorDelegationCall.java:39).
pub fn get_value_argument_list(tree: &Tree, call: NodeId) -> Option<NodeId> {
    match tree.node_kind(call)? {
        CALL_EXPRESSION | ANNOTATION_ENTRY | SUPER_TYPE_CALL_ENTRY | CONSTRUCTOR_DELEGATION_CALL => {
            child(tree, call, VALUE_ARGUMENT_LIST)
        }
        _ => None,
    }
}

/// `KtCallElement.getLambdaArguments`: a call's `LAMBDA_ARGUMENT` children (KtCallExpression.kt:55), none for the
/// other call elements.
pub fn get_lambda_arguments(tree: &Tree, call: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let owner = (tree.node_kind(call) == Some(CALL_EXPRESSION)).then_some(call);
    children_of(tree, owner, LAMBDA_ARGUMENT)
}

/// `KtCallElement.getValueArguments`: the `VALUE_ARGUMENT`s of the value argument list
/// (`KtValueArgumentList.getArguments`, KtValueArgumentList.java:43), then a call's lambda arguments
/// (KtCallExpression.kt:59).
pub fn get_value_arguments(tree: &Tree, call: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    children_of(tree, get_value_argument_list(tree, call), VALUE_ARGUMENT).chain(get_lambda_arguments(tree, call))
}

/// `KtCallElement.getTypeArgumentList`: a call's `TYPE_ARGUMENT_LIST` child (KtCallExpression.kt:45); an annotation
/// entry's and a super type call entry's are those of their user type (KtAnnotationEntry.java:98,
/// KtSuperTypeCallEntry.java:83); a user type's own (KtUserType.java:68). None for a constructor delegation call.
pub fn get_type_argument_list(tree: &Tree, element: NodeId) -> Option<NodeId> {
    match tree.node_kind(element)? {
        CALL_EXPRESSION | USER_TYPE => child(tree, element, TYPE_ARGUMENT_LIST),
        ANNOTATION_ENTRY | SUPER_TYPE_CALL_ENTRY => {
            let user_type = kt_type_reference_get_type_element(tree, get_type_reference(tree, element)?)?;
            if is_kind(tree, user_type, USER_TYPE) { child(tree, user_type, TYPE_ARGUMENT_LIST) } else { None }
        }
        _ => None,
    }
}

/// `KtValueArgument.getArgumentExpression` (KtValueArgument.java:55): the first `KtExpression` child of a value or
/// lambda argument.
pub fn kt_value_argument_get_argument_expression(tree: &Tree, argument: NodeId) -> Option<NodeId> {
    child_expression(tree, argument)
}

/// `KtValueArgument.getArgumentName` (KtValueArgument.java:85): the `VALUE_ARGUMENT_NAME` child of `name = value`.
pub fn kt_value_argument_get_argument_name(tree: &Tree, argument: NodeId) -> Option<NodeId> {
    child(tree, argument, VALUE_ARGUMENT_NAME)
}

/// `KtValueArgument.isNamed` (KtValueArgument.java:95).
pub fn kt_value_argument_is_named(tree: &Tree, argument: NodeId) -> bool {
    kt_value_argument_get_argument_name(tree, argument).is_some()
}

/// `KtValueArgument.isSpread` (KtValueArgument.java:117): a `*` child.
pub fn kt_value_argument_is_spread(tree: &Tree, argument: NodeId) -> bool {
    child(tree, argument, MUL).is_some()
}

/// `KtLambdaArgument.getLambdaExpression` (KtLambdaArgument.kt:27): the argument's expression unpacked by
/// `unpackFunctionLiteral` (ktElementUtils.kt:19): a lambda itself, the base of a labeled (`l@ { }`) or annotated
/// (`@A { }`) expression; a parenthesized one is none.
pub fn kt_lambda_argument_get_lambda_expression(tree: &Tree, argument: NodeId) -> Option<NodeId> {
    let mut e = kt_value_argument_get_argument_expression(tree, argument)?;
    loop {
        match tree.node_kind(e)? {
            LAMBDA_EXPRESSION => return Some(e),
            LABELED_EXPRESSION | ANNOTATED_EXPRESSION => e = child_expression(tree, e)?,
            _ => return None,
        }
    }
}

/// `KtLambdaExpression.getFunctionLiteral` (KtLambdaExpression.java:42): the `FUNCTION_LITERAL` child.
pub fn kt_lambda_expression_get_function_literal(tree: &Tree, lambda: NodeId) -> Option<NodeId> {
    child(tree, lambda, FUNCTION_LITERAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_sets_are_in_token_id_order() {
        // TokenSet.getTypes() lists a set by element type index, which for Kotlin's tokens is the KtTokens id order.
        let visibility: Vec<TokenKind> = crate::tokens::VISIBILITY_MODIFIERS.iter().collect();
        assert_eq!(visibility, VISIBILITY_MODIFIERS_TYPES);
        let modality: Vec<TokenKind> = crate::tokens::MODALITY_MODIFIERS.iter().collect();
        assert_eq!(modality, MODALITY_MODIFIERS_TYPES);
    }

    #[test]
    fn unquotes_as_kt_psi_util() {
        assert_eq!(unquote_identifier("`a b`"), "a b");
        assert_eq!(unquote_identifier("`"), "`");
        assert_eq!(unquote_identifier("``"), "");
        assert_eq!(unquote_identifier("`a"), "`a");
        assert_eq!(unquote_identifier("plain"), "plain");
        assert_eq!(unquote_identifier_or_field_reference("$`a`"), "$a");
        assert_eq!(unquote_identifier_or_field_reference("$a"), "$a");
        assert_eq!(unquote_identifier_or_field_reference("`$a`"), "$a");
        assert_eq!(unquote_identifier_or_field_reference("$`a"), "$`a");
    }
}
