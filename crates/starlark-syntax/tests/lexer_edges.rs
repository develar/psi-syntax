use starlark_syntax::TokenKind as T;
use starlark_syntax::lexer::lex;

#[test]
fn trailing_escape_is_not_part_of_an_unterminated_string() {
    let source = br#"x="a\"#;
    let kinds: Vec<T> = lex(source).into_iter().map(|token| token.kind).collect();
    assert_eq!(kinds, [T::IDENTIFIER, T::EQ, T::STRING, T::BAD_CHARACTER, T::STATEMENT_BREAK]);
}
