//! The reading API every consumer of a Java tree shares (`psi_builder::Tree`): what is trivia, what is a doc comment,
//! what lies inside one, and what a declaration's own range is. `Java` answers them from the PSI, so that no consumer
//! keeps a list of Java's comment tokens or of the node types a doc comment holds.

use java_syntax::{FileKind, NodeKind, Tree, TokenKind, psi};
use psi_check::{find, first, own_text};

fn parse(text: &str) -> Tree {
    Tree::parse(text, FileKind::default())
}

/// The `;` the parser puts inside a field and an abstract method is their own text, as `PsiField.getTextRange()`
/// holds it; the `,` between enum constants is the class's, not the constant's, so there is nothing to drop there.
/// The doc comment and the comments a binder put inside a declaration at its edges are not its own.
#[test]
fn a_declaration_owns_its_semicolon_and_not_the_comment_bound_into_it() {
    let tree = parse("class A {\n  /** doc */\n  // note\n  int f;\n  enum E { X, Y; }\n}\n");
    assert_eq!(own_text(&tree, NodeKind::FIELD), "int f;");
    assert_eq!(own_text(&tree, NodeKind::ENUM_CONSTANT), "X");

    let abstract_method = parse("interface I {\n  void m();\n}\n");
    assert_eq!(own_text(&abstract_method, NodeKind::METHOD), "void m();");
}

/// A doc comment carries no code, and the references, the types and the tags it holds are of the comment: a consumer
/// that reads the code of a file asks `in_doc_comment` instead of listing the node types javadoc puts inside itself.
#[test]
fn a_doc_comment_is_trivia_and_what_it_holds_lies_in_it() {
    let tree = parse("/** {@link A#m} */\nclass A {\n  void m() {}\n}\n");
    let doc = find(&tree, |id| tree.is_doc_comment(id)).expect("a doc comment");
    assert!(tree.is_trivia(doc), "a doc comment carries no code");
    assert_eq!(tree.kind(doc).token(), Some(TokenKind::DOC_COMMENT));

    let reference = first(&tree, NodeKind::JAVA_CODE_REFERENCE);
    assert!(tree.in_doc_comment(reference), "the `A` of a link is of the comment, not of the code");

    let class = first(&tree, NodeKind::CLASS);
    assert!(!tree.is_trivia(class) && !tree.in_doc_comment(class));
    let name = psi::get_name_identifier(&tree, class).expect("the class's name");
    assert!(!tree.in_doc_comment(name));
}

/// `PsiJavaCodeReferenceElement.getReferenceNameElement()` of any reference, dispatched on the element type: a
/// consumer asks one function instead of matching the four classes that answer it.
#[test]
fn a_reference_is_named_by_its_name_element_whatever_its_class() {
    for (text, kind, name) in [
        ("import static java.util.Map.entry;\n", NodeKind::IMPORT_STATIC_REFERENCE, "entry"),
        ("class A {\n  int m(A a) { return a.size(); }\n}\n", NodeKind::REFERENCE_EXPRESSION, "size"),
        ("class A extends java.util.ArrayList {\n}\n", NodeKind::JAVA_CODE_REFERENCE, "ArrayList"),
        ("class A {\n  Runnable r = A::m;\n  void m() {}\n}\n", NodeKind::METHOD_REF_EXPRESSION, "m"),
    ] {
        let tree = parse(text);
        let reference = first(&tree, kind);
        let element = psi::get_reference_name_element(&tree, reference).unwrap_or_else(|| panic!("{kind:?}"));
        assert_eq!(tree.utf8_text(element), Some(name), "{kind:?}");
    }
    let tree = parse("class A {}\n");
    assert_eq!(psi::get_reference_name_element(&tree, first(&tree, NodeKind::CLASS)), None, "a class is no reference");
}
