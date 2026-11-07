use starlark_syntax::TokenKind as T;
use starlark_syntax::lexer::tokenize;

fn token_at(lexed: &starlark_syntax::lexer::StarlarkLexed, kind: T, start: usize) -> usize {
    lexed
        .types
        .iter()
        .enumerate()
        .find_map(|(index, token)| (*token == kind && lexed.starts[index] == start as u32).then_some(index))
        .unwrap()
}

#[test]
fn plain_marks_code_only_when_the_initial_lexer_state_is_empty() {
    let lexed = tokenize(b"  x\n  y\nz");
    assert!(!lexed.is_plain(token_at(&lexed, T::IDENTIFIER, 2)));
    assert!(!lexed.is_plain(token_at(&lexed, T::IDENTIFIER, 6)));
    assert!(!lexed.is_plain(token_at(&lexed, T::DEDENT, 7)));
    assert!(lexed.is_plain(token_at(&lexed, T::IDENTIFIER, 8)));
}
