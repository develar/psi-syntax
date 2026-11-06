//! Any bytes parse without a panic and the tree covers them: random bytes, random mixes of Kotlin punctuation and
//! keywords, and every golden cut at many points.

mod common;

use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::{FileKind, NodeId, Tree};
use psi_check::{XorShift, check_leaves_are_text, limit, on_stack, too_deep_errors};

/// The leaves of the tree, in order, are the whole text, and node ids are in preorder.
fn check_lossless(text: &[u8], kind: FileKind) {
    let tree = Tree::parse(text, kind);
    assert!(tree.preorder(tree.root()).map(|(id, _)| id.0 as usize).eq(0..tree.len()));
    check_navigation(&tree);
    let mut covered = Vec::with_capacity(text.len());
    for id in tree.tokens() {
        covered.extend_from_slice(tree.text(id));
    }
    assert_eq!(covered, text, "{:?}", String::from_utf8_lossy(text));
    assert_eq!(tree.range(tree.root()), 0..text.len());
    let _ = psi_to_string(&tree, "a.kt");
}

/// The children, first/last child and next and previous sibling of each node agree with its parent links, and each
/// child's range is inside its parent's.
fn check_navigation(tree: &Tree) {
    assert_eq!(tree.previous_sibling(tree.root()), None);
    let mut children: Vec<Vec<NodeId>> = vec![Vec::new(); tree.len()];
    for i in 0..tree.len() as u32 {
        if let Some(parent) = tree.parent(NodeId(i)) {
            assert!(parent.0 < i);
            let (outer, inner) = (tree.range(parent), tree.range(NodeId(i)));
            assert!(outer.start <= inner.start && inner.end <= outer.end, "{outer:?} {inner:?}");
            children[parent.0 as usize].push(NodeId(i));
        }
    }
    for (i, expected) in children.iter().enumerate() {
        let id = NodeId(i as u32);
        assert!(tree.children(id).eq(expected.iter().copied()));
        assert_eq!(tree.first_child(id), expected.first().copied());
        assert_eq!(tree.last_child(id), expected.last().copied());
        for pair in expected.windows(2) {
            assert_eq!(tree.next_sibling(pair[0]), Some(pair[1]));
            assert_eq!(tree.previous_sibling(pair[1]), Some(pair[0]));
        }
        if let Some(&first) = expected.first() {
            assert_eq!(tree.previous_sibling(first), None);
        }
        if let Some(&last) = expected.last() {
            assert_eq!(tree.next_sibling(last), None);
        }
    }
}

#[test]
fn random_bytes_and_fragments_parse() {
    let mut rng = XorShift(0x9E3779B97F4A7C15);
    let pieces: &[&[u8]] = &[
        b"\"",
        b"\"\"\"",
        b"$",
        b"${",
        b"{",
        b"}",
        b"`",
        b"/*",
        b"*/",
        b"/**",
        b"'",
        b"\\",
        b"\n",
        b"(",
        b")",
        b"<",
        b">",
        b"fun",
        b"val",
        b"class",
        b"when",
        b"->",
        b"?.",
        b"?:",
        b"!!",
        b"@",
        b"[",
        b"]",
        b"#!",
        b" ",
        b"x",
        b"1.",
        b"0x",
        b"\xFF",
        b"\xC3",
        b"\xE2\x82\xAC",
        b"* @param ",
        b"```\n",
        b"[a.b]",
    ];
    for round in 0..3000 {
        let text = rng.bytes_or_pieces(round, pieces);
        check_lossless(&text, if round % 2 == 0 { FileKind::Kt } else { FileKind::Kts });
    }
    for golden in common::goldens() {
        let text = common::source_text(&golden).into_bytes();
        let step = (text.len() / 7).max(1);
        let mut cut = 0;
        while cut <= text.len() {
            check_lossless(&text[..cut], FileKind::Kt);
            cut += step;
        }
    }
}

/// The checks of `check_lossless` but the dump, whose indentation is quadratic in the depth: ids in preorder, the
/// navigation consistent with the parent links, the leaves the whole text.
fn check_tree(tree: &Tree, text: &[u8]) {
    assert!(tree.preorder(tree.root()).map(|(id, _)| id.0 as usize).eq(0..tree.len()));
    check_navigation(tree);
    check_leaves_are_text(tree, text);
}

/// Parses `text` on a `STACK` thread named `name`: the time of the parse, the tree well formed and lossless, and what
/// `measure` makes of it.
fn parse_on_small_stack<T: Send + 'static>(
    name: &str,
    text: String,
    measure: impl FnOnce(&Tree) -> T + Send + 'static,
) -> (std::time::Duration, T) {
    on_stack(name, move || {
        let start = std::time::Instant::now();
        let tree = Tree::parse(text.as_bytes(), FileKind::Kt);
        let elapsed = start.elapsed();
        check_tree(&tree, text.as_bytes());
        (elapsed, measure(&tree))
    })
}

