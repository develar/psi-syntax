//! `PsiJavaCodeReferenceElementImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiJavaCodeReferenceElementImpl.java`)
//! over a `JAVA_CODE_REFERENCE`: a type or package name (`a.b.C<T>`), its qualifier the nested reference (`a.b`), its
//! name the last identifier, its type arguments the trailing `REFERENCE_PARAMETER_LIST`. A type annotation before it
//! or after a dot (`extends @A Base`, `java.util.@T List`) is an `ANNOTATION` child, never its qualifier.

use super::{find_child_backward, find_child_by_type, import_statement, new_expression};
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::tokens::{ElementType, TokenKind};
use crate::tree::{NodeId, Tree};

/// `PsiJavaCodeReferenceElementImpl.Kind`: how a reference resolves, by where it stands.
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    CLASS_NAME_KIND,
    PACKAGE_NAME_KIND,
    CLASS_OR_PACKAGE_NAME_KIND,
    CLASS_FQ_NAME_KIND,
    CLASS_FQ_OR_PACKAGE_NAME_KIND,
    CLASS_IN_QUALIFIED_NEW_KIND,
}

/// `getReferenceNameElement()` (`PsiJavaCodeReferenceElementImpl.java:1011`, `findChildByRole(REFERENCE_NAME)` `:299`):
/// the last `IDENTIFIER` child (`TreeUtil.findChildBackward`).
pub fn get_reference_name_element(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    find_child_backward(tree, reference, TokenKind::IDENTIFIER)
}

/// `getQualifier()` (`PsiJavaCodeReferenceElementImpl.java:894`, `findChildByRole(QUALIFIER)` `:306`): the first
/// `JAVA_CODE_REFERENCE` child.
pub fn get_qualifier(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    find_child_by_type(tree, reference, NodeKind::JAVA_CODE_REFERENCE)
}

/// `isQualified()` (`PsiJavaCodeReferenceElementImpl.java:889`): it has a qualifier.
pub fn is_qualified(tree: &Tree, reference: NodeId) -> bool {
    get_qualifier(tree, reference).is_some()
}

/// `getParameterList()` (`PsiJavaCodeReferenceElementImpl.java:1021`, `findChildByRole(REFERENCE_PARAMETER_LIST)`
/// `:302`): the last child when it is a `REFERENCE_PARAMETER_LIST`.
pub fn get_parameter_list(tree: &Tree, reference: NodeId) -> Option<NodeId> {
    super::last_child(tree, reference).filter(|&c| tree.node_kind(c) == Some(NodeKind::REFERENCE_PARAMETER_LIST))
}

/// `getKindEnum(containingFile)` (`PsiJavaCodeReferenceElementImpl.java:148`), by the parent:
/// - a `TYPE`: `CLASS_NAME_KIND`;
/// - a reference list (extends, implements, permits, a bound, throws, provides with), `Outer.this`, `Iface.super`, a
///   doc comment's method or field reference and tag value, type arguments, an annotation, `uses`, `provides`:
///   `CLASS_OR_PACKAGE_NAME_KIND` when qualified, else `CLASS_NAME_KIND`;
/// - `new`, or the base of an anonymous class's `new`: `CLASS_IN_QUALIFIED_NEW_KIND` when the `new` has a qualifier
///   (`o.new I()`), else `CLASS_NAME_KIND`; another reference of an anonymous class `CLASS_OR_PACKAGE_NAME_KIND`;
/// - `package`, `exports`, `opens`: `PACKAGE_NAME_KIND`;
/// - `import`: `CLASS_FQ_OR_PACKAGE_NAME_KIND` on demand, else `CLASS_FQ_NAME_KIND`; `import static`, a static
///   import's reference: `CLASS_FQ_OR_PACKAGE_NAME_KIND`;
/// - a reference: its kind, `CLASS_NAME_KIND` widened to `CLASS_OR_PACKAGE_NAME_KIND` and `CLASS_FQ_NAME_KIND` to
///   `CLASS_FQ_OR_PACKAGE_NAME_KIND`;
/// - a class, a parameter list, an error element: `CLASS_OR_PACKAGE_NAME_KIND`;
/// - a doc tag, an inline doc tag, a doc reference or type holder: `CLASS_OR_PACKAGE_NAME_KIND`, but
///   `CLASS_FQ_OR_PACKAGE_NAME_KIND` in a doc comment of a `package-info.java` (`in_package_info`) below Java 8
///   (`JavaFeature.PACKAGE_INFO_DOC_IMPORTS`, the tree's level);
/// - anything else (where IntelliJ logs an error), and the dummy holders of code fragments: `CLASS_NAME_KIND`.
///
/// Iterative over nested references.
pub fn get_kind_enum(tree: &Tree, reference: NodeId, in_package_info: bool) -> Kind {
    let mut outermost = reference;
    let mut nested = false;
    while let Some(parent) =
        tree.parent(outermost).filter(|&p| tree.node_kind(p) == Some(NodeKind::JAVA_CODE_REFERENCE))
    {
        outermost = parent;
        nested = true;
    }
    let kind = own_kind_enum(tree, outermost, in_package_info);
    if !nested {
        return kind;
    }
    match kind {
        Kind::CLASS_NAME_KIND => Kind::CLASS_OR_PACKAGE_NAME_KIND,
        Kind::CLASS_FQ_NAME_KIND => Kind::CLASS_FQ_OR_PACKAGE_NAME_KIND,
        kind => kind,
    }
}

