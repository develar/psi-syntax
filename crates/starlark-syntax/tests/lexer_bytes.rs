use starlark_syntax::lexer::lex;

#[test]
fn arbitrary_bytes_are_lossless() {
    for len in 0..=256 {
        let source: Vec<u8> = (0..len).map(|index| (index * 37) as u8).collect();
        let mut previous_end = 0;
        for token in lex(&source) {
            assert!(token.start >= previous_end, "out-of-order token {token:?}");
            assert!(token.end >= token.start, "negative token range {token:?}");
            previous_end = token.end;
        }
        assert_eq!(previous_end, source.len());
    }
}