/// Every recursive form, `n` levels deep.
fn nested_forms(n: usize) -> Vec<(&'static str, String)> {
    vec![
        ("parentheses", format!("val x = {}1{}\n", "(".repeat(n), ")".repeat(n))),
        ("unary minus", format!("val x = {}1\n", "- ".repeat(n))),
        ("calls", format!("val x = {}{}\n", "f(".repeat(n), ")".repeat(n))),
        ("collection literals", format!("val x = {}1{}\n", "[".repeat(n), "]".repeat(n))),
        ("lambdas", format!("val x = {}{}\n", "{ ".repeat(n), "}".repeat(n))),
        ("blocks", format!("fun f() {}{}\n", "{ run ".repeat(n), "}".repeat(n + 1))),
        ("local functions", format!("fun f() {}{}\n", "{ fun g() ".repeat(n), "}".repeat(n + 1))),
        ("generics", format!("val x: {}B{} = 1\n", "A<".repeat(n), ">".repeat(n))),
        ("function types", format!("val x: {}A{} = 1\n", "((".repeat(n), ") -> A)".repeat(n))),
        ("nullable types", format!("val x: {}A{} = 1\n", "(".repeat(n), ")?".repeat(n))),
        ("string templates", format!("val x = {}1{}\n", "\"${".repeat(n), "}\"".repeat(n))),
        ("classes", format!("{}{}\n", "class A { ".repeat(n), "}".repeat(n))),
        ("objects", format!("{}{}\n", "object A { ".repeat(n), "}".repeat(n))),
        ("object literals", format!("val x = {}1{}\n", "object { val y = ".repeat(n), " }".repeat(n))),
        ("if", format!("fun f() = {}1\n", "if (a) 1 else ".repeat(n))),
        ("when", format!("fun f() = {}1{}\n", "when { a -> ".repeat(n), " }".repeat(n))),
        ("try", format!("fun f() = {}1{}\n", "try { ".repeat(n), " } finally { }".repeat(n))),
        ("annotated expressions", format!("val x = {}1{}\n", "@A(".repeat(n), ")".repeat(n))),
        ("annotations", format!("{}{} fun f() {{}}\n", "@A(".repeat(n), ")".repeat(n))),
        ("labels", format!("val x = {}1\n", "a@ ".repeat(n))),
        ("destructuring", format!("val x = {{ {}a{} -> }}\n", "(".repeat(n), ")".repeat(n))),
        ("doc comments", format!("{}{}\nfun f() {{}}\n", "/** ".repeat(n), "*/".repeat(n))),
    ]
}

/// Every recursive form nested 100 000 deep parses on a 1 MB stack, in well under a second, into a well-formed,
/// lossless tree with the error "Code is too deeply nested" where the nesting passes the limit; the same text parses
/// to the same tree again (the limit is deterministic). PSI has no defined result here (kotlinc overflows its stack).
#[test]
fn deeply_nested_forms_parse_on_a_small_stack() {
    let forms = nested_forms(100_000);
    for (name, text) in forms {
        let again = text.clone();
        let (elapsed, errors) = parse_on_small_stack(name, text, move |tree| {
            let again = Tree::parse(again.as_bytes(), FileKind::Kt);
            assert!((0..tree.len() as u32).map(NodeId).all(|i| tree.kind(i) == again.kind(i)
                && tree.range(i) == again.range(i)
                && tree.parent(i) == again.parent(i)));
            too_deep_errors(tree)
        });
        // A doc comment's `/**` nests in the lexer, not the parser.
        assert!(errors > 0 || name == "doc comments", "{name}: no error at the limit");
        assert!(elapsed < limit(std::time::Duration::from_millis(2000)), "{name}: {elapsed:?}");
    }
}

/// With a tiny nesting limit, ordinary code takes the too-deep paths everywhere: every golden still parses without a
/// panic into a well-formed, lossless tree, and body-less parsing then `parse_body` gives the full parse's subtree.
#[test]
fn tiny_nesting_limits_keep_trees_lossless() {
    use kotlin_syntax::dump::subtree_to_string;
    use kotlin_syntax::{ParseOptions, Parser};
    for limit in [1, 2, 3, 5, 8, 13] {
        let (mut parser, mut body_parser): (Parser, Parser) =
            (psi_check::nesting_limited(limit), psi_check::nesting_limited(limit));
        let mut deep = 0;
        for golden in common::goldens() {
            let text = common::source_text(&golden).into_bytes();
            let kind = FileKind::from_file_name(&golden.name);
            let full = parser.parse(&text, kind);
            check_tree(&full, &text);
            deep += too_deep_errors(&full);
            let lazy = parser.parse_with_options(&text, kind, ParseOptions { expand_bodies: false });
            psi_check::each_body(&golden.name, &lazy, &full, |ln, fnode| {
                let body = body_parser.parse_body(&lazy, ln, ParseOptions::default()).unwrap();
                assert_eq!(subtree_to_string(&body, body.root()), subtree_to_string(&full, fnode), "{}", golden.name);
            });
        }
        assert!(deep > 0, "limit {limit}");
    }
}

