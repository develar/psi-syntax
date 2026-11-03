//! Deeply nested and long inputs parse on a small stack in linear time: every recursive form of the Java parsers
//! nested 100 000 deep, every left-associative chain of 100 000 operators, and the goldens under tiny nesting limits.

use java_syntax::{FileKind, LanguageLevel, NodeId, Tree};

/// A wall-clock limit of these tests, multiplied by `PSI_SYNTAX_TIME_SCALE` (accio's CI sets it for its shared runners);
/// unset, the limit as measured on a developer's machine, which `scripts/gate.sh` holds.
fn limit(limit: std::time::Duration) -> std::time::Duration {
    limit * std::env::var("PSI_SYNTAX_TIME_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1)
}

/// The stack a parse is promised to fit (AGENTS.md, "The consumer contract"): 1 MB in a release build; a debug build's
/// frames are several times larger.
const STACK: usize = if cfg!(debug_assertions) { 16 << 20 } else { 1 << 20 };

fn on_stack<T: Send + 'static>(stack: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().stack_size(stack).spawn(f).unwrap().join().unwrap()
}

/// A field initializer.
fn field(expr: &str) -> String {
    format!("class C {{ Object x = {expr}; }}\n")
}

/// Statements in a method body.
fn method(statements: &str) -> String {
    format!("class C {{ void f() {{ {statements} }} }}\n")
}

