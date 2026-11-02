//! Green trees built by hand, flattened and dumped: positions as Roslyn's red tree computes them (missing tokens,
//! fixed-text tokens, skipped tokens as structured trivia), leading and trailing trivia, diagnostics in Roslyn's order
//! and at their offsets.

use csharp_syntax::dump::{index_line, lex_to_string, tree_to_string};
use csharp_syntax::green::{DiagArg, Green, GreenArena, TokenText, TriviaList};
use csharp_syntax::{ErrorCode, FileKind, NodeKind, SyntaxKind, TokenKind, Tree};

fn token(
    arena: &mut GreenArena,
    kind: SyntaxKind,
    start: u32,
    width: u32,
    leading: TriviaList,
    trailing: TriviaList,
) -> Green {
    arena.create_token(leading, kind, kind, TokenText::Source { start, width }, trailing)
}

/// `x = y \n` with the semicolon missing, then `@` skipped into the end of file's leading trivia.
const TEXT: &[u8] = b"x = y \n@";

fn build(arena: &mut GreenArena) -> Green {
    let space = |a: &mut GreenArena, at| {
        let t = a.trivia(SyntaxKind::WhitespaceTrivia, at, 1);
        a.trivia_list(&[t])
    };
    let s1 = space(arena, 1);
    let x = token(arena, SyntaxKind::IdentifierToken, 0, 1, TriviaList::EMPTY, s1);
    let s3 = space(arena, 3);
    let eq = arena.token_with_trivia(TriviaList::EMPTY, SyntaxKind::EqualsToken, s3);
    let ws = arena.trivia(SyntaxKind::WhitespaceTrivia, 5, 1);
    let nl = arena.trivia(SyntaxKind::EndOfLineTrivia, 6, 1);
    let trailing = arena.trivia_list(&[ws, nl]);
    let y = token(arena, SyntaxKind::IdentifierToken, 4, 1, TriviaList::EMPTY, trailing);
    let left = arena.identifier_name(x);
    let right = arena.identifier_name(y);
    let assign = arena.assignment_expression(SyntaxKind::SimpleAssignmentExpression, left, eq, right);
    let semi = arena.missing_token(SyntaxKind::SemicolonToken);
    let error = arena.make_diagnostic(0, 0, ErrorCode::ERR_SemicolonExpected, &[]);
    let semi = arena.with_additional_diagnostics(semi, &[error]);
    let statement = arena.expression_statement(Green::NULL, assign, semi);
    let global = arena.global_statement(Green::NULL, Green::NULL, statement);
    let bad = token(arena, SyntaxKind::BadToken, 7, 1, TriviaList::EMPTY, TriviaList::EMPTY);
    let unexpected = arena.make_diagnostic(0, 1, ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::TokenText(bad)]);
    let bad = arena.with_additional_diagnostics(bad, &[unexpected]);
    let skipped = arena.skipped_tokens_trivia(bad);
    let skipped = arena.structured_trivia(skipped);
    let leading = arena.trivia_list(&[skipped]);
    let eof = arena.token_with_trivia(leading, SyntaxKind::EndOfFileToken, TriviaList::EMPTY);
    let members = arena.list(&[global]);
    arena.compilation_unit(Green::NULL, Green::NULL, Green::NULL, members, eof)
}

#[test]
fn flattens_as_the_red_tree() {
    let mut arena = GreenArena::new();
    let root = build(&mut arena);
    let tree = Tree::from_green(&arena, root, TEXT, FileKind::default());
    let expected = "\
CompilationUnit [0..8)
  GlobalStatement [0..7)
    ExpressionStatement [0..7)
      SimpleAssignmentExpression [0..7)
        IdentifierName [0..2)
          IdentifierToken [0..1) 'x'
          T:WhitespaceTrivia [1..2) ' '
        EqualsToken [2..3) '='
        T:WhitespaceTrivia [3..4) ' '
        IdentifierName [4..7)
          IdentifierToken [4..5) 'y'
          T:WhitespaceTrivia [5..6) ' '
          T:EndOfLineTrivia [6..7) '\\n'
      SemicolonToken [7..7) missing
  L:SkippedTokensTrivia [7..8)
    BadToken [7..8) '@'
  EndOfFileToken [8..8) ''
Diagnostics:
  CS1002 [7..7) ; expected
  CS1056 [7..8) Unexpected character \\'@\\'
";
    assert_eq!(tree_to_string(&tree), expected);
    // the tree's own view
    assert_eq!(tree.kind(tree.root()), csharp_syntax::psi_builder::ElementType::Node(NodeKind::CompilationUnit));
    let missing: Vec<_> = tree.descendants(tree.root()).filter(|&id| tree.is_missing(id)).collect();
    assert_eq!(missing.len(), 1);
    assert_eq!(tree.error_message(missing[0]), Some("; expected"));
    let leaves: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).to_vec()).collect();
    assert_eq!(leaves, TEXT);
    assert!(tree.leaves().any(|(k, _)| k == TokenKind::BadToken));
    assert_eq!(index_line("a/b.cs", &tree), r#"{"path":"a/b.cs","errors":2,"firstError":"CS1002 [7..7) ; expected"}"#);
}

#[test]
fn lexed_tokens_dump_at_depth_zero() {
    let mut arena = GreenArena::new();
    let nl = arena.trivia(SyntaxKind::EndOfLineTrivia, 1, 1);
    let trailing = arena.trivia_list(&[nl]);
    let a = token(&mut arena, SyntaxKind::IdentifierToken, 0, 1, TriviaList::EMPTY, trailing);
    let eof = token(&mut arena, SyntaxKind::EndOfFileToken, 2, 0, TriviaList::EMPTY, TriviaList::EMPTY);
    let tree = Tree::from_tokens(&arena, &[a, eof], b"a\n", FileKind::default());
    assert_eq!(
        lex_to_string(&tree),
        "IdentifierToken [0..1) 'a'\nT:EndOfLineTrivia [1..2) '\\n'\nEndOfFileToken [2..2) ''\n"
    );
    assert_eq!(csharp_syntax::dump::lexed_tokens(&tree), 2);
}
