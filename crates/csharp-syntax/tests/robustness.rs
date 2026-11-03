//! Any bytes parse without a panic into a lossless tree on a 1 MB stack: random bytes and random fragments of C#, every
//! recursive form of the parser and the lexer nested 100 000 deep, every chain and list of 100 000 elements in linear
//! time, and the speculative forms (generic `<`, lambdas, casts, patterns, out variables) with their re-parses bounded.
//! The leaves tile the text, ids are in preorder and the links agree, a second parse gives the same tree, the lexer dump
//! and the tree dump run. The lexer is Roslyn's (its recursive scanners nest to `MAX_NESTED_SCAN_DEPTH`), and so is the
//! parser: past `MAX_RECURSION_DEPTH` levels of `_recursionDepth` it fails as Roslyn's does on an exhausted stack
//! (`CreateForGlobalFailure`: the whole text one skipped `BadToken`, `ERR_InsufficientStack`); DESIGN.md, "C# nesting",
//! compares the limit with Roslyn's own on its 64 MB stack.

use std::time::{Duration, Instant};

/// A wall-clock limit of these tests, multiplied by `PSI_SYNTAX_TIME_SCALE` (accio's CI sets it for its shared runners);
/// unset, the limit as measured on a developer's machine, which `scripts/gate.sh` holds.
fn limit(limit: std::time::Duration) -> std::time::Duration {
    limit * std::env::var("PSI_SYNTAX_TIME_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1)
}

use csharp_syntax::dump::{lex_to_string, tree_to_string};
use csharp_syntax::{DocumentationMode, Entry, ErrorCode, FileKind, LanguageVersion, NodeId, Parser, Tree};

/// The stack a parse is promised to fit (AGENTS.md, "The consumer contract"): 1 MB in a release build; a debug build's
/// frames are several times larger.
const STACK: usize = if cfg!(debug_assertions) { 16 << 20 } else { 1 << 20 };

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

/// Runs `f` on a `STACK` thread named `name`: a stack overflow aborts the test binary with the form's name.
fn on_stack<T: Send + 'static>(name: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().name(name.to_string()).stack_size(STACK).spawn(f).unwrap().join().unwrap()
}

/// The tree is lossless and consistent: the token leaves, in order, are the text; ids are preorder; every node's range
/// covers its children's; the child and sibling links agree with the parent links.
fn check(tree: &Tree, text: &[u8]) {
    assert!(tree.preorder(tree.root()).map(|(id, _)| id.0 as usize).eq(0..tree.len()), "ids are not preorder");
    let mut last_child: Vec<Option<NodeId>> = vec![None; tree.len()];
    for i in 1..tree.len() as u32 {
        let id = NodeId(i);
        let parent = tree.parent(id).expect("every node but the root has a parent");
        assert!(parent.0 < i && tree.subtree_end(parent).0 >= tree.subtree_end(id).0);
        let (p, r) = (tree.range(parent), tree.range(id));
        assert!(p.start <= r.start && r.end <= p.end, "{id:?} {r:?} is outside its parent's {p:?}");
        match last_child[parent.0 as usize] {
            None => assert_eq!(tree.first_child(parent), Some(id)),
            Some(previous) => assert_eq!(tree.next_sibling(previous), Some(id)),
        }
        last_child[parent.0 as usize] = Some(id);
    }
    for (i, last) in last_child.iter().enumerate() {
        assert_eq!(tree.last_child(NodeId(i as u32)), *last);
    }
    let mut previous_end = 0;
    let mut covered = Vec::with_capacity(text.len());
    for id in tree.tokens() {
        let range = tree.range(id);
        assert_eq!(range.start, previous_end, "leaves do not tile the text at {id:?}");
        previous_end = range.end;
        covered.extend_from_slice(tree.text(id));
    }
    assert!(covered == text, "the leaves are not the text");
    // the dump of a tree of a million nodes takes longer than its parse
    if tree.len() < 100_000 {
        let _ = tree_to_string(tree);
    }
}

/// The two trees have the same nodes: kinds, ranges, shape and diagnostics.
fn same_tree(a: &Tree, b: &Tree) -> bool {
    a.len() == b.len()
        && (0..a.len() as u32).all(|i| {
            let id = NodeId(i);
            a.kind(id) == b.kind(id) && a.range(id) == b.range(id) && a.subtree_end(id) == b.subtree_end(id)
        })
        && a.diagnostics() == b.diagnostics()
}

fn insufficient_stack(tree: &Tree) -> bool {
    tree.diagnostics().iter().any(|d| d.code == ErrorCode::ERR_InsufficientStack)
}

/// What a parse on a `STACK` thread measured: the first parse's time, whether it failed as an exhausted stack does,
/// the tree's depth and size.
struct Parsed {
    elapsed: Duration,
    too_deep: bool,
    depth: usize,
    nodes: usize,
}

