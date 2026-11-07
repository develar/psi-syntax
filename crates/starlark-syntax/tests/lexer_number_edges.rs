use starlark_syntax::TokenKind as T;
use starlark_syntax::lexer::lex;

fn kinds(source: &[u8]) -> Vec<T> {
    lex(source).into_iter().map(|token| token.kind).collect()
}

#[test]
fn incomplete_hex_and_octal_prefixes_fall_back_to_zero() {
    assert_eq!(kinds(b"0x"), [T::INT, T::IDENTIFIER, T::STATEMENT_BREAK]);
    assert_eq!(kinds(b"0X"), [T::INT, T::IDENTIFIER, T::STATEMENT_BREAK]);
    assert_eq!(kinds(b"0o"), [T::INT, T::IDENTIFIER, T::STATEMENT_BREAK]);
    assert_eq!(kinds(b"0o8"), [T::INT, T::IDENTIFIER, T::STATEMENT_BREAK]);
}