/// Every recursive form, `n` levels deep.
fn nested_forms(n: usize) -> Vec<(&'static str, String)> {
    let r = |s: &str, k: usize| s.repeat(k);
    vec![
        ("parentheses", field(&format!("{}1{}", r("(", n), r(")", n)))),
        ("unary minus", field(&format!("{}1", r("- ", n)))),
        ("unary not", field(&format!("{}a", r("!", n)))),
        ("casts", field(&format!("{}a", r("(A) ", n)))),
        ("primitive casts", field(&format!("{}1", r("(int) ", n)))),
        ("generic casts", field(&format!("{}a", r("(A<B>) ", n)))),
        ("calls", field(&format!("{}{}", r("f(", n), r(")", n)))),
        ("qualified calls", field(&format!("{}{}", r("a.f(", n), r(")", n)))),
        ("array initializers", field(&format!("{}1{}", r("{", n), r("}", n)))),
        ("new arrays", field(&format!("{}1{}", r("new int[", n), r("]", n)))),
        ("array initializers in new", field(&format!("{}1{}", r("new int[] {", n), r("}", n)))),
        ("array accesses", field(&format!("{}0{}", r("a[", n), r("]", n)))),
        ("lambdas", field(&r("x -> ", n).to_string())),
        ("parenthesized lambdas", field(&format!("{}1", r("(a) -> ", n)))),
        ("typed lambdas", field(&format!("{}1", r("(A a) -> ", n)))),
        ("lambda blocks", field(&format!("{}1{}", r("() -> { return ", n), r("; }", n)))),
        ("anonymous classes", field(&format!("{}1{}", r("new A() { Object x = ", n), r("; }", n)))),
        ("anonymous class methods", field(&format!("{}1{}", r("new A() { void f() { g(", n), r("); } }", n)))),
        ("conditionals", field(&format!("{}c", r("a ? b : ", n)))),
        ("conditionals in the middle", field(&format!("{}b{}", r("a ? ", n), r(" : c", n)))),
        ("switch expressions", field(&format!("{}1{}", r("switch (a) { default -> ", n), r("; }", n)))),
        ("switch expression blocks", field(&format!("{}1{}", r("switch (a) { default -> { yield ", n), r("; } }", n)))),
        ("string templates", field(&format!("{}1{}", r("\"\\{", n), r("}\"", n)))),
        ("text block templates", field(&format!("{}1{}", r("\"\"\"\n\\{", n), r("}\"\"\"", n)))),
        ("method references", field(&format!("{}{}::f", r("A<", n), r(">", n)))),
        ("generic method calls", field(&format!("this.<{}B{}>f()", r("A<", n), r(">", n)))),
        ("class literals", field(&format!("{}{}.class", r("A<", n), r(">", n)))),
        ("annotated expressions", field(&format!("{}1{}", r("@A(", n), r(")", n)))),
        ("assignments", method(&format!("{}a;", r("a = ", n)))),
        ("compound assignments", method(&format!("{}a;", r("a += ", n)))),
        ("blocks", method(&format!("{}{}", r("{ ", n), r("}", n)))),
        ("if", method(&format!("{};", r("if (a) ", n)))),
        ("if else", method(&format!("{};", r("if (a) ; else ", n)))),
        ("while", method(&format!("{};", r("while (a) ", n)))),
        ("for", method(&format!("{};", r("for (;;) ", n)))),
        ("for each", method(&format!("{};", r("for (A a : b) ", n)))),
        ("do", method(&format!("{};{}", r("do ", n), r(" while (a);", n)))),
        ("labels", method(&format!("{};", r("a: ", n)))),
        ("synchronized", method(&format!("{}{}", r("synchronized (a) { ", n), r("}", n)))),
        ("try", method(&format!("{}{}", r("try { ", n), r("} finally { }", n)))),
        ("try with resources", method(&format!("{}{}", r("try (A a = b) { ", n), r("}", n)))),
        ("catch", method(&format!("{}{}", r("try { } catch (E e) { ", n), r("}", n)))),
        ("switch statements", method(&format!("{}{}", r("switch (a) { case 1: ", n), r("}", n)))),
        ("switch rules", method(&format!("{};{}", r("switch (a) { case 1 -> { ", n), r("} }", n)))),
        ("local classes", method(&format!("{}{}", r("class A { void f() { ", n), r("} }", n)))),
        ("record patterns", method(&format!("if (o instanceof {}a{}) ;", r("A(", n), r(")", n)))),
        ("case patterns", method(&format!("switch (o) {{ case {}a{} -> {{}} }}", r("A(", n), r(")", n)))),
        ("classes", format!("{}{}\n", r("class A { ", n), r("}", n))),
        ("interfaces", format!("{}{}\n", r("interface A { ", n), r("}", n))),
        ("enums", format!("{}{}\n", r("enum A { B; ", n), r("}", n))),
        ("enum constant bodies", format!("{}{}\n", r("enum A { B { ", n), r("} }", n))),
        ("records", format!("{}{}\n", r("record A() { ", n), r("}", n))),
        ("annotation types", format!("{}{}\n", r("@interface A { ", n), r("}", n))),
        ("initializers", format!("class C {}{}\n", r("{ class A ", n), r("}", n + 1))),
        ("generics", format!("class C {{ {}B{} x; }}\n", r("A<", n), r(">", n))),
        ("wildcards", format!("class C {{ {}B{} x; }}\n", r("A<? extends ", n), r(">", n))),
        ("type parameters", format!("class C<T extends {}B{}> {{}}\n", r("A<", n), r(">", n))),
        ("annotations", format!("{}{} class C {{}}\n", r("@A(", n), r(")", n))),
        ("annotation arrays", format!("@A({}{}) class C {{}}\n", r("{", n), r("}", n))),
        ("annotation values", format!("{}{} class C {{}}\n", r("@A(b = ", n), r(")", n))),
        ("type annotations", format!("class C {{ List<{}{} B> x; }}\n", r("@A(", n), r(")", n))),
        ("annotated statements", method(&format!("{}{} int x;", r("@A(", n), r(")", n)))),
        ("module annotations", format!("{}{} module m {{}}\n", r("@A(", n), r(")", n))),
        ("doc comments", format!("{}{}\nclass C {{}}\n", r("/** ", n), r("*/", n))),
        ("doc inline tags", format!("/** {}{} */\nclass C {{}}\n", r("{@code ", n), r("}", n))),
        ("doc link tags", format!("/** {}{} */\nclass C {{}}\n", r("{@link A ", n), r("}", n))),
        ("doc braces", format!("/** {}{} */\nclass C {{}}\n", r("{", n), r("}", n))),
        ("doc references", format!("/** @see A#f({}B{}) */\nclass C {{}}\n", r("A<", n), r(">", n))),
        ("markdown doc", format!("/// {}a{}\nclass C {{}}\n", r("[", n), r("]", n))),
        ("annotations in calls", field(&format!("{}1{}", r("@A(f(", n), r("))", n)))),
        ("annotated parenthesized", field(&format!("{}1{}", r("(@A(", n), r(") a)", n)))),
        ("annotated casts", field(&format!("{}1{}", r("(@A(", n), r(") B) + ", n)))),
        ("annotated type arguments", field(&format!("{}1{}", r("a < @A(", n), r(") B > c", n)))),
        ("class literal reparse", field(&format!("{}1{}", r("f(", n), r(").a.class", n)))),
        ("this reparse", field(&format!("{}1{}", r("f(", n), r(").a.this", n)))),
        ("array class reparse", field(&format!("{}1{}", r("f(", n), r(").a[]", n)))),
        ("generic calls reparse", field(&format!("{}1{}", r("a<b>f(", n), r(")", n)))),
        ("method refs reparse", field(&format!("{}1{}", r("f(", n), r(")::g", n)))),
        ("typed lambda reparse", field(&format!("{}1{}", r("(a<b>c(", n), r(")) + 1", n)))),
        ("cast reparse", field(&format!("{}1{}", r("(a<b>c(", n), r(")) - 1", n)))),
        ("instanceof patterns", field(&format!("{}1{}", r("a instanceof A(B b) && f(", n), r(")", n)))),
        ("javadoc in nested classes", format!("{}{}\n", r("/** {@code a} */ class A { ", n), r("}", n))),
    ]
}

