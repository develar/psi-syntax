//! The reading API every consumer of a Kotlin tree shares (`psi_builder::Tree`): what is trivia, what is a doc
//! comment, what lies inside one, and what a declaration's own range is. `Kotlin` answers them, so that no consumer
//! keeps a list of Kotlin's comment tokens or decides for itself which trailing token a declaration owns.

use kotlin_syntax::{FileKind, NodeKind, TokenKind, Tree};
use psi_check::{find, first, own_text};

fn parse(text: &str) -> Tree {
    Tree::parse(text, FileKind::Kt)
}

/// An enum entry holds the `,` or the `;` that ends it, and that token separates it from the next entry; a property
/// holds the `;` after it, which is its own text, and a value parameter's `,` is its list's. The KDoc and the
/// comments a binder put inside a declaration at its edges are not its own.
#[test]
fn an_enum_entry_drops_its_separator_and_a_property_keeps_its_semicolon() {
    let tree = parse("enum class E { X, Y; }\n");
    assert_eq!(own_text(&tree, NodeKind::ENUM_ENTRY), "X");

    let tree = parse("class C {\n  /** doc */\n  // note\n  val a = 1;\n}\n");
    assert_eq!(own_text(&tree, NodeKind::PROPERTY), "val a = 1;");

    let tree = parse("class C(\n  val a: Int,\n  val b: Int,\n)\n");
    assert_eq!(own_text(&tree, NodeKind::VALUE_PARAMETER), "val a: Int");
}

/// A KDoc carries no code, and the names it holds are of the comment: a consumer that reads the code of a file asks
/// `in_doc_comment`.
#[test]
fn a_kdoc_is_trivia_and_what_it_holds_lies_in_it() {
    let tree = parse("/** [C] */\nclass C {\n  fun m() {}\n}\n");
    let doc = find(&tree, |id| tree.is_doc_comment(id)).expect("a KDoc");
    assert!(tree.is_trivia(doc), "a KDoc carries no code");
    assert_eq!(tree.kind(doc).token(), Some(TokenKind::DOC_COMMENT));

    let name = first(&tree, NodeKind::KDOC_NAME);
    assert!(tree.in_doc_comment(name), "the `C` of a link is of the comment, not of the code");

    let class = first(&tree, NodeKind::CLASS);
    assert!(!tree.is_trivia(class) && !tree.in_doc_comment(class));
}

/// Every comment of Kotlin is trivia, the shebang of a script among them, and `is_trivia` says so without a list of
/// the comment tokens on the consumer's side.
#[test]
fn every_comment_of_kotlin_is_trivia() {
    let tree = Tree::parse("#!/usr/bin/env kotlin\n// line\n/* block */\n/** doc */\nfun m() {}\n", FileKind::Kts);
    for token in [TokenKind::SHEBANG_COMMENT, TokenKind::EOL_COMMENT, TokenKind::BLOCK_COMMENT, TokenKind::DOC_COMMENT]
    {
        let id = find(&tree, |id| tree.kind(id).token() == Some(token)).unwrap_or_else(|| panic!("no {token:?}"));
        assert!(tree.is_trivia(id), "{token:?}");
    }
    let function = first(&tree, NodeKind::FUN);
    assert_eq!(own_text(&tree, NodeKind::FUN), "fun m() {}", "the comments bound into it are not its own");
    assert!(!tree.is_trivia(function));
}
