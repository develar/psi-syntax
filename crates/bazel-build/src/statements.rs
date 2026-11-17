//! The statements of a BUILD or MODULE file as values, read without evaluating them: `load`, top-level assignments
//! and calls, each argument an expression tree of literals, names and calls (bazelbuild/starlark `spec.md`,
//! "Expressions"), read from psi-syntax's Starlark tree, the one every consumer reads (decision
//! 2026-11-07-bazel-reads-the-starlark-tree). A BUILD file is declarations only, no `def`, `if` or `for` (Bazel's BUILD
//! dialect), so a statement that is none of the three, or that holds a syntax error, makes the file `partial`: targets
//! it declares may be unseen. An expression the reader does not model (a comprehension, a subscript, a lambda) is kept
//! as the sub-expressions it holds, so every literal a target names is still seen.
//!
//! Lines are 1-based. The model holds no offsets, so a file's statements can be kept as a record and read back
//! (accio's `crates/bazel/src/facts.rs`); the byte ranges of the arguments are the reading's alone (`Parsed::args`).
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use starlark_syntax::{FileKind, NodeId, NodeKind as N, Parser, TokenKind as T, Tree};

/// A parsed file.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct File {
    pub loads: Vec<Load>,
    /// Top-level `NAME = value` (`+=` appends: the value is `old + new`), the last binding of a name.
    pub assigns: HashMap<String, Expr>,
    /// Top-level calls, in order.
    pub calls: Vec<Call>,
    /// Calls the file makes inside an expression statement that is no call (a comprehension of rule calls): their
    /// arguments are seen, their names may be computed.
    pub nested: Vec<Call>,
    /// A statement it could not read.
    pub unread: bool,
}

impl File {
    /// May the file declare a target the reader cannot name: a statement it could not read, or a call whose name is
    /// computed.
    pub fn partial(&self) -> bool {
        self.unread || !self.nested.is_empty()
    }

    /// Each name the file's `load` statements bind, with the module and the symbol it is loaded from.
    pub fn loaded(&self) -> HashMap<String, (String, String)> {
        self.loads.iter().flat_map(|l| l.names.iter().map(move |(local, sym)| (local.clone(), (l.module.clone(), sym.clone())))).collect()
    }
}

/// `load("<module>", "a", b = "c")`: each local name with the symbol it binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Load {
    pub module: String,
    pub names: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The callee as written: `jvm_library`, `native.glob`; `.format` on a receiver is `Expr::Method`.
    pub func: String,
    pub args: Vec<Arg>,
    pub line: u32,
}

impl Call {
    /// The argument named `name`.
    pub fn kw(&self, name: &str) -> Option<&Expr> {
        self.args.iter().find(|a| a.name.as_deref() == Some(name)).map(|a| &a.value)
    }

    /// The positional argument `i`.
    pub fn pos(&self, i: usize) -> Option<&Expr> {
        self.args.iter().filter(|a| a.name.is_none()).nth(i).map(|a| &a.value)
    }