/// Every left-associative chain of 100 000 operators parses in linear time on a 1 MB stack: each operator wraps the
/// expression so far (`precede`), which the production list does in constant time, and the tree nests 100 000 deep
/// without recursion. Before the list, `a+a+…` took 4.67 s for 100 000 operators.
#[test]
fn long_left_associative_chains_parse_in_linear_time() {
    let n = 100_000;
    let chains: Vec<(&str, String)> = vec![
        ("+", format!("val x = a{}\n", "+a".repeat(n))),
        ("*", format!("val x = a{}\n", "*a".repeat(n))),
        ("&&", format!("val x = a{}\n", "&&a".repeat(n))),
        ("||", format!("val x = a{}\n", "||a".repeat(n))),
        ("?:", format!("val x = a{}\n", "?:a".repeat(n))),
        ("==", format!("val x = a{}\n", "==a".repeat(n))),
        ("<", format!("val x = a{}\n", " < a".repeat(n))),
        ("<, no spaces", format!("val x = a{}\n", "<a".repeat(n))),
        ("..", format!("val x = a{}\n", "..a".repeat(n))),
        ("infix calls", format!("val x = a{}\n", " to a".repeat(n))),
        ("as", format!("val x = a{}\n", " as A".repeat(n))),
        ("is", format!("val x = a{}\n", " is A".repeat(n))),
        ("a.b", format!("val x = a{}\n", ".b".repeat(n))),
        ("a?.b", format!("val x = a{}\n", "?.b".repeat(n))),
        ("a[0]", format!("val x = a{}\n", "[0]".repeat(n))),
        ("f()", format!("val x = f{}\n", "()".repeat(n))),
        ("!!", format!("val x = a{}\n", "!!".repeat(n))),
        ("assignments", format!("fun f() {{ a{} }}\n", " = a".repeat(n))),
    ];
    for (name, text) in chains {
        let (elapsed, depth) =
            parse_on_small_stack(name, text, |tree| tree.preorder(tree.root()).map(|(_, d)| d).max().unwrap_or(0));
        println!("{name}: {} ms, depth {depth}", elapsed.as_millis());
        assert!(depth > n, "{name}: depth {depth}");
        assert!(elapsed < limit(std::time::Duration::from_millis(1000)), "{name}: {elapsed:?}");
    }
}

/// A parser keeps at most its retained bytes from one file to the next (`Parser::set_retained_bytes`, the same
/// mechanism as C#'s), and a parser whose buffers were shrunk parses as a fresh one does.
#[test]
fn a_parser_shrinks_its_buffers_past_the_retained_bytes_and_parses_as_before() {
    let file = "/** doc [link] */\nclass C<T> { fun f(x: Int) = listOf(1, 2).map { it * x }.filter { y -> y > 0 }?.size ?: 0\n  val s = \"a${b}c\"\n}\n";
    let big = file.repeat(20_000);
    psi_check::check_retained_bytes(file, &big, |parser: &mut kotlin_syntax::Parser, text: &str| {
        let tree = parser.parse(text, FileKind::Kt);
        assert_eq!(psi_to_string(&tree, "a.kt"), psi_to_string(&Tree::parse(text, FileKind::Kt), "a.kt"));
    });
}

/// The levels every recursive form reaches before the stack stops it (`psi_builder::stack`): the depth limit, a count,
/// is the same bound on every target, while the stack a level takes is the compiler's (506 KB at the limit on macOS
/// arm64, 610 KB on Windows x64, within the budget). So every form reaches the limit on every target this runs on, and
/// its tree is the same. Prints each form's levels and stack, on a 1 MB stack.
#[test]
fn the_depth_limit_holds_before_the_stack_runs_out() {
    let failures = psi_check::depth_limit_failures(nested_forms(100_000), u32::MAX, |text| {
        let mut parser = kotlin_syntax::Parser::new();
        let _ = parser.parse(text, FileKind::Kt);
        parser.stack_use()
    });
    assert!(failures.is_empty(), "{failures:#?}");
}