/// Parses `text` twice on a `STACK` thread: the tree is well formed and lossless, and the second parse (by the same,
/// reused parser) gives the same tree (the limits are deterministic).
fn parse_on_small_stack(name: &str, text: String, file_kind: FileKind) -> Parsed {
    on_stack(name, move || {
        let mut parser = Parser::new();
        let start = Instant::now();
        let tree = parser.parse(text.as_bytes(), file_kind);
        let elapsed = start.elapsed();
        check(&tree, text.as_bytes());
        let again = parser.parse(text.as_bytes(), file_kind);
        assert!(same_tree(&tree, &again), "the second parse differs");
        let depth = tree.preorder(tree.root()).map(|(_, d)| d).max().unwrap_or(0);
        Parsed { elapsed, too_deep: insufficient_stack(&tree), depth, nodes: tree.len() }
    })
}

/// A field initializer.
fn field(expr: &str) -> String {
    format!("class C {{ object x = {expr}; }}\n")
}

/// Statements in a method body.
fn method(statements: &str) -> String {
    format!("class C {{ void M() {{ {statements} }} }}\n")
}

/// Statements in an async method's body (`await` is a keyword there).
fn async_method(statements: &str) -> String {
    format!("class C {{ async void M() {{ {statements} }} }}\n")
}

