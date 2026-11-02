//! Any bytes parse without a panic into a lossless tree: random bytes, random fragments of C#, deep nesting and long
//! chains, on a 1 MB stack; the leaves tile the text, ids are in preorder and the links agree, the lexer dump and the
//! tree dump run. The lexer is Roslyn's (its recursive scanners nest to `MAX_NESTED_SCAN_DEPTH`), the parser is still
//! stubs (phase 2): these hold for them, and must keep holding as the parser is ported (phase 4 adds the deep forms
//! against the oracle).

use csharp_syntax::dump::{lex_to_string, tree_to_string};
use csharp_syntax::{DocumentationMode, Entry, FileKind, NodeId, Parser, Tree};

/// A deterministic generator (xorshift64*), so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The tree is lossless and consistent: the token leaves, in order, are the text; every node's range covers its
/// children's; ids are preorder.
fn check(tree: &Tree, text: &[u8]) {
    let covered: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).to_vec()).collect();
    assert_eq!(covered, text, "the leaves are not the text");
    let mut previous_end = 0;
    for id in tree.tokens() {
        let range = tree.range(id);
        assert_eq!(range.start, previous_end, "leaves do not tile the text at {id:?}");
        previous_end = range.end;
    }
    for i in 1..tree.len() as u32 {
        let id = NodeId(i);
        let parent = tree.parent(id).expect("every node but the root has a parent");
        assert!(parent.0 < i && tree.subtree_end(parent).0 >= tree.subtree_end(id).0);
        let (p, r) = (tree.range(parent), tree.range(id));
        assert!(p.start <= r.start && r.end <= p.end, "{id:?} {r:?} is outside its parent's {p:?}");
    }
    let _ = tree_to_string(tree);
}

const FRAGMENTS: &[&str] = &[
    "class",
    " C",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "<",
    ">",
    ";",
    ",",
    ".",
    "=",
    "=>",
    "?",
    "??",
    ":",
    "::",
    "\"",
    "'",
    "@\"",
    "$\"",
    "\"\"\"",
    "{0}",
    "/*",
    "*/",
    "//",
    "///",
    "<summary>",
    "#if X",
    "#else",
    "#endif",
    "#region",
    "\n",
    "\r\n",
    "\r",
    " ",
    "\t",
    "int",
    "var",
    "async",
    "await",
    "x",
    "_",
    "1",
    "0x",
    "1.0e",
    "'\\u0041'",
    "\\u0061",
    "\u{FEFF}",
    "é",
    "😀",
    "\u{2028}",
    "\u{85}",
    "\0",
    "\u{FFFF}",
    "\\U0001F600",
    "\\u",
    "1e400",
    "1.5m",
    "0b_1",
    "#define A",
    "#elif",
    "#line (1, 2) - (3, 4) 5 \"f\"",
    "#pragma warning disable CS1",
    "#error version",
    "$$\"\"\"",
    "{{",
    "}}",
    "<<<<<<< ",
    "=======",
    ">>>>>>> ",
    "&lt;",
    "@@",
];

fn run_on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(1 << 20).spawn(f).unwrap().join().unwrap();
}

#[test]
fn random_bytes_and_fragments_parse_losslessly() {
    run_on_small_stack(|| {
        let mut rng = Rng(0x9E3779B97F4A7C15);
        let mut parser = Parser::new();
        for round in 0..2000 {
            let mut text = Vec::new();
            let len = rng.below(200);
            if round % 2 == 0 {
                text.extend((0..len).map(|_| rng.next() as u8));
            } else {
                for _ in 0..len / 4 {
                    text.extend_from_slice(FRAGMENTS[rng.below(FRAGMENTS.len())].as_bytes());
                }
            }
            let entry =
                [Entry::CompilationUnit, Entry::CompilationUnit, Entry::Expression, Entry::Statement][round % 4];
            let tree = parser.parse(&text, FileKind { entry, ..FileKind::default() });
            if entry == Entry::CompilationUnit {
                check(&tree, &text);
            }
            for doc_mode in [DocumentationMode::Parse, DocumentationMode::None, DocumentationMode::Diagnose] {
                let lexed = parser.lex(&text, FileKind { doc_mode, ..FileKind::default() });
                check(&lexed, &text);
                let _ = lex_to_string(&lexed);
            }
        }
    });
}

#[test]
fn deep_nesting_and_long_chains_parse_losslessly() {
    run_on_small_stack(|| {
        let mut parser = Parser::new();
        for (open, close) in [("(", ")"), ("{", "}"), ("[", "]"), ("<", ">"), ("/*", "*/"), ("#if A\n", "#endif\n")] {
            let text = format!("{}x{}", open.repeat(100_000), close.repeat(100_000));
            let tree = parser.parse(&text, FileKind::default());
            check(&tree, text.as_bytes());
        }
        let chain = format!("x = {};", vec!["a"; 100_000].join(" + "));
        let tree = parser.parse(&chain, FileKind::default());
        check(&tree, chain.as_bytes());
    });
}

/// The lexer's own recursion: holes of interpolated strings and brackets in them, directive expressions, nested
/// excluded `#if` sections, 100 000 deep.
#[test]
fn deep_lexer_forms_lex_losslessly() {
    run_on_small_stack(|| {
        let mut parser = Parser::new();
        let n = 100_000;
        let texts = [
            format!("{}x{}", "$\"{".repeat(n), "}\"".repeat(n)),
            format!("$\"{{{}x{}}}\"", "(".repeat(n), ")".repeat(n)),
            format!("$$\"\"\"{{{{{}x{}}}}}\"\"\"", "[".repeat(n), "]".repeat(n)),
            format!("#if {}A{}\n#endif\n", "(".repeat(n), ")".repeat(n)),
            format!("#if {}A\n#endif\n", "!".repeat(n)),
            format!("#if {}\n#endif\n", vec!["A"; n].join(" == ")),
            format!("#if B\n{}{}#endif\n", "#if A\n".repeat(n), "#endif\n".repeat(n)),
            format!("#region\n{}", "#region\n#endregion\n".repeat(n)),
            format!("{}\"", "@".repeat(n)),
            format!("$\"{{{}}}\"", "@".repeat(n)),
        ];
        for text in &texts {
            let lexed = parser.lex(text, FileKind::default());
            check(&lexed, text.as_bytes());
            let tree = parser.parse(text, FileKind::default());
            check(&tree, text.as_bytes());
        }
    });
}