/// Every left-associative chain (and loop) of `n` operators.
fn chains(n: usize) -> Vec<(&'static str, String)> {
    let r = |s: &str| s.repeat(n);
    let mut chains: Vec<(&'static str, String)> =
        ["+", "-", "*", "/", "%", "<<", ">>", ">>>", "<", ">", "<=", ">=", "==", "!=", "&", "^", "|", "&&", "||"]
            .iter()
            .map(|op| (*op, field(&format!("a{}", format!(" {op} a").repeat(n)))))
            .collect();
    chains.extend([
        ("<, no spaces", field(&format!("a{}", r("<a")))),
        (">, no spaces", field(&format!("a{}", r(">a")))),
        ("generic-looking", field(&format!("a{}", r("<b>c")))),
        ("instanceof", field(&format!("a{}", r(" instanceof A")))),
        ("postfix ++", field(&format!("a{}", r("++")))),
        ("postfix --", field(&format!("a{}", r("--")))),
        ("a.b", field(&format!("a{}", r(".b")))),
        ("a.f()", field(&format!("a{}", r(".f()")))),
        ("a[0]", field(&format!("a{}", r("[0]")))),
        ("a::f", field(&format!("a{}", r("::f")))),
        ("a.new B()", field(&format!("a{}", r(".new B()")))),
        ("arguments", field(&format!("f(a{})", r(", a")))),
        ("array elements", field(&format!("{{a{}}}", r(", a")))),
        ("expression lists", method(&format!("a{};", r(", a")))),
        ("statements", method(&r("a(); "))),
        ("declarations", method(&r("int a; "))),
        ("members", format!("class C {{ {} }}\n", r("int a; "))),
        ("qualified names", field(&format!("a{}", r(".a")))),
        ("array dimensions", format!("class C {{ int{} x; }}\n", r("[]"))),
        ("type arguments", format!("class C {{ A<B{}> x; }}\n", r(", B"))),
        ("modifiers", format!("{} class C {{}}\n", r("public "))),
        ("annotation list", format!("{} class C {{}}\n", r("@A "))),
        ("imports", r("import a.b;\n")),
        ("doc tags", format!("/**{}*/\nclass C {{}}\n", r("\n * @param a b"))),
        ("string concatenation", field(&format!("\"a\"{}", r(" + \"a\"")))),
    ]);
    chains
}

/// Ids in preorder, the navigation consistent with the parent links, the leaves the whole text.
fn check_tree(tree: &Tree, text: &[u8]) {
    assert!(tree.preorder(tree.root()).map(|(id, _)| id.0 as usize).eq(0..tree.len()));
    let mut last_child: Vec<Option<NodeId>> = vec![None; tree.len()];
    for i in 0..tree.len() as u32 {
        let id = NodeId(i);
        if let Some(parent) = tree.parent(id) {
            assert!(parent.0 < i);
            let (outer, inner) = (tree.range(parent), tree.range(id));
            assert!(outer.start <= inner.start && inner.end <= outer.end, "{outer:?} {inner:?}");
            match last_child[parent.0 as usize] {
                None => assert_eq!(tree.first_child(parent), Some(id)),
                Some(previous) => assert_eq!(tree.next_sibling(previous), Some(id)),
            }
            assert_eq!(tree.previous_sibling(id), last_child[parent.0 as usize]);
            last_child[parent.0 as usize] = Some(id);
        }
    }
    for (i, last) in last_child.iter().enumerate() {
        assert_eq!(tree.last_child(NodeId(i as u32)), *last);
    }
    let covered: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).iter().copied()).collect();
    assert!(covered == text, "the leaves are not the text");
    assert_eq!(tree.range(tree.root()), 0..text.len());
}