/// Every recursive form of the parser (and of the lexer under it), `n` levels deep: the text and where it sits.
pub fn nested_forms(n: usize) -> Vec<(&'static str, String)> {
    let r = |s: &str, k: usize| s.repeat(k);
    vec![
        // expressions
        ("parentheses", field(&format!("{}1{}", r("(", n), r(")", n)))),
        ("unary minus", field(&format!("{}1", r("- ", n)))),
        ("unary not", field(&format!("{}a", r("!", n)))),
        ("complement", field(&format!("{}a", r("~", n)))),
        ("prefix increments", field(&format!("{}a", r("++ ", n)))),
        ("index from end", field(&format!("{}a", r("^", n)))),
        ("address of", field(&format!("{}a", r("& ", n)))),
        ("dereference", field(&format!("{}a", r("*", n)))),
        ("ranges", field(&format!("{}a", r("..", n)))),
        ("await", async_method(&format!("{}a;", r("await ", n)))),
        ("casts", field(&format!("{}a", r("(A) ", n)))),
        ("predefined casts", field(&format!("{}1", r("(int) ", n)))),
        ("generic casts", field(&format!("{}a", r("(A<B>) ", n)))),
        ("tuple casts", field(&format!("{}a", r("((A, B)) ", n)))),
        ("calls", field(&format!("{}{}", r("f(", n), r(")", n)))),
        ("qualified calls", field(&format!("{}{}", r("a.f(", n), r(")", n)))),
        ("generic calls", field(&format!("{}{}", r("f<A>(", n), r(")", n)))),
        ("named arguments", field(&format!("{}1{}", r("f(a: ", n), r(")", n)))),
        ("ref arguments", field(&format!("{}a{}", r("f(ref ", n), r(")", n)))),
        ("out variables", field(&format!("{}{}", r("f(out var a, ", n), r(")", n)))),
        ("element accesses", field(&format!("{}0{}", r("a[", n), r("]", n)))),
        ("conditional accesses", field(&format!("{}0{}", r("a?[", n), r("]", n)))),
        ("object creations", field(&format!("{}{}", r("new A(", n), r(")", n)))),
        ("target-typed new", field(&format!("{}{}", r("new(", n), r(")", n)))),
        ("object initializers", field(&format!("{}1{}", r("new A { X = ", n), r(" }", n)))),
        ("collection initializers", field(&format!("{}1{}", r("new A { ", n), r(" }", n)))),
        ("nested initializers", field(&format!("new A {{ X = {}1{} }}", r("{ ", n), r(" }", n)))),
        ("anonymous objects", field(&format!("{}1{}", r("new { A = ", n), r(" }", n)))),
        ("array creations", field(&format!("{}1{}", r("new int[", n), r("]", n)))),
        ("array initializers", format!("class C {{ int[] x = {}1{}; }}\n", r("{", n), r("}", n))),
        ("array creation initializers", field(&format!("{}1{}", r("new[] { ", n), r(" }", n)))),
        ("stackalloc initializers", field(&format!("{}1{}", r("stackalloc[] { ", n), r(" }", n)))),
        ("collection expressions", field(&format!("{}1{}", r("[", n), r("]", n)))),
        ("spreads", field(&format!("{}1{}", r("[..", n), r("]", n)))),
        ("with expressions", field(&format!("{}1{}", r("a with { X = ", n), r(" }", n)))),
        ("lambdas", field(&format!("{}1", r("x => ", n)))),
        ("parenthesized lambdas", field(&format!("{}1", r("(a) => ", n)))),
        ("typed lambdas", field(&format!("{}1", r("(A a) => ", n)))),
        ("async lambdas", field(&format!("{}1", r("async x => ", n)))),
        ("static lambdas", field(&format!("{}1", r("static () => ", n)))),
        ("lambdas with return types", field(&format!("{}1", r("A () => ", n)))),
        ("lambda blocks", field(&format!("{}1{}", r("() => { return ", n), r("; }", n)))),
        ("anonymous methods", field(&format!("{}1{}", r("delegate { return ", n), r("; }", n)))),
        ("conditionals", field(&format!("{}c", r("a ? b : ", n)))),
        ("conditionals in the middle", field(&format!("{}b{}", r("a ? ", n), r(" : c", n)))),
        ("switch expressions", field(&format!("{}1{}", r("a switch { _ => ", n), r(" }", n)))),
        ("switch expression arms in parentheses", field(&format!("{}1{}", r("(a switch { _ => ", n), r(" })", n)))),
        ("assignments", method(&format!("{}a;", r("a = ", n)))),
        ("compound assignments", method(&format!("{}a;", r("a += ", n)))),
        ("coalescing assignments", method(&format!("{}a;", r("a ??= ", n)))),
        ("coalescing", field(&format!("{}a", r("a ?? ", n)))),
        ("throw expressions", field(&format!("{}a", r("a ?? throw ", n)))),
        ("interpolated strings", field(&format!("{}1{}", r("$\"{", n), r("}\"", n)))),
        ("verbatim interpolated strings", field(&format!("{}1{}", r("$@\"{", n), r("}\"", n)))),
        ("raw interpolated strings", field(&format!("{}1{}", r("$\"\"\"{", n), r("}\"\"\"", n)))),
        ("interpolation alignments", field(&format!("{}1{}", r("$\"{a,", n), r("}\"", n)))),
        ("typeof", field(&format!("typeof({}B{})", r("A<", n), r(">", n)))),
        ("default", field(&format!("default({}B{})", r("A<", n), r(">", n)))),
        ("sizeof", field(&format!("sizeof({}B{})", r("A<", n), r(">", n)))),
        ("nameof", field(&format!("{}a{}", r("nameof(", n), r(")", n)))),
        ("checked", field(&format!("{}a{}", r("checked(", n), r(")", n)))),
        ("refvalue", field(&format!("{}a, int{}", r("__refvalue(", n), r(")", n)))),
        ("makeref", field(&format!("{}a{}", r("__makeref(", n), r(")", n)))),
        ("tuples", field(&format!("{}1{}", r("(a, ", n), r(")", n)))),
        ("tuples first", field(&format!("{}1{}", r("(", n), r(", a)", n)))),
        ("ref expressions", method(&format!("ref int x = ref {}a;", r("b ? ref ", n)))),
        ("generic names", field(&format!("{}B{}.f", r("A<", n), r(">", n)))),
        ("generic method calls", field(&format!("f<{}B{}>()", r("A<", n), r(">", n)))),
        ("is types", field(&format!("a is {}B{}", r("A<", n), r(">", n)))),
        ("as types", field(&format!("a as {}B{}", r("A<", n), r(">", n)))),
        ("queries", field(&format!("{}a{}", r("from a in ", n), r(" select a", n)))),
        ("query selects", field(&format!("{}a", r("from a in b select ", n)))),
        ("query continuations", field(&format!("from a in b {}select a", r("group a by a into g ", n)))),
        ("query lets", field(&format!("from a in b {}select a", r("let c = from d in e ", n)))),
        // patterns
        ("recursive patterns", field(&format!("a is {}b{}", r("A(", n), r(")", n)))),
        ("property patterns", field(&format!("a is {}1{}", r("{ P: ", n), r(" }", n)))),
        ("extended property patterns", field(&format!("a is {}1{}", r("{ P.Q: ", n), r(" }", n)))),
        ("positional patterns", field(&format!("a is {}1{}", r("(", n), r(", 1)", n)))),
        ("parenthesized patterns", field(&format!("a is {}1{}", r("(", n), r(")", n)))),
        ("not patterns", field(&format!("a is {}1", r("not ", n)))),
        ("and patterns", field(&format!("a is {}1{}", r("1 and (", n), r(")", n)))),
        ("list patterns", field(&format!("a is {}1{}", r("[", n), r("]", n)))),
        ("slice patterns", field(&format!("a is {}1{}", r("[.. ", n), r("]", n)))),
        ("relational patterns", field(&format!("a is {}1", r("> ", n)))),
        ("var designations", field(&format!("a is var {}b{}", r("(", n), r(")", n)))),
        ("switch expression patterns", field(&format!("a switch {{ {}b{} => 1 }}", r("A(", n), r(")", n)))),
        ("case patterns", method(&format!("switch (a) {{ case {}b{}: break; }}", r("A(", n), r(")", n)))),
        ("deconstructions", method(&format!("var {}a{} = b;", r("(a, ", n), r(")", n)))),
        ("deconstruction declarations", method(&format!("{}a{} = b;", r("(var a, ", n), r(")", n)))),
        // statements
        ("blocks", method(&format!("{}{}", r("{ ", n), r("}", n)))),
        ("if", method(&format!("{};", r("if (a) ", n)))),
        ("if else", method(&format!("{};", r("if (a) ; else ", n)))),
        ("while", method(&format!("{};", r("while (a) ", n)))),
        ("do", method(&format!("{};{}", r("do ", n), r(" while (a);", n)))),
        ("for", method(&format!("{};", r("for (;;) ", n)))),
        ("foreach", method(&format!("{};", r("foreach (var a in b) ", n)))),
        ("using", method(&format!("{};", r("using (a) ", n)))),
        ("lock", method(&format!("{};", r("lock (a) ", n)))),
        ("fixed", method(&format!("{};", r("fixed (int* p = a) ", n)))),
        ("checked blocks", method(&format!("{}{}", r("checked { ", n), r("}", n)))),
        ("unsafe blocks", method(&format!("{}{}", r("unsafe { ", n), r("}", n)))),
        ("labels", method(&format!("{};", r("a: ", n)))),
        ("try", method(&format!("{}{}", r("try { ", n), r("} finally { }", n)))),
        ("catch", method(&format!("{}{}", r("try { } catch (E e) { ", n), r("}", n)))),
        ("catch filters", method(&format!("{}{}", r("try { } catch when (", n), r(") { }", n)))),
        ("switch statements", method(&format!("{}{}", r("switch (a) { case 1: ", n), r("}", n)))),
        ("local functions", method(&format!("{}{}", r("void g() { ", n), r("}", n)))),
        ("expression-bodied local functions", method(&format!("{}1;", r("int g() => ", n)))),
        ("local declarations", method(&format!("{}1;", r("var a = ", n)))),
        ("yield", method(&format!("{}1;", r("yield return ", n)))),
        // declarations
        ("namespaces", format!("{}{}\n", r("namespace A { ", n), r("}", n))),
        ("classes", format!("{}{}\n", r("class A { ", n), r("}", n))),
        ("structs", format!("{}{}\n", r("struct A { ", n), r("}", n))),
        ("interfaces", format!("{}{}\n", r("interface A { ", n), r("}", n))),
        ("records", format!("{}{}\n", r("record A() { ", n), r("}", n))),
        ("extensions", format!("static class E {{ {}{} }}\n", r("extension(int a) { ", n), r("}", n))),
        ("methods in classes", format!("{}{}\n", r("class A { void f() { ", n), r("} }", n))),
        ("property accessors", format!("{}1{}\n", r("class A { int P { get { return ", n), r("; } } }", n))),
        ("lambdas in members", format!("{}1{}\n", r("class A { int P => ", n), r("; }", n))),
        ("attributes", format!("{}1{} class C {{}}\n", r("[A(", n), r(")]", n))),
        ("attribute arguments", format!("[A({}1{})] class C {{}}\n", r("f(", n), r(")", n))),
        ("attributes on parameters", format!("{}{}\n", r("class A { void f([B] int a) { ", n), r("} }", n))),
        ("attributes on lambdas", field(&format!("{}1", r("[A] () => ", n)))),
        ("enums in classes", format!("{}{}\n", r("class A { enum E { B } ", n), r("}", n))),
        ("delegates in classes", format!("{}{}\n", r("class A { delegate void D(); ", n), r("}", n))),
        // types
        ("generics", format!("class C {{ {}B{} x; }}\n", r("A<", n), r(">", n))),
        ("generics with arguments", format!("class C {{ {}B{} x; }}\n", r("A<B, ", n), r(">", n))),
        ("nullable generics", format!("class C {{ {}B{} x; }}\n", r("A<", n), r("?>", n))),
        ("array generics", format!("class C {{ {}B{} x; }}\n", r("A<", n), r("[]>", n))),
        ("tuple types", format!("class C {{ {}A{} x; }}\n", r("(A, ", n), r(")", n))),
        ("function pointers", format!("class C {{ {}A{} x; }}\n", r("delegate*<", n), r(">", n))),
        ("type parameter constraints", format!("class C<T> where T : {}B{} {{}}\n", r("A<", n), r(">", n))),
        ("base types", format!("class C : {}B{} {{}}\n", r("A<", n), r(">", n))),
        ("array rank expressions", format!("class C {{ int[{}1{}] x; }}\n", r("a[", n), r("]", n))),
        ("local generic types", method(&format!("{}B{} x;", r("A<", n), r(">", n)))),
        ("generic parameters", format!("class C {{ void f({}B{} a) {{}} }}\n", r("A<", n), r(">", n))),
        // documentation comments, directives
        ("doc elements", format!("/// {}x{}\nclass C {{}}\n", r("<a>", n), r("</a>", n))),
        ("doc crefs", format!("/// <see cref=\"{}B{}\"/>\nclass C {{}}\n", r("A{", n), r("}", n))),
        ("doc cref parameters", format!("/// <see cref=\"M({}B{})\"/>\nclass C {{}}\n", r("A{", n), r("}", n))),
        ("doc extension crefs", format!("/// <see cref=\"{}M\"/>\nclass C {{}}\n", r("extension(int).", n))),
        ("directive parentheses", format!("#if {}A{}\n#endif\n", r("(", n), r(")", n))),
        ("directive not", format!("#if {}A\n#endif\n", r("!", n))),
        ("nested if directives", format!("#if B\n{}{}#endif\n", r("#if A\n", n), r("#endif\n", n))),
        ("active if directives", format!("{}{}", r("#if true\n", n), r("#endif\n", n))),
        ("regions", format!("{}{}", r("#region\n", n), r("#endregion\n", n))),
        // speculative forms nested
        ("generic comparisons", field(&format!("{}b{}", r("f(a < ", n), r(", c > d)", n)))),
        ("lambda parameter lists", field(&format!("{}1{}", r("((a, b) => ", n), r(")", n)))),
        ("parenthesized casts", field(&format!("{}a{}", r("((A)", n), r(")", n)))),
        ("cast-looking calls", field(&format!("{}1{}", r("(a)(", n), r(")", n)))),
        ("out declaration types", field(&format!("{}{}", r("f(out A<B> a, ", n), r(")", n)))),
        ("java-like reparse", field(&format!("{}1{}", r("(a<b>c(", n), r(")) + 1", n)))),
        ("patterns with types", field(&format!("{}1{}", r("a is A<B> b && f(", n), r(")", n)))),
        ("switch arms with types", field(&format!("{}1{}", r("a switch { A<B> b => ", n), r(" }", n)))),
    ]
}