    /// The literal string of the argument named `name`.
    pub fn kw_str(&self, name: &str) -> Option<&str> {
        match self.kw(name)? {
            Expr::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    /// The keyword; `*` and `**` for an unpacked argument; None for a positional one.
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Str(String),
    /// A list or a tuple.
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Name(String),
    Call(Call),
    /// `recv.name(args)`.
    Method(Box<Expr>, String, Vec<Arg>),
    /// `recv.name`.
    Attr(Box<Expr>, String),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    /// `a if c else b`: either value.
    Cond(Box<Expr>, Box<Expr>),
    /// What the reader does not model, as the expressions it holds: a number, `not x`, a comprehension, a
    /// subscript, a lambda's body.
    Other(Vec<Expr>),
}

impl Expr {
    /// Every expression below this one and this one, depth first.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        f(self);
        match self {
            Expr::Str(_) | Expr::Name(_) => {}
            Expr::List(v) | Expr::Other(v) => v.iter().for_each(|e| e.walk(f)),
            Expr::Dict(v) => v.iter().for_each(|(k, e)| {
                k.walk(f);
                e.walk(f)
            }),
            Expr::Call(c) => c.args.iter().for_each(|a| a.value.walk(f)),
            Expr::Method(r, _, args) => {
                r.walk(f);
                args.iter().for_each(|a| a.value.walk(f))
            }
            Expr::Attr(r, _) => r.walk(f),
            Expr::Bin(_, a, b) | Expr::Cond(a, b) => {
                a.walk(f);
                b.walk(f)
            }
        }
    }
}

/// The binary operators the model names: precedence decides no value it models (`+` of lists and of strings is all it
/// reads), and every operand is kept; a keyword operator (`in`, `and`, `or`, `is`, `not in`) is `op`.
const BINARY: [&str; 18] = ["+", "-", "*", "/", "//", "%", "**", "|", "&", "^", "<<", ">>", "==", "!=", "<=", ">=", "<", ">"];

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// The file `src` as values: its tree read statement by statement at the top level.
pub fn parse(src: &[u8]) -> File {
    read(src, |_, parsed| parsed.file)
}

/// A file as values with what the reading saw of its text.
pub(crate) struct Parsed {
    pub file: File,
    /// The byte ranges of the arguments of each of `file.calls`, in the order of its `args`.
    pub args: Vec<Vec<Range<usize>>>,
}

/// `src` read by the thread's parser, its tree and what it holds handed to `f`. A byte offset of the tree is the
/// offset in `src`: the document text keeps every offset (`starlark_syntax::document_text`).
pub(crate) fn read<R>(src: &[u8], f: impl FnOnce(&Tree, Parsed) -> R) -> R {
    let text = starlark_syntax::document_text(src);
    let tree = PARSER.with(|parser| parser.borrow_mut().parse(&text, FileKind::Build));
    let lines: Vec<usize> = std::iter::once(0).chain(text.iter().enumerate().filter(|&(_, b)| *b == b'\n').map(|(i, _)| i + 1)).collect();
    let reader = Reader { tree: &tree, lines };
    let mut parsed = Parsed { file: File::default(), args: Vec::new() };
    for statement in tree.children(tree.root()) {
        reader.statement(statement, &mut parsed);
    }
    f(&tree, parsed)
}

struct Reader<'t> {
    tree: &'t Tree,
    /// The offset each line starts at.
    lines: Vec<usize>,
}