/// The subtree of `a` at `a_root` and that of `b` at `b_root` have the same nodes: kinds, ranges, error messages and
/// shape.
fn same_subtree(a: &Tree, a_root: NodeId, b: &Tree, b_root: NodeId) -> bool {
    let size = a.subtree_end(a_root).0 - a_root.0;
    if b.subtree_end(b_root).0 - b_root.0 != size {
        return false;
    }
    (0..size).all(|k| {
        let (x, y) = (NodeId(a_root.0 + k), NodeId(b_root.0 + k));
        a.kind(x) == b.kind(y)
            && a.range(x) == b.range(y)
            && a.error_message(x) == b.error_message(y)
            && a.subtree_end(x).0 - x.0 == b.subtree_end(y).0 - y.0
            && (k == 0 || a.parent(x).map(|p| x.0 - p.0) == b.parent(y).map(|p| y.0 - p.0))
    })
}

fn too_deep_errors(tree: &Tree) -> usize {
    (0..tree.len() as u32).filter(|&i| tree.error_message(NodeId(i)) == Some("Code is too deeply nested")).count()
}

const KIND: FileKind = FileKind { level: LanguageLevel::JDK_21_PREVIEW, entry: java_syntax::Entry::File };

/// The forms whose rolled-back attempt scans ahead at every level (see `lambda_parameter_recovery_is_bounded`).
const SCANNING_FORMS: [&str; 2] = ["typed lambda reparse", "cast reparse"];

/// Parses `text` twice on a `STACK` thread: the time of the first parse, the too-deep errors, the tree's depth; the
/// tree is well formed and lossless, and the second parse gives the same tree (the limits are deterministic).
fn parse_on_small_stack(text: String) -> (std::time::Duration, usize, usize) {
    on_stack(STACK, move || {
        let start = std::time::Instant::now();
        let tree = Tree::parse(text.as_bytes(), KIND);
        let elapsed = start.elapsed();
        check_tree(&tree, text.as_bytes());
        let again = Tree::parse(text.as_bytes(), KIND);
        assert!(same_subtree(&tree, tree.root(), &again, again.root()), "the second parse differs");
        let depth = tree.preorder(tree.root()).map(|(_, d)| d).max().unwrap_or(0);
        (elapsed, too_deep_errors(&tree), depth)
    })
}

/// Every recursive form nested 100 000 deep parses on a 1 MB stack, in well under a second, into a well-formed,
/// lossless tree with the error "Code is too deeply nested" where the nesting passes a limit; the same text parses to
/// the same tree again. PSI has no defined result here (javac and IntelliJ overflow their stacks).
#[test]
fn deeply_nested_forms_parse_on_a_small_stack() {
    let n = 100_000;
    // No limit is reached: `else if` is a loop of the parser; `/**` nests in no lexer, `{@code {…}` is the tag's text,
    // `[[…]]` a loop; `A<A<…>>::f` and `A<A<…>>.class` are a type tried and rolled back at the type argument limit,
    // then a flat comparison chain.
    let flat = [
        "if else",
        "doc comments",
        "doc inline tags",
        "doc braces",
        "markdown doc",
        "method references",
        "class literals",
    ];
    for (name, text) in nested_forms(n) {
        if SCANNING_FORMS.contains(&name) {
            continue;
        }
        let (elapsed, errors, _) = parse_on_small_stack(text);
        println!("{name}: {:.1} ms, {errors} too-deep errors", elapsed.as_secs_f64() * 1000.0);
        assert!(errors > 0 || flat.contains(&name), "{name}: no error at the limit");
        assert!(elapsed < limit(std::time::Duration::from_millis(2000)), "{name}: {elapsed:?}");
    }
}