/// Every chain (a loop of the parser) and list of `n` elements.
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
        ("generic-looking calls", field(&format!("a{}", r("<b>(c)")))),
        ("is", field(&format!("a{}", r(" is A")))),
        ("as", field(&format!("a{}", r(" as A")))),
        ("is patterns", field(&format!("a{} 1", r(" is 1 or")))),
        ("pattern combinators", field(&format!("a is 1{}", r(" or 1")))),
        ("pattern and", field(&format!("a is 1{}", r(" and not 1")))),
        ("postfix ++", field(&format!("a{}", r("++")))),
        ("postfix --", field(&format!("a{}", r("--")))),
        ("null forgiving", field(&format!("a{}", r("!")))),
        ("a.b", field(&format!("a{}", r(".b")))),
        ("a?.b", field(&format!("a{}", r("?.b")))),
        ("a->b", field(&format!("a{}", r("->b")))),
        ("a.f()", field(&format!("a{}", r(".f()")))),
        ("a?.f()", field(&format!("a{}", r("?.f()")))),
        ("a[0]", field(&format!("a{}", r("[0]")))),
        ("a?[0]", field(&format!("a{}", r("?[0]")))),
        ("a::b", field(&format!("a{}", r("::b")))),
        ("switch", field(&format!("a{}", r(" switch { _ => 1 }")))),
        ("with", field(&format!("a{}", r(" with { }")))),
        ("arguments", field(&format!("f(a{})", r(", a")))),
        ("named arguments list", field(&format!("f(a: 1{})", r(", a: 1")))),
        ("array elements", format!("class C {{ int[] x = {{a{}}}; }}\n", r(", a"))),
        ("collection elements", field(&format!("[a{}]", r(", a")))),
        ("initializer members", field(&format!("new A {{ X = 1{} }}", r(", X = 1")))),
        ("tuple elements", field(&format!("(a{})", r(", a")))),
        ("interpolation holes", field(&format!("$\"{}\"", r("{a}")))),
        ("string concatenation", field(&format!("\"a\"{}", r(" + \"a\"")))),
        ("switch arms", field(&format!("a switch {{ {} }}", r("1 => 1, ")))),
        ("list pattern elements", field(&format!("a is [1{}]", r(", 1")))),
        ("property subpatterns", field(&format!("a is {{ P: 1{} }}", r(", P: 1")))),
        ("query clauses", field(&format!("from a in b {}select a", r("where a ")))),
        ("query lets", field(&format!("from a in b {}select a", r("let c = a ")))),
        ("query orderings", field(&format!("from a in b orderby a{} select a", r(", a")))),
        ("query joins", field(&format!("from a in b {}select a", r("join c in d on a equals c ")))),
        ("query froms", field(&format!("from a in b {}select a", r("from c in d ")))),
        ("statements", method(&r("a(); "))),
        ("declarations", method(&r("int a; "))),
        ("declarators", method(&format!("int a{};", r(", a")))),
        ("empty statements", method(&r(";"))),
        ("else if", method(&format!("if (a) ;{}", r(" else if (a) ;")))),
        ("switch sections", method(&format!("switch (a) {{ {} }}", r("case 1: break; ")))),
        ("case labels", method(&format!("switch (a) {{ {}break; }}", r("case 1: ")))),
        ("catch clauses", method(&format!("try {{ }}{}", r(" catch { }")))),
        ("for incrementors", method(&format!("for (;; a++{}) ;", r(", a++")))),
        ("local functions list", method(&r("void g() { } "))),
        ("members", format!("class C {{ {} }}\n", r("int a; "))),
        ("methods", format!("class C {{ {} }}\n", r("void f() { } "))),
        ("types", r("class C { }\n")),
        ("enum members", format!("enum E {{ {} }}\n", r("A, "))),
        ("parameters", format!("class C {{ void f(int a{}) {{}} }}\n", r(", int a"))),
        ("type parameters", format!("class C<T{}> {{}}\n", r(", T"))),
        ("type arguments", format!("class C {{ A<B{}> x; }}\n", r(", B"))),
        ("constraint clauses", format!("class C<T> {}{{}}\n", r("where T : A "))),
        ("constraints", format!("class C<T> where T : A{} {{}}\n", r(", A"))),
        ("base types list", format!("class C : A{} {{}}\n", r(", A"))),
        ("qualified names", format!("class C {{ A{} x; }}\n", r(".A"))),
        ("alias qualified names", format!("class C {{ A::A{} x; }}\n", r(".A"))),
        ("array ranks", format!("class C {{ int{} x; }}\n", r("[]"))),
        ("array rank commas", format!("class C {{ int[{}] x; }}\n", r(","))),
        ("pointers", format!("class C {{ int{} x; }}\n", r("*"))),
        ("modifiers", format!("{} class C {{}}\n", r("public "))),
        ("attribute lists", format!("{} class C {{}}\n", r("[A] "))),
        ("attributes in a list", format!("[A{}] class C {{}}\n", r(", A"))),
        ("attribute arguments list", format!("[A(1{})] class C {{}}\n", r(", 1"))),
        ("usings", r("using A;\n")),
        ("externs", r("extern alias A;\n")),
        ("namespace members", format!("namespace N {{ {} }}\n", r("class C { } "))),
        ("file-scoped namespace members", format!("namespace N;\n{}", r("class C { } "))),
        ("top-level statements", r("a();\n")),
        ("accessors", format!("class C {{ int P {{ {} }} }}\n", r("get; "))),
        ("doc lines", format!("{}class C {{}}\n", r("/// <a/>\n"))),
        ("doc attributes", format!("/// <a {}/>\nclass C {{}}\n", r("b=\"\" "))),
        ("if directives", r("#if A\n#endif\n")),
        ("regions list", r("#region\n#endregion\n")),
        ("pragmas", r("#pragma warning disable CS1\n")),
        ("line comments", r("// a\n")),
        ("raw string lines", field(&format!("\"\"\"\n{}\"\"\"", r("a\n")))),
        ("skipped tokens", method(&r(") "))),
        ("bad members", format!("class C {{ {} }}\n", r(") "))),
    ]);
    chains
}