/// `getKindEnum` of a reference whose parent is no reference.
fn own_kind_enum(tree: &Tree, reference: NodeId, in_package_info: bool) -> Kind {
    let Some(parent) = tree.parent(reference) else { return Kind::CLASS_NAME_KIND };
    let qualified = || {
        if is_qualified(tree, reference) { Kind::CLASS_OR_PACKAGE_NAME_KIND } else { Kind::CLASS_NAME_KIND }
    };
    let in_qualified_new = |new: NodeId| {
        if new_expression::find_qualifier(tree, new).is_some() {
            Kind::CLASS_IN_QUALIFIED_NEW_KIND
        } else {
            Kind::CLASS_NAME_KIND
        }
    };
    match tree.kind(parent) {
        ElementType::Node(node) => match node {
            NodeKind::TYPE => Kind::CLASS_NAME_KIND,
            NodeKind::EXTENDS_LIST
            | NodeKind::IMPLEMENTS_LIST
            | NodeKind::PERMITS_LIST
            | NodeKind::EXTENDS_BOUND_LIST
            | NodeKind::THROWS_LIST
            | NodeKind::THIS_EXPRESSION
            | NodeKind::SUPER_EXPRESSION
            | NodeKind::DOC_METHOD_OR_FIELD_REF
            | NodeKind::DOC_TAG_VALUE_ELEMENT
            | NodeKind::REFERENCE_PARAMETER_LIST
            | NodeKind::ANNOTATION
            | NodeKind::USES_STATEMENT
            | NodeKind::PROVIDES_STATEMENT
            | NodeKind::PROVIDES_WITH_LIST => qualified(),
            NodeKind::NEW_EXPRESSION => in_qualified_new(parent),
            NodeKind::ANONYMOUS_CLASS => {
                // `getChildRole(this) == BASE_CLASS_REFERENCE`: the reference is the anonymous class's first child.
                if tree.first_child(parent) == Some(reference) {
                    match tree.parent(parent) {
                        Some(new) if tree.node_kind(new) == Some(NodeKind::NEW_EXPRESSION) => in_qualified_new(new),
                        _ => Kind::CLASS_NAME_KIND,
                    }
                } else {
                    Kind::CLASS_OR_PACKAGE_NAME_KIND // incomplete code
                }
            }
            NodeKind::PACKAGE_STATEMENT | NodeKind::EXPORTS_STATEMENT | NodeKind::OPENS_STATEMENT => {
                Kind::PACKAGE_NAME_KIND
            }
            NodeKind::IMPORT_STATEMENT => {
                if import_statement::is_on_demand(tree, parent) {
                    Kind::CLASS_FQ_OR_PACKAGE_NAME_KIND
                } else {
                    Kind::CLASS_FQ_NAME_KIND
                }
            }
            NodeKind::IMPORT_STATIC_STATEMENT | NodeKind::IMPORT_STATIC_REFERENCE => {
                Kind::CLASS_FQ_OR_PACKAGE_NAME_KIND
            }
            NodeKind::CLASS | NodeKind::PARAMETER_LIST | NodeKind::ERROR_ELEMENT => Kind::CLASS_OR_PACKAGE_NAME_KIND,
            NodeKind::DOC_TAG | NodeKind::DOC_INLINE_TAG => doc_kind(tree, in_package_info),
            _ => Kind::CLASS_NAME_KIND,
        },
        ElementType::Token(TokenKind::DOC_REFERENCE_HOLDER | TokenKind::DOC_TYPE_HOLDER) => {
            doc_kind(tree, in_package_info)
        }
        ElementType::Token(_) => Kind::CLASS_NAME_KIND,
    }
}

/// The kind of a reference in a doc tag or holder (`PsiJavaCodeReferenceElementImpl.java:224`).
fn doc_kind(tree: &Tree, in_package_info: bool) -> Kind {
    if in_package_info && tree.file_kind().level.is_less_than(LanguageLevel::JDK_1_8) {
        Kind::CLASS_FQ_OR_PACKAGE_NAME_KIND
    } else {
        Kind::CLASS_OR_PACKAGE_NAME_KIND
    }
}