/// Every left-associative chain of 100 000 operators, and every long list, parses in linear time on a 1 MB stack. A
/// binary operator's chain is one polyadic expression; a postfix, qualified, call, index, `instanceof` or qualified
/// `new` chain wraps the expression so far at each operator (`precede`, constant time in the production list) and
/// nests 100 000 deep in the tree without recursion. `a < a < …` tries a type with type arguments at every `a`,
/// bounded by `TYPE_ARGUMENTS_LIMIT` with the lists before the deepest too deep (15 s for 16 000 operators without
/// it).
#[test]
fn long_chains_parse_in_linear_time() {
    let n = 100_000;
    let nested = ["instanceof", "postfix ++", "postfix --", "a.b", "a.f()", "a[0]", "a.new B()", "qualified names"];
    for (name, text) in chains(n) {
        let (elapsed, _, depth) = parse_on_small_stack(text);
        println!("{name}: {:.1} ms, depth {depth}", elapsed.as_secs_f64() * 1000.0);
        assert!(depth > n || !nested.contains(&name), "{name}: depth {depth}");
        assert!(elapsed < limit(std::time::Duration::from_millis(1000)), "{name}: {elapsed:?}");
    }
}

/// A typed lambda's parameter list that is not one (`(a<b>c(…`) is parsed with its error recovery to the `)` before
/// it is rolled back, as in PSI: each level scans the rest of the text, so the work is the levels inside the nesting
/// limit times the text, linear but not small (DESIGN, "Open items").
#[test]
fn lambda_parameter_recovery_is_bounded() {
    for name in SCANNING_FORMS {
        let text = nested_forms(10_000).into_iter().find(|(n, _)| *n == name).unwrap().1;
        let (elapsed, errors, _) = parse_on_small_stack(text);
        println!("{name}: {:.1} ms at 10 000 levels, {errors} too-deep errors", elapsed.as_secs_f64() * 1000.0);
        assert!(errors > 0, "{name}: no error at the limit");
        assert!(elapsed < limit(std::time::Duration::from_millis(10_000)), "{name}: {elapsed:?}");
    }
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Random mixes of the recursive expression forms nested past the limit parse on a 1 MB stack: the stack a level
/// takes depends on the path, and a mix takes paths no single form does.
#[test]
fn random_mixes_parse_on_a_small_stack() {
    let forms: &[(&str, &str)] = &[
        ("(", ")"),
        ("- ", ""),
        ("(A) ", ""),
        ("(A<B>) ", ""),
        ("f(", ")"),
        ("a.f(", ")"),
        ("{", "}"),
        ("new int[", "]"),
        ("new int[] {", "}"),
        ("a[", "]"),
        ("x -> ", ""),
        ("(a) -> ", ""),
        ("(A a) -> ", ""),
        ("a ? b : ", ""),
        ("a ? ", " : c"),
        ("a = ", ""),
        ("new A() { Object x = ", "; }"),
        ("new A() { void f() { g(", "); } }"),
        ("switch (a) { default -> ", "; }"),
        ("() -> { return ", "; }"),
        ("\"\\{", "}\""),
        ("@A(", ")"),
        ("@A(f(", "))"),
        ("a instanceof A(B b) && f(", ")"),
        ("A.<B>f(", ")"),
        ("new A<B>(", ")"),
    ];
    let mut rng = XorShift(0x2545F4914F6CDD1D);
    for _ in 0..40 {
        let (mut open, mut close) = (String::new(), Vec::new());
        for _ in 0..3000 {
            let (o, c) = forms[(rng.next() % forms.len() as u64) as usize];
            open.push_str(o);
            close.push(c);
        }
        close.reverse();
        let text = field(&format!("{open}1{}", close.concat()));
        let (elapsed, errors, _) = parse_on_small_stack(text);
        assert!(errors > 0, "no error at the limit");
        assert!(elapsed < limit(std::time::Duration::from_millis(2000)), "{elapsed:?}");
    }
}

/// Any bytes and random mixes of Java punctuation, keywords and doc comment pieces parse without a panic into a
/// well-formed, lossless tree.
#[test]
fn random_bytes_and_fragments_parse() {
    let pieces: &[&[u8]] = &[
        b"\"",
        b"\"\"\"\n",
        b"\\{",
        b"{",
        b"}",
        b"(",
        b")",
        b"[",
        b"]",
        b"<",
        b">",
        b"/*",
        b"*/",
        b"/**",
        b"///",
        b"'",
        b"\\",
        b"\n",
        b"@",
        b"->",
        b"::",
        b"?",
        b":",
        b";",
        b",",
        b".",
        b" ",
        b"x",
        b"1",
        b"0x",
        b"class",
        b"record",
        b"enum",
        b"interface",
        b"new",
        b"switch",
        b"case",
        b"default",
        b"yield",
        b"if",
        b"else",
        b"try",
        b"catch",
        b"for",
        b"var",
        b"instanceof",
        b"when",
        b"module",
        b"requires",
        b"\xFF",
        b"\xC3",
        b"\xE2\x82\xAC",
        b"{@code ",
        b"{@link ",
        b"@param ",
        b"```\n",
        b"[a.b]",
    ];
    let mut rng = XorShift(0x9E3779B97F4A7C15);
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
        let tree = Tree::parse(&text, FileKind::default());
        check_tree(&tree, &text);
    }
}