/// The forms whose levels each speculate over what follows them (a reset point, a scan or a parse, a rollback): the
/// work of nested speculation grows as the square of the depth or faster.
const SPECULATIVE: [&str; 20] = [
    "conditional accesses",
    "collection expressions",
    "spreads",
    "attributes",
    "parentheses",
    "tuples",
    "tuples first",
    "positional patterns",
    "parenthesized patterns",
    "deconstruction declarations",
    "ref expressions",
    "generic comparisons",
    "lambda parameter lists",
    "parenthesized casts",
    "cast-looking calls",
    "out declaration types",
    "java-like reparse",
    "patterns with types",
    "switch arms with types",
    "conditionals in the middle",
];

/// Every recursive form nested 100 000 deep parses on a 1 MB stack, in under a second, into a well-formed, lossless
/// tree; past the limit the parse fails as Roslyn's does when its stack runs out; the same text parses to the same tree
/// again.
#[test]
fn deeply_nested_forms_parse_on_a_small_stack() {
    let n = 100_000;
    // No limit is reached: loops of the parser (`else if`, a relational pattern's operand, a skipped `{` of an
    // initializer, declarations and `yield` that do not nest), the lexer's own nesting (bounded by
    // `MAX_NESTED_SCAN_DEPTH`), directives (a stack of the lexer, not a recursion), the doc comment parser's elements.
    let flat = [
        "if else",
        "relational patterns",
        "nested initializers",
        "expression-bodied local functions",
        "local declarations",
        "yield",
        "doc elements",
        "doc crefs",
        "doc cref parameters",
        "doc extension crefs",
        "directive parentheses",
        "directive not",
        "nested if directives",
        "active if directives",
        "regions",
    ];
    let mut slowest = (Duration::ZERO, "");
    let mut failures = Vec::new();
    for (name, text) in nested_forms(n) {
        let parsed = parse_on_small_stack(name, text, FileKind::default());
        println!(
            "{name}: {:.1} ms, depth {}, {} nodes{}",
            parsed.elapsed.as_secs_f64() * 1000.0,
            parsed.depth,
            parsed.nodes,
            if parsed.too_deep { ", too deep" } else { "" }
        );
        if !parsed.too_deep && !flat.contains(&name) {
            failures.push(format!("{name}: no failure at the limit"));
        }
        if parsed.elapsed >= limit(Duration::from_secs(1)) {
            failures.push(format!("{name}: {:?}", parsed.elapsed));
        }
        slowest = slowest.max((parsed.elapsed, name));
    }
    println!("slowest: {} {:?}", slowest.1, slowest.0);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The chains that fail as too deep: `a < a < …` is scanned as nested type argument lists (`ScanType` recurses at each
/// `<`, `MAX_RECURSION_DEPTH`), `a?[0]?[0]…` speculates over the rest at every `?[`, twice nested (the work budget;
/// 17 of them reach it).
const TOO_DEEP_CHAINS: [&str; 3] = ["<", "<, no spaces", "a?[0]"];

/// Every chain of 100 000 operators and every list of 100 000 elements parses in linear time on a 1 MB stack: a
/// binary operator's chain is built in a loop (`ParseExpressionContinued`), a postfix chain wraps the expression so
/// far in a loop (`ParsePostFixExpression`), and the tree, 100 000 deep, is flattened without recursion.
#[test]
fn long_chains_parse_in_linear_time() {
    let n = 100_000;
    let mut failures = Vec::new();
    for (name, text) in chains(n) {
        let small = text.len();
        let parsed = parse_on_small_stack(name, text, FileKind::default());
        println!(
            "{name}: {:.1} ms, depth {}, {} nodes, {:.1} MB/s{}",
            parsed.elapsed.as_secs_f64() * 1000.0,
            parsed.depth,
            parsed.nodes,
            small as f64 / parsed.elapsed.as_secs_f64() / 1e6,
            if parsed.too_deep { ", too deep" } else { "" }
        );
        if parsed.too_deep != TOO_DEEP_CHAINS.contains(&name) {
            failures.push(format!("{name}: too deep {}", parsed.too_deep));
        }
        if parsed.elapsed >= limit(Duration::from_secs(1)) {
            failures.push(format!("{name}: {:?}", parsed.elapsed));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Where Roslyn speculates, each level of a nested form may scan or parse what follows it before it is rolled back:
/// `a?[a?[…` parses the rest twice at every level (Roslyn did not finish 900 levels in 25 minutes), `[[[…` parses
/// an attribute list over the rest at every level. The work budget (`green::work_budget_for`) bounds every parse:
/// past it the parse fails as it does when the stack runs out. Each form at 250, 1000 and 100 000 levels parses in
/// under a second; the time per level is printed.
#[test]
fn speculation_is_bounded() {
    let mut failures = Vec::new();
    for name in SPECULATIVE {
        let mut times = Vec::new();
        for n in [250, 1000, 100_000] {
            let text = nested_forms(n).into_iter().find(|(f, _)| *f == name).unwrap().1;
            let parsed = parse_on_small_stack(name, text, FileKind::default());
            if parsed.elapsed >= limit(Duration::from_secs(1)) {
                failures.push(format!("{name} at {n} levels: {:?}", parsed.elapsed));
            }
            times.push((n, parsed.elapsed, parsed.too_deep));
        }
        let row: Vec<String> = times
            .iter()
            .map(|(n, t, deep)| {
                format!(
                    "{n}: {:.2} ms ({:.2} us a level){}",
                    t.as_secs_f64() * 1e3,
                    t.as_secs_f64() * 1e6 / *n as f64,
                    if *deep { ", too deep" } else { "" }
                )
            })
            .collect();
        println!("{name}: {}", row.join(", "));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A literal in the holes of literals each in a speculative form (`a ? [` parses what follows twice) is met twice as
/// often at every level: it is made into an expression once (`ParsedStrings`), where it was made, scanned and its holes
/// lexed at every meeting (16 levels around a 20 KB string took 5.2 s, each level doubling). Each form at 8, 16 and 24
/// levels parses in under a quarter of a second.
#[test]
fn literals_nested_in_speculation_are_made_once() {
    let forms = [("$\"{a ? [", "]}\""), ("$\"\"\"{[(", ")]}\"\"\""), ("$@\"{f(a = ", ")}\"")];
    let mut failures = Vec::new();
    for (open, close) in forms {
        for n in [8, 16, 24] {
            let text = field(&format!("{}\"{}\"{}", open.repeat(n), "x".repeat(20_000), close.repeat(n)));
            let parsed = parse_on_small_stack(open, text, FileKind::default());
            println!("{open} {n} levels: {:.2} ms", parsed.elapsed.as_secs_f64() * 1e3);
            if parsed.elapsed >= limit(Duration::from_millis(250)) {
                failures.push(format!("{open} at {n} levels: {:?}", parsed.elapsed));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Random mixes of the recursive expression and statement forms nested past the limit parse on a 1 MB stack: the
/// stack a level takes depends on the path, and a mix takes paths no single form does.
#[test]
fn random_mixes_parse_on_a_small_stack() {
    let forms: &[(&str, &str)] = &[
        ("(", ")"),
        ("- ", ""),
        ("(A) ", ""),
        ("(A<B>) ", ""),
        ("f(", ")"),
        ("a.f(", ")"),
        ("a[", "]"),
        ("new A(", ")"),
        ("new A { X = ", " }"),
        ("new[] { ", " }"),
        ("[", "]"),
        ("x => ", ""),
        ("(a) => ", ""),
        ("(A a) => ", ""),
        ("() => { return ", "; }"),
        ("delegate { return ", "; }"),
        ("() => { if (a) { var x = ", "; } return 1; }"),
        ("() => { switch (a) { case 1: return ", "; } }"),
        ("() => { void g() { return ", "; } }"),
        ("() => { try { return ", "; } finally { } }"),
        ("a ? b : ", ""),
        ("a ? ", " : c"),
        ("a = ", ""),
        ("a ?? ", ""),
        ("a switch { _ => ", " }"),
        ("a switch { A(B b) => ", " }"),
        ("$\"{", "}\""),
        ("$\"\"\"{", "}\"\"\""),
        ("from a in ", " select a"),
        ("a with { X = ", " }"),
        ("(a, ", ")"),
        ("f(out var a, ", ")"),
        ("f<A<B>>(", ")"),
        ("checked(", ")"),
        ("typeof(A<B>) is { P: ", " }"),
        ("a is A(", ") b"),
        ("a is [", "] b"),
        ("a is not (", ")"),
    ];
    let mut rng = Rng(0x2545F4914F6CDD1D);
    let mut too_deep = 0;
    for round in 0..40 {
        let (mut open, mut close) = (String::new(), Vec::new());
        for _ in 0..3000 {
            let (o, c) = forms[rng.below(forms.len())];
            open.push_str(o);
            close.push(c);
        }
        close.reverse();
        let text = field(&format!("{open}1{}", close.concat()));
        let parsed = parse_on_small_stack(&format!("mix {round}"), text, FileKind::default());
        assert!(parsed.elapsed < limit(Duration::from_secs(1)), "round {round}: {:?}", parsed.elapsed);
        too_deep += usize::from(parsed.too_deep);
    }
    // an interpolated string past the lexer's `MAX_NESTED_SCAN_DEPTH` takes the rest of the line as its text, short of
    // the limit (16 of the 40 reach it)
    println!("{too_deep} of 40 mixes reached the limit");
    assert!(too_deep > 0);
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
    "new",
    "switch",
    "case",
    "default",
    "is",
    "not",
    "and",
    "or",
    "when",
    "from",
    "select",
    "where",
    "let",
    "record",
    "struct",
    "interface",
    "namespace",
    "using",
    "static",
    "delegate",
    "*",
    "&",
    "..",
    "with",
    "extension",
    "ref",
    "out",
    "scoped",
    "params",
    "required",
    "field",
];

/// Any bytes and random mixes of C# punctuation, keywords, literals, trivia and doc comment pieces parse and lex, at
/// every entry and documentation mode, without a panic into a well-formed, lossless tree.
#[test]
fn random_bytes_and_fragments_parse_losslessly() {
    on_stack("random", || {
        let mut rng = Rng(0x9E3779B97F4A7C15);
        let mut parser = Parser::new();
        let entries = [
            Entry::CompilationUnit,
            Entry::CompilationUnit,
            Entry::Expression,
            Entry::Statement,
            Entry::Member,
            Entry::TypeName,
            Entry::Name,
            Entry::ParameterList,
            Entry::BracketedParameterList,
            Entry::ArgumentList,
            Entry::BracketedArgumentList,
            Entry::AttributeArgumentList,
        ];
        let versions = [LanguageVersion::LATEST, LanguageVersion::Preview, LanguageVersion::CSharp1];
        for round in 0..4000 {
            let mut text = Vec::new();
            let len = rng.below(200);
            if round % 3 == 0 {
                text.extend((0..len).map(|_| rng.next() as u8));
            } else {
                for _ in 0..len / 3 {
                    text.extend_from_slice(FRAGMENTS[rng.below(FRAGMENTS.len())].as_bytes());
                }
            }
            let entry = entries[round % entries.len()];
            let version = versions[(round / entries.len()) % versions.len()];
            let doc_mode = [DocumentationMode::Parse, DocumentationMode::None, DocumentationMode::Diagnose][round % 3];
            let file_kind = FileKind { version, entry, doc_mode };
            let tree = parser.parse(&text, file_kind);
            // a partial entry's root may leave text out, as Roslyn's does (tools/csharp-oracle/README.md)
            if entry == Entry::CompilationUnit {
                check(&tree, &text);
            } else {
                let _ = tree_to_string(&tree);
            }
            let lexed = parser.lex(&text, FileKind { doc_mode, ..FileKind::default() });
            check(&lexed, &text);
            let _ = lex_to_string(&lexed);
        }
    });
}

/// The lexer's own recursion: holes of interpolated strings and brackets in them, directive expressions, nested
/// excluded `#if` sections, documentation comments, 100 000 deep, lexed and parsed.
#[test]
fn deep_lexer_forms_lex_losslessly() {
    on_stack("lexer forms", || {
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
            // documentation comments: nested elements, cref type arguments and extension members, many attributes
            format!("/// {}x{}\nclass C {{}}", "<a>".repeat(n), "</a>".repeat(n)),
            format!("/** {}x{} */\nclass C {{}}", "<a>".repeat(n), "</a>".repeat(n)),
            format!("/// <see cref=\"{}B{}\"/>\nclass C {{}}", "A{".repeat(n), "}".repeat(n)),
            format!("/// <see cref=\"M({}B{})\"/>\nclass C {{}}", "A{".repeat(n), "}".repeat(n)),
            format!("/// <see cref=\"{}M\"/>\nclass C {{}}", "extension(int).".repeat(n)),
            format!("/// <a {}/>\nclass C {{}}", (0..n).map(|i| format!("b{i}=\"\" ")).collect::<String>()),
            format!("/// <a {}/>\nclass C {{}}", "b=\"\" ".repeat(n)),
        ];
        for text in &texts {
            let lexed = parser.lex(text, FileKind::default());
            check(&lexed, text.as_bytes());
            let diagnosed = parser.lex(text, FileKind { doc_mode: DocumentationMode::Diagnose, ..FileKind::default() });
            check(&diagnosed, text.as_bytes());
            let tree = parser.parse(text, FileKind::default());
            check(&tree, text.as_bytes());
        }
    });
}
