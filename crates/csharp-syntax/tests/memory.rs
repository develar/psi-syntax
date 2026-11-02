//! A parse's memory is bounded by its input: past `green::budget_for` the parse is abandoned as Roslyn abandons one
//! whose stack runs out (the whole text one skipped `BadToken`, `ERR_InsufficientStack`), and the tree stays lossless.

use csharp_syntax::{ErrorCode, FileKind, Parser};

const TEXT: &str = "class C { void M() { var x = a + b * (c - d); if (x > 0) { M(); } } }\n";

fn source(n: usize) -> String {
    TEXT.repeat(n)
}

#[test]
fn a_parse_past_its_arena_budget_is_abandoned_like_an_exhausted_stack() {
    let text = source(2000);
    let mut parser = Parser::new();
    parser.set_arena_budget(64 << 10);
    let tree = parser.parse(&text, FileKind::default());
    let codes: Vec<ErrorCode> = tree.diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(codes, [ErrorCode::ERR_InsufficientStack]);
    let leaves: Vec<_> = tree.leaves().map(|(_, r)| r).collect();
    let mut at = 0;
    for r in &leaves {
        assert_eq!(r.start, at, "leaves tile the text");
        at = r.end;
    }
    assert_eq!(at, text.len());
}

#[test]
fn the_default_budget_leaves_an_ordinary_parse_alone() {
    let text = source(2000);
    let tree = Parser::new().parse(&text, FileKind::default());
    assert!(tree.diagnostics().iter().all(|d| d.code != ErrorCode::ERR_InsufficientStack));
}

#[test]
fn the_budget_is_reset_for_the_next_file() {
    let mut parser = Parser::new();
    parser.set_arena_budget(64 << 10);
    let big = parser.parse(source(2000), FileKind::default());
    assert!(big.diagnostics().iter().any(|d| d.code == ErrorCode::ERR_InsufficientStack));
    let small = parser.parse(TEXT, FileKind::default());
    assert!(small.diagnostics().iter().all(|d| d.code != ErrorCode::ERR_InsufficientStack));
}