/// The goldens of `testdata/java`: each line of the manifest is `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM`.
fn goldens() -> Vec<(String, Vec<u8>, FileKind)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java");
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).expect("read manifest.tsv");
    let mut goldens = Vec::new();
    for line in manifest.lines().filter(|l| !l.is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let level = LanguageLevel::from_name(columns[1]).unwrap_or(LanguageLevel::HIGHEST);
        let entry = java_syntax::Entry::from_name(columns[2]).unwrap_or(java_syntax::Entry::File);
        let mut text: Vec<u8> = std::fs::read(root.join(columns[0])).expect("read golden");
        text.retain(|&b| b != b'\r');
        if columns[3] == "1" {
            let start = text.iter().position(|&b| b > b' ').unwrap_or(text.len());
            let end = text.iter().rposition(|&b| b > b' ').map_or(start, |e| e + 1);
            text = text[start..end].to_vec();
        }
        goldens.push((columns[0].to_string(), text, FileKind { level, entry }));
    }
    goldens
}

/// With a tiny nesting limit, ordinary code takes the too-deep paths everywhere: every golden still parses without a
/// panic into a well-formed, lossless tree, and body-less parsing then `parse_body` gives the full parse's subtree.
#[test]
fn tiny_nesting_limits_keep_trees_lossless() {
    use java_syntax::{ParseOptions, Parser};
    let goldens = goldens();
    for limit in [1, 2, 3, 5, 8, 13] {
        let mut parser = Parser::new();
        parser.set_nesting_limit(limit);
        let mut body_parser = Parser::new();
        body_parser.set_nesting_limit(limit);
        let (mut deep, mut bodies) = (0, 0);
        for (name, text, kind) in &goldens {
            let full = parser.parse(text, *kind);
            check_tree(&full, text);
            deep += too_deep_errors(&full);
            let lazy = parser.parse_with_options(text, *kind, ParseOptions { expand_bodies: false });
            let mut f = 0u32;
            for l in 0..lazy.len() as u32 {
                let (ln, fnode) = (NodeId(l), NodeId(f));
                assert_eq!(lazy.range(ln), full.range(fnode), "{name}");
                if lazy.is_unexpanded(ln) {
                    bodies += 1;
                    let body = body_parser.parse_body(&lazy, ln, ParseOptions::default()).unwrap();
                    assert!(same_subtree(&body, body.root(), &full, fnode), "{name}: body {:?}", lazy.range(ln));
                    f = full.subtree_end(fnode).0;
                } else {
                    f += 1;
                }
            }
        }
        println!("limit {limit}: {deep} too-deep errors, {bodies} bodies");
        assert!(deep > 0, "limit {limit}");
    }
}

/// A parser keeps at most its retained bytes from one file to the next (`Parser::set_retained_bytes`, the same
/// mechanism as C#'s), and a parser whose buffers were shrunk parses as a fresh one does.
#[test]
fn a_parser_shrinks_its_buffers_past_the_retained_bytes_and_parses_as_before() {
    let file = "/** doc {@link A#b(int)} */\nclass C<T> { int f(int x) { return java.util.List.of(1, 2).stream().map(y -> y * x).count() > 0 ? 1 : 0; } }\n";
    let big = file.repeat(20_000);
    let kind = FileKind::file(LanguageLevel::HIGHEST);
    let parse = |parser: &mut java_syntax::Parser, text: &str| {
        let tree = parser.parse(text, kind);
        let dump = |t: &Tree| java_syntax::dump::psi_to_string(t, "A.java");
        assert_eq!(dump(&tree), dump(&Tree::parse(text, kind)));
    };
    let mut parser = java_syntax::Parser::new();
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