impl Reader<'_> {
    fn kind(&self, node: NodeId) -> Option<N> {
        self.tree.node_kind(node)
    }

    /// The node's children that are elements, not tokens.
    fn elements(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.tree.children(node).filter(|&c| self.tree.token_kind(c).is_none())
    }

    fn text(&self, node: NodeId) -> String {
        String::from_utf8_lossy(self.tree.text(node)).into_owned()
    }

    /// The node's first token of `kind` among its children.
    fn token(&self, node: NodeId, kind: T) -> Option<NodeId> {
        self.tree.children(node).find(|&c| self.tree.token_kind(c) == Some(kind))
    }

    fn line(&self, node: NodeId) -> u32 {
        self.lines.partition_point(|&start| start <= self.tree.range(node).start) as u32
    }

    fn has_error(&self, node: NodeId) -> bool {
        self.tree.descendants(node).any(|n| self.kind(n) == Some(N::ERROR_ELEMENT))
    }

    /// One top-level statement into `parsed`; one the reader does not read makes its file `unread`.
    fn statement(&self, node: NodeId, parsed: &mut Parsed) {
        if self.tree.token_kind(node).is_some() {
            return;
        }
        let file = &mut parsed.file;
        if self.has_error(node) {
            file.unread = true;
            return;
        }
        match self.kind(node) {
            Some(N::STATEMENT_LIST) => {
                for s in self.elements(node).collect::<Vec<_>>() {
                    self.statement(s, parsed);
                }
            }
            Some(N::LOAD_STATEMENT) => self.load(node, file),
            Some(N::ASSIGNMENT_STATEMENT) => {
                // `NAME = value` binds; a tuple, a subscript or an attribute on the left (`X["k"] = v`) binds no name the
                // reader keeps.
                let parts: Vec<NodeId> = self.elements(node).collect();
                if let [target, value] = parts[..] && let Some(name) = self.token(target, T::IDENTIFIER).filter(|_| self.kind(target) == Some(N::TARGET_EXPRESSION)) {
                    file.assigns.insert(self.text(name), self.expr(value));
                }
            }
            Some(N::AUG_ASSIGNMENT_STATEMENT) => {
                // `NAME += value` appends: the value is `old + new`; any other operator binds nothing the reader keeps.
                let parts: Vec<NodeId> = self.elements(node).collect();
                if let [target, value] = parts[..] {
                    let simple = self.token(node, T::PLUSEQ).is_some() && self.kind(target) == Some(N::REFERENCE_EXPRESSION) && self.elements(target).next().is_none();
                    if let Some(name) = self.token(target, T::IDENTIFIER).filter(|_| simple) {
                        let name = self.text(name);
                        let old = file.assigns.remove(&name).unwrap_or(Expr::List(Vec::new()));
                        file.assigns.insert(name, Expr::Bin("+", Box::new(old), Box::new(self.expr(value))));
                    }
                }
            }
            Some(N::EXPRESSION_STATEMENT) => {
                let Some(expression) = self.elements(node).next() else { return };
                match self.expr(expression) {
                    Expr::Call(c) => {
                        file.calls.push(c);
                        parsed.args.push(self.arg_ranges(expression));
                    }
                    other => {
                        // A comprehension of calls (`[http_archive(name = "x_%s" % p, …) for p in …]`): declarations
                        // whose names the reader cannot know, each a call with a `name`.
                        let mut calls = Vec::new();
                        other.walk(&mut |e| {
                            if let Expr::Call(c) = e && c.kw("name").is_some() {
                                calls.push(c.clone());
                            }
                        });
                        file.nested.extend(calls);
                    }
                }
            }
            // A `def`, `if`, `for`, `return`, `pass`, `break` or `continue`: none of Bazel's BUILD dialect.
            _ => file.unread = true,
        }
    }

    /// `load("<module>", "a", b = "c")`.
    fn load(&self, node: NodeId, file: &mut File) {
        let mut module = None;
        let mut names = Vec::new();
        for value in self.elements(node) {
            let string = self.elements(value).find(|&s| self.kind(s) == Some(N::STRING_LITERAL_EXPRESSION)).map(|s| self.string(s));
            match (self.kind(value), string) {
                (Some(N::FILENAME_LOAD_VALUE), Some(s)) => module = Some(s),
                (Some(N::STRING_LOAD_VALUE), Some(s)) => names.push((s.clone(), s)),
                (Some(N::NAMED_LOAD_VALUE), Some(s)) => {
                    if let Some(local) = self.token(value, T::IDENTIFIER) {
                        names.push((self.text(local), s));
                    }
                }
                _ => {}
            }
        }
        match module {
            Some(module) => file.loads.push(Load { module, names }),
            None => file.unread = true,
        }
    }

    fn string(&self, literal: NodeId) -> String {
        starlark_syntax::string::value(&self.text(literal))
    }

    /// The name a reference, a glob or a loop target writes, with the receiver it is written on (`native.glob`).
    fn reference(&self, node: NodeId) -> Expr {
        let name = self.token(node, T::IDENTIFIER).map(|n| self.text(n)).unwrap_or_default();
        match self.elements(node).next() {
            Some(receiver) => Expr::Attr(Box::new(self.expr(receiver)), name),
            None => Expr::Name(name),
        }
    }

    fn expr(&self, node: NodeId) -> Expr {
        let all = |r: &Self| -> Vec<Expr> { r.elements(node).flat_map(|e| r.held(e)).collect() };
        match self.kind(node) {
            Some(N::STRING_LITERAL_EXPRESSION) => Expr::Str(self.string(node)),
            Some(N::TRUE_LITERAL_EXPRESSION | N::FALSE_LITERAL_EXPRESSION | N::NONE_LITERAL_EXPRESSION) => Expr::Name(self.text(node)),
            Some(N::REFERENCE_EXPRESSION | N::GLOB_EXPRESSION | N::TARGET_EXPRESSION) => self.reference(node),
            Some(N::CALL_EXPRESSION) => self.call(node),
            Some(N::LIST_LITERAL_EXPRESSION | N::TUPLE_EXPRESSION) => Expr::List(self.elements(node).map(|e| self.expr(e)).collect()),
            // `(x)` is the value, `(x,)` a tuple, `()` an empty one.
            Some(N::PARENTHESIZED_EXPRESSION) => self.elements(node).next().map_or(Expr::List(Vec::new()), |e| self.expr(e)),
            Some(N::STAR_EXPRESSION | N::DOUBLE_STAR_EXPRESSION) => self.elements(node).next().map_or(Expr::Other(Vec::new()), |e| self.expr(e)),
            Some(N::DICT_LITERAL_EXPRESSION) => {
                let mut pairs = Vec::new();
                let mut other = Vec::new();
                for e in self.elements(node) {
                    match self.key_value(e) {
                        Some(pair) => pairs.push(pair),
                        None => other.push(self.expr(e)),
                    }
                }
                if other.is_empty() { Expr::Dict(pairs) } else { Expr::Other(pairs.into_iter().flat_map(|(k, v)| [k, v]).chain(other).collect()) }
            }
            Some(N::BINARY_EXPRESSION) => {
                let mut operands = self.elements(node).map(|e| self.expr(e));
                let (Some(left), Some(right)) = (operands.next(), operands.next()) else { return Expr::Other(all(self)) };
                let op = self
                    .tree
                    .children(node)
                    .filter_map(|c| self.tree.token_kind(c))
                    .find(|k| !k.is_whitespace() && !k.is_comment())
                    .and_then(|k| BINARY.into_iter().find(|&b| b == k.debug_name()))
                    .unwrap_or("op");
                Expr::Bin(op, Box::new(left), Box::new(right))
            }
            Some(N::CONDITIONAL_EXPRESSION) => {
                let mut parts = self.elements(node).map(|e| self.expr(e));
                match parts.next() {
                    Some(value) => Expr::Cond(Box::new(value), Box::new(Expr::Other(parts.collect()))),
                    None => Expr::Other(Vec::new()),
                }
            }
            // A lambda's body; its parameters bind names.
            Some(N::LAMBDA_EXPRESSION) => Expr::Other(self.elements(node).filter(|&e| self.kind(e) != Some(N::PARAMETER_LIST)).map(|e| self.expr(e)).collect()),
            // A number, `not x`, `-x`, a comprehension, a subscript or slice: the expressions it holds.
            _ => Expr::Other(all(self)),
        }
    }

    /// The expressions an element holds for `Other`: a comprehension's loop targets none, a `k: v` both, a slice its
    /// bounds.
    fn held(&self, node: NodeId) -> Vec<Expr> {
        match self.kind(node) {
            Some(N::TARGET_EXPRESSION) => Vec::new(),
            Some(N::TUPLE_EXPRESSION) if self.elements(node).all(|e| self.kind(e) == Some(N::TARGET_EXPRESSION)) => Vec::new(),
            Some(N::KEY_VALUE_EXPRESSION | N::SLICE_ITEM) => self.elements(node).map(|e| self.expr(e)).collect(),
            _ => vec![self.expr(node)],
        }
    }

    fn key_value(&self, node: NodeId) -> Option<(Expr, Expr)> {
        if self.kind(node) != Some(N::KEY_VALUE_EXPRESSION) {
            return None;
        }
        let mut parts = self.elements(node).map(|e| self.expr(e));
        Some((parts.next()?, parts.next()?))
    }

    /// A call: a plain or dotted callee is a `Call` (`native.glob`), one on another receiver a method (`"…".format`),
    /// any other callee's call the expressions it holds.
    fn call(&self, node: NodeId) -> Expr {
        let line = self.line(node);
        let mut parts = self.elements(node);
        let Some(callee) = parts.next() else { return Expr::Other(Vec::new()) };
        let args = parts.find(|&a| self.kind(a) == Some(N::ARGUMENT_LIST)).map(|a| self.args(a)).unwrap_or_default();
        match self.expr(callee) {
            Expr::Name(func) => Expr::Call(Call { func, args, line }),
            Expr::Attr(receiver, name) => match *receiver {
                Expr::Name(base) => Expr::Call(Call { func: format!("{base}.{name}"), args, line }),
                receiver => Expr::Method(Box::new(receiver), name, args),
            },
            other => Expr::Other(std::iter::once(other).chain(args.into_iter().map(|a| a.value)).collect()),
        }
    }

    /// The byte ranges of the arguments of the call at `node`, as `call` reads them.
    fn arg_ranges(&self, node: NodeId) -> Vec<Range<usize>> {
        let list = self.elements(node).skip(1).find(|&a| self.kind(a) == Some(N::ARGUMENT_LIST));
        list.map(|l| self.elements(l).map(|a| self.tree.range(a)).collect()).unwrap_or_default()
    }

    fn args(&self, list: NodeId) -> Vec<Arg> {
        self.elements(list)
            .map(|a| {
                let value = self.elements(a).last().map_or(Expr::Other(Vec::new()), |v| self.expr(v));
                let name = match self.kind(a) {
                    Some(N::NAMED_ARGUMENT_EXPRESSION) => self.token(a, T::IDENTIFIER).map(|n| self.text(n)),
                    Some(N::STAR_ARGUMENT_EXPRESSION) => Some(if self.token(a, T::EXP).is_some() { "**" } else { "*" }.to_string()),
                    _ => None,
                };
                Arg { name, value }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A generated BUILD section: a load, a target with a glob and lists, a `select`, an assignment appended to.
    #[test]
    fn reads_loads_targets_and_assignments() {
        let f = parse(
            br#"load("@rules_jvm//:jvm.bzl", "jvm_library", lib = "jvm_import")
DEPS = ["//a"]
DEPS += [":b"]
jvm_library(
    name = "x",
    srcs = glob(["src/**/*.kt"], exclude = ["src/gen/**"], allow_empty = True),
    deps = DEPS + select({"@platforms//os:linux": ["//linux"], "//conditions:default": []}),
    visibility = ["//visibility:public"],
)
"#,
        );
        assert!(!f.partial());
        assert_eq!(f.loads.len(), 1);
        assert_eq!(f.loads[0].names, vec![("jvm_library".to_string(), "jvm_library".to_string()), ("lib".to_string(), "jvm_import".to_string())]);
        assert!(matches!(f.assigns.get("DEPS"), Some(Expr::Bin("+", _, _))));
        let c = &f.calls[0];
        assert_eq!((c.func.as_str(), c.kw_str("name"), c.line), ("jvm_library", Some("x"), 4));
        assert!(matches!(c.kw("srcs"), Some(Expr::Call(g)) if g.func == "glob" && g.kw("exclude").is_some()));
    }

    /// A comprehension of calls and a `def` make the file partial; the calls inside still show their arguments.
    #[test]
    fn a_comprehension_or_a_def_is_partial() {
        let f = parse(b"[http_archive(name = \"x_%s\" % p, url = u) for p, u in X.items()]\nok(name = \"y\")\n");
        assert!(f.partial() && !f.unread);
        assert_eq!(f.nested.len(), 1);
        assert_eq!(f.calls.len(), 1);
        let f = parse(b"def f():\n    pass\nok(name = \"y\")\n");
        assert!(f.unread);
        assert_eq!(f.calls.iter().filter(|c| c.func == "ok").count(), 1);
    }

    /// A method on a string, a dotted callee, a conditional and a tuple.
    #[test]
    fn methods_dotted_calls_and_conditionals() {
        let f = parse(b"x(name = \"n\", a = \"//p:{}\".format(v), b = native.glob([\"*\"]), c = \"a\" if k else \"b\", d = (\"t\", \"u\"))\n");
        let c = &f.calls[0];
        assert!(matches!(c.kw("a"), Some(Expr::Method(_, m, _)) if m == "format"));
        assert!(matches!(c.kw("b"), Some(Expr::Call(g)) if g.func == "native.glob"));
        assert!(matches!(c.kw("c"), Some(Expr::Cond(_, _))));
        assert!(matches!(c.kw("d"), Some(Expr::List(v)) if v.len() == 2));
    }
}
