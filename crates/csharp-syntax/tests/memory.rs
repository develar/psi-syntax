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

/// Where a parse is abandoned depends on its file alone: a parser whose buffers kept the capacity of a bigger file
/// checks the budget where a fresh one does (`green::BudgetMarks`), so it abandons, or keeps, the same files.
#[test]
fn the_budget_trips_where_it_trips_on_a_fresh_parser() {
    let big = source(20_000);
    let mut kept = Parser::new();
    kept.set_retained_bytes(usize::MAX);
    kept.parse(&big, FileKind::default());
    let text = source(2000);
    let abandons = |budget: usize, parser: &mut Parser| {
        parser.set_arena_budget(budget);
        let tree = parser.parse(&text, FileKind::default());
        tree.diagnostics().iter().any(|d| d.code == ErrorCode::ERR_InsufficientStack)
    };
    // the smallest budget that keeps the whole file, as fresh parsers find it
    let (mut low, mut high) = (64usize << 10, 64usize << 20);
    assert!(abandons(low, &mut Parser::new()) && !abandons(high, &mut Parser::new()));
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if abandons(mid, &mut Parser::new()) { low = mid } else { high = mid }
    }
    for budget in [low / 2, low, high, high * 2] {
        let mut fresh = Parser::new();
        fresh.set_arena_budget(budget);
        kept.set_arena_budget(budget);
        let (a, b) = (fresh.parse(&text, FileKind::default()), kept.parse(&text, FileKind::default()));
        assert_eq!(csharp_syntax::dump::tree_to_string(&a), csharp_syntax::dump::tree_to_string(&b), "budget {budget}");
    }
    assert!(abandons(low, &mut kept) && !abandons(high, &mut kept));
}

/// A file of every kind of buffer the parser owns: directives, documentation comments, interpolated and raw strings
/// with holes, attributes, generics, lambdas, queries.
const MIXED: &str = r#"#define X
#region R
/// <summary>A <see cref="List{T}"/> &amp; <c>code</c>.</summary>
/// <param name="a">the a</param>
[Obsolete("no")]
partial class C<T> where T : class {
#if X
    async Task<int> M(int a) { var s = $"{a,3:X}{$"{a}"}" + """raw {x}""" + $$"""{{a}}"""; return await F(x => x * 2, from y in z select y); }
#else
    void N() { }
#endif
}
#endregion
"#;

/// The dump of `text` parsed by `parser` and by a fresh parser, which must be the same.
fn parse_as_fresh(parser: &mut Parser, text: &str) {
    let tree = parser.parse(text, FileKind::default());
    let fresh = Parser::new().parse(text, FileKind::default());
    assert_eq!(csharp_syntax::dump::tree_to_string(&tree), csharp_syntax::dump::tree_to_string(&fresh));
}

#[test]
fn a_parser_shrinks_its_buffers_past_the_retained_bytes_and_parses_as_before() {
    let big = format!("{}{}", MIXED.repeat(2000), source(10_000));
    let mut parser = Parser::new();
    parser.set_retained_bytes(8 << 20);
    parse_as_fresh(&mut parser, &big);
    assert!(parser.grown_bytes() > 8 << 20, "the file grows the buffers past the bound: {}", parser.grown_bytes());
    assert!(parser.heap_bytes() <= 4 << 20, "shrunk to half the bound: {}", parser.heap_bytes());
    // the next files reuse what is left (and may grow a buffer of other proportions), and parse as a fresh parser
    parse_as_fresh(&mut parser, MIXED);
    assert!(parser.heap_bytes() <= 8 << 20);
    parse_as_fresh(&mut parser, &big);
    // freed entirely, the buffers grow again
    parser.shrink_to(0);
    assert!(parser.heap_bytes() < 1024, "only the empty directive stack's cell: {}", parser.heap_bytes());
    parse_as_fresh(&mut parser, MIXED);
    parse_as_fresh(&mut parser, &big);
    // without a bound the buffers keep what they grew to
    parser.set_retained_bytes(usize::MAX);
    parse_as_fresh(&mut parser, &big);
    assert_eq!(parser.heap_bytes(), parser.grown_bytes());
    assert!(parser.heap_bytes() > 8 << 20);
    // a bound above what the buffers hold leaves them alone
    let held = parser.heap_bytes();
    parser.shrink_to(held);
    assert_eq!(parser.heap_bytes(), held);
}

#[test]
fn the_default_bound_keeps_an_ordinary_file_buffers() {
    let mut parser = Parser::new();
    parse_as_fresh(&mut parser, &source(1000));
    assert_eq!(parser.heap_bytes(), parser.grown_bytes());
    assert!(parser.heap_bytes() <= csharp_syntax::psi_builder::buffers::RETAINED_BYTES);
}
