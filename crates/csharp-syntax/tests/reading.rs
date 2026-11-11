//! The reading API every consumer of a C# tree shares (`psi_builder::Tree` and `read`): what is trivia, what is a
//! documentation comment, what a declaration's own range is, and where the name a token spells is written. `CSharp`
//! answers them, so that no consumer decides for itself that a `;` is a separator or looks for a name in the line.

use csharp_syntax::{DocumentationMode, Entry, FileKind, NodeId, NodeKind, TokenKind, Tree, read};
use psi_check::{find, first, own_text};

/// As a consumer that reads declarations parses a file: the documentation comments left unparsed.
fn parse(text: &str) -> Tree {
    Tree::parse(text, FileKind { doc_mode: DocumentationMode::None, ..FileKind::default() })
}

/// Roslyn puts a field's and a method's `;` inside the declaration, where it is its own text, and the `,` between
/// enum members in the list around them: C# has no separator to drop, and a consumer that dropped a trailing `;`
/// took text that belonged to the declaration.
#[test]
fn a_declaration_owns_its_semicolon_and_not_the_trivia_around_it() {
    let tree = parse("class C {\n  /// <summary>x</summary>\n  // note\n  int f;\n  void M();\n  enum E { X, Y }\n}\n");
    assert_eq!(own_text(&tree, NodeKind::FieldDeclaration), "int f;");
    assert_eq!(own_text(&tree, NodeKind::MethodDeclaration), "void M();");
    assert_eq!(own_text(&tree, NodeKind::EnumMemberDeclaration), "X");
}

/// A documentation comment carries no code, parsed into nodes or left a comment by `DocumentationMode::None`, which
/// is how a consumer that only reads declarations parses a file.
#[test]
fn a_documentation_comment_is_trivia_however_it_is_parsed() {
    let unparsed = parse("/// <summary>x</summary>\nclass C { }\n");
    let comment = find(&unparsed, |id| unparsed.token_kind(id) == Some(TokenKind::SingleLineCommentTrivia))
        .expect("the comment");
    assert!(unparsed.is_trivia(comment) && unparsed.is_doc_comment(comment), "`///` is a documentation comment");
    let plain = parse("// note\nclass C { }\n");
    let note =
        find(&plain, |id| plain.token_kind(id) == Some(TokenKind::SingleLineCommentTrivia)).expect("the comment");
    assert!(plain.is_trivia(note) && !plain.is_doc_comment(note), "a `//` comment is no documentation comment");

    let parsed = Tree::parse(
        "/// <summary>x</summary>\nclass C { }\n",
        FileKind { doc_mode: DocumentationMode::Parse, ..FileKind::default() },
    );
    let trivia = find(&parsed, |id| parsed.is_doc_comment(id)).expect("the documentation trivia");
    assert_eq!(parsed.node_kind(trivia), Some(NodeKind::SingleLineDocumentationCommentTrivia));
    assert!(parsed.is_trivia(trivia));
    let summary = find(&parsed, |id| parsed.node_kind(id) == Some(NodeKind::XmlElement)).expect("the `summary`");
    assert!(parsed.in_doc_comment(summary), "the XML of a documentation comment is of the comment");
    assert!(!parsed.in_doc_comment(first(&parsed, NodeKind::ClassDeclaration)));
}

/// `read::word_at` places the name a token spells at the text that writes it: past the `@` of a verbatim identifier,
/// and at the token itself where an escape makes the value bytes the source does not hold.
#[test]
fn a_name_is_placed_at_the_text_that_writes_it() {
    let tree = Tree::parse(
        "class C { int @count; int C\\u733f; }\n",
        FileKind { entry: Entry::CompilationUnit, doc_mode: DocumentationMode::None, ..FileKind::default() },
    );
    let names: Vec<(&str, usize, &str)> = (0..tree.len() as u32)
        .map(NodeId)
        .filter(|&id| tree.token_kind(id) == Some(TokenKind::IdentifierToken))
        .filter_map(|id| {
            let (value, at) = read::word_at(&tree, id)?;
            Some((value, at, std::str::from_utf8(&tree.source()[tree.range(id)]).ok()?))
        })
        .collect();
    assert_eq!(names, [("C", 6, "C"), ("count", 15, "@count"), ("C猿", 26, "C\\u733f")], "{names:?}");
}
