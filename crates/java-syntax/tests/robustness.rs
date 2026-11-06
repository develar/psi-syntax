//! Deeply nested and long inputs parse on a small stack in linear time: every recursive form of the Java parsers
//! nested 100 000 deep, every left-associative chain of 100 000 operators, and the goldens under tiny nesting limits.

mod common;

use java_syntax::{FileKind, LanguageLevel, Tree};
use psi_check::{XorShift, check_leaves_are_text, check_links, limit, on_stack, same_subtree, too_deep_errors};

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
    let mut chains = psi_check::operator_chains(n, field);
    chains.extend([
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
    check_links(tree, |id, previous| assert_eq!(tree.previous_sibling(id), previous));
    check_leaves_are_text(tree, text);
}

const KIND: FileKind = FileKind { level: LanguageLevel::JDK_21_PREVIEW, entry: java_syntax::Entry::File };

/// The forms whose rolled-back attempt scans ahead at every level (see `lambda_parameter_recovery_is_bounded`).
const SCANNING_FORMS: [&str; 2] = ["typed lambda reparse", "cast reparse"];

/// Parses `text` twice on a `STACK` thread: the time of the first parse, the too-deep errors, the tree's depth; the
/// tree is well formed and lossless, and the second parse gives the same tree (the limits are deterministic).
fn parse_on_small_stack(name: &str, text: String) -> (std::time::Duration, usize, usize) {
    on_stack(name, move || {
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
        let (elapsed, errors, _) = parse_on_small_stack(name, text);
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
        let (elapsed, _, depth) = parse_on_small_stack(name, text);
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
        let (elapsed, errors, _) = parse_on_small_stack(name, text);
        println!("{name}: {:.1} ms at 10 000 levels, {errors} too-deep errors", elapsed.as_secs_f64() * 1000.0);
        assert!(errors > 0, "{name}: no error at the limit");
        assert!(elapsed < limit(std::time::Duration::from_millis(10_000)), "{name}: {elapsed:?}");
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
        let text = field(&psi_check::nested_mix(forms, 3000, |n| (rng.next() % n as u64) as usize));
        let (elapsed, errors, _) = parse_on_small_stack("mix", text);
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
        let text = rng.bytes_or_pieces(round, pieces);
        let tree = Tree::parse(&text, FileKind::default());
        check_tree(&tree, &text);
    }
}

/// The goldens of `testdata/java`, each with its text without `\r`s, trimmed when its test trims it.
fn goldens() -> Vec<(String, Vec<u8>, FileKind)> {
    let text = |g: &common::Golden| if g.trim { common::trim(g.text_without_cr()) } else { g.text_without_cr() };
    common::goldens().iter().map(|g| (g.path.clone(), text(g), FileKind { level: g.level, entry: g.entry })).collect()
}

/// With a tiny nesting limit, ordinary code takes the too-deep paths everywhere: every golden still parses without a
/// panic into a well-formed, lossless tree, and body-less parsing then `parse_body` gives the full parse's subtree.
#[test]
fn tiny_nesting_limits_keep_trees_lossless() {
    use java_syntax::{ParseOptions, Parser};
    let goldens = goldens();
    for limit in [1, 2, 3, 5, 8, 13] {
        let (mut parser, mut body_parser): (Parser, Parser) =
            (psi_check::nesting_limited(limit), psi_check::nesting_limited(limit));
        let (mut deep, mut bodies) = (0, 0);
        for (name, text, kind) in &goldens {
            let full = parser.parse(text, *kind);
            check_tree(&full, text);
            deep += too_deep_errors(&full);
            let lazy = parser.parse_with_options(text, *kind, ParseOptions { expand_bodies: false });
            bodies += psi_check::each_body(name, &lazy, &full, |ln, fnode| {
                let body = body_parser.parse_body(&lazy, ln, ParseOptions::default()).unwrap();
                assert!(same_subtree(&body, body.root(), &full, fnode), "{name}: body {:?}", lazy.range(ln));
            });
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
    psi_check::check_retained_bytes(file, &big, |parser: &mut java_syntax::Parser, text: &str| {
        let tree = parser.parse(text, kind);
        let dump = |t: &Tree| java_syntax::dump::psi_to_string(t, "A.java");
        assert_eq!(dump(&tree), dump(&Tree::parse(text, kind)));
    });
}

/// The levels every recursive form reaches before the stack stops it (`psi_builder::stack`): the depth limit, a count,
/// is the same bound on every target, while the stack a level takes is the compiler's (885 KB at the limit on macOS
/// arm64, 1097 KB on Windows x64 for the same form, before the stack bound). So below `MIN_LEVELS` levels the tree is
/// the same on every target this runs on (`Parenth2.java`, the deepest community file, is parentheses, which reach the
/// limit on all of them). Prints each form's levels and stack, on a 1 MB stack.
#[test]
fn the_depth_limit_holds_before_the_stack_runs_out() {
    const MIN_LEVELS: u32 = 800;
    let failures = psi_check::depth_limit_failures(nested_forms(100_000), MIN_LEVELS, |text| {
        let mut parser = java_syntax::Parser::new();
        let _ = parser.parse(text, KIND);
        parser.stack_use()
    });
    assert!(failures.is_empty(), "{failures:#?}");
}
