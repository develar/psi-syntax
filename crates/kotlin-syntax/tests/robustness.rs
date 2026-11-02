//! Any bytes parse without a panic and the tree covers them: random bytes, random mixes of Kotlin punctuation and
//! keywords, and every golden cut at many points.

mod common;

use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::{FileKind, NodeId, Tree};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

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
        let len = (rng.next() % 200) as usize;
        let mut text = Vec::with_capacity(len * 3);
        for _ in 0..len {
            if round % 3 == 0 {
                text.push(rng.next() as u8);
            } else {
                text.extend_from_slice(pieces[(rng.next() % pieces.len() as u64) as usize]);
            }
        }
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

/// The stack a parse is promised to fit (AGENTS.md, "The consumer contract"): 1 MB in a release build; a debug build's
/// frames are several times larger.
const STACK: usize = if cfg!(debug_assertions) { 16 << 20 } else { 1 << 20 };

/// Runs `f` on a thread with `STACK` bytes of stack.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().stack_size(STACK).spawn(f).unwrap().join().unwrap()
}

/// The checks of `check_lossless` but the dump, whose indentation is quadratic in the depth: ids in preorder, the
/// navigation consistent with the parent links, the leaves the whole text.
fn check_tree(tree: &Tree, text: &[u8]) {
    assert!(tree.preorder(tree.root()).map(|(id, _)| id.0 as usize).eq(0..tree.len()));
    check_navigation(tree);
    let covered: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).iter().copied()).collect();
    assert!(covered == text, "the leaves are not the text");
    assert_eq!(tree.range(tree.root()), 0..text.len());
}

fn too_deep_errors(tree: &Tree) -> usize {
    (0..tree.len() as u32).filter(|&i| tree.error_message(NodeId(i)) == Some("Code is too deeply nested")).count()
}

/// Every recursive form nested 100 000 deep parses on a 1 MB stack, in well under a second, into a well-formed,
/// lossless tree with the error "Code is too deeply nested" where the nesting passes the limit; the same text parses
/// to the same tree again (the limit is deterministic). PSI has no defined result here (kotlinc overflows its stack).
#[test]
fn deeply_nested_forms_parse_on_a_small_stack() {
    let n = 100_000;
    let forms: Vec<(&str, String)> = vec![
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
    ];
    for (name, text) in forms {
        let (elapsed, errors) = on_small_stack(move || {
            let start = std::time::Instant::now();
            let tree = Tree::parse(text.as_bytes(), FileKind::Kt);
            let elapsed = start.elapsed();
            check_tree(&tree, text.as_bytes());
            let again = Tree::parse(text.as_bytes(), FileKind::Kt);
            assert!((0..tree.len() as u32).map(NodeId).all(|i| tree.kind(i) == again.kind(i)
                && tree.range(i) == again.range(i)
                && tree.parent(i) == again.parent(i)));
            (elapsed, too_deep_errors(&tree))
        });
        // A doc comment's `/**` nests in the lexer, not the parser.
        assert!(errors > 0 || name == "doc comments", "{name}: no error at the limit");
        assert!(elapsed.as_millis() < 2000, "{name}: {elapsed:?}");
    }
}

/// With a tiny nesting limit, ordinary code takes the too-deep paths everywhere: every golden still parses without a
/// panic into a well-formed, lossless tree, and body-less parsing then `parse_body` gives the full parse's subtree.
#[test]
fn tiny_nesting_limits_keep_trees_lossless() {
    use kotlin_syntax::dump::subtree_to_string;
    use kotlin_syntax::{ParseOptions, Parser};
    for limit in [1, 2, 3, 5, 8, 13] {
        let mut parser = Parser::new();
        parser.set_nesting_limit(limit);
        let mut body_parser = Parser::new();
        body_parser.set_nesting_limit(limit);
        let mut deep = 0;
        for golden in common::goldens() {
            let text = common::source_text(&golden).into_bytes();
            let kind = FileKind::from_file_name(&golden.name);
            let full = parser.parse(&text, kind);
            check_tree(&full, &text);
            deep += too_deep_errors(&full);
            let lazy = parser.parse_with_options(&text, kind, ParseOptions { expand_bodies: false });
            let mut f = 0u32;
            for l in 0..lazy.len() as u32 {
                let (ln, fnode) = (NodeId(l), NodeId(f));
                assert_eq!(lazy.range(ln), full.range(fnode), "{}", golden.name);
                if lazy.is_unexpanded(ln) {
                    let body = body_parser.parse_body(&lazy, ln, ParseOptions::default()).unwrap();
                    assert_eq!(
                        subtree_to_string(&body, body.root()),
                        subtree_to_string(&full, fnode),
                        "{}",
                        golden.name
                    );
                    f = full.subtree_end(fnode).0;
                } else {
                    f += 1;
                }
            }
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
        let (elapsed, depth) = on_small_stack(move || {
            let start = std::time::Instant::now();
            let tree = Tree::parse(text.as_bytes(), FileKind::Kt);
            let elapsed = start.elapsed();
            check_tree(&tree, text.as_bytes());
            (elapsed, tree.preorder(tree.root()).map(|(_, d)| d).max().unwrap_or(0))
        });
        println!("{name}: {} ms, depth {depth}", elapsed.as_millis());
        assert!(depth > n, "{name}: depth {depth}");
        assert!(elapsed.as_millis() < 1000, "{name}: {elapsed:?}");
    }
}

/// A parser keeps at most its retained bytes from one file to the next (`Parser::set_retained_bytes`, the same
/// mechanism as C#'s), and a parser whose buffers were shrunk parses as a fresh one does.
#[test]
fn a_parser_shrinks_its_buffers_past_the_retained_bytes_and_parses_as_before() {
    let file = "/** doc [link] */\nclass C<T> { fun f(x: Int) = listOf(1, 2).map { it * x }.filter { y -> y > 0 }?.size ?: 0\n  val s = \"a${b}c\"\n}\n";
    let big = file.repeat(20_000);
    let parse = |parser: &mut kotlin_syntax::Parser, text: &str| {
        let tree = parser.parse(text, FileKind::Kt);
        assert_eq!(psi_to_string(&tree, "a.kt"), psi_to_string(&Tree::parse(text, FileKind::Kt), "a.kt"));
    };
    let mut parser = kotlin_syntax::Parser::new();
    parser.set_retained_bytes(8 << 20);
    parse(&mut parser, &big);
    assert!(parser.grown_bytes() > 8 << 20, "{}", parser.grown_bytes());
    assert!(parser.heap_bytes() <= 4 << 20, "{}", parser.heap_bytes());
    // a share under 1 / the frames' capacity still keeps the file's builder (`Shrink::nested`), so the buffers keep
    // nearly all of what they are shrunk to (0.58 of it while the share dropped every frame)
    assert!(parser.heap_bytes() > 3 << 20, "{}", parser.heap_bytes());
    parse(&mut parser, file);
    parse(&mut parser, &big);
    parser.shrink_to(0);
    assert_eq!(parser.heap_bytes(), 0);
    parse(&mut parser, file);
    parser.set_retained_bytes(usize::MAX);
    parse(&mut parser, &big);
    assert_eq!(parser.heap_bytes(), parser.grown_bytes());
}
