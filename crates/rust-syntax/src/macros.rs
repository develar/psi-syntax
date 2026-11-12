//! A `macro_rules!` expanded as rust-analyzer expands it: its matcher and transcriber (`ra_ap_mbe`) over token trees
//! whose every token carries the place it came from (`ra_ap_span::Span`), the expansion parsed back into a tree of
//! items (`ra_ap_syntax_bridge::token_tree_to_syntax_node`). Where a token of an expansion was written is its span:
//! a token of the call's input (`ARGUMENT` of `flags! { ARGUMENT }`) is at its byte in the file, a token of the
//! macro's template (`const` of `$vis const $name: u32`) at its byte in the definition (`Written`). Which macro a call
//! names, how deep expansions nest and what a consumer reads of the items are the consumer's (accio's
//! `extract::rust::macros`).

use ra_ap_mbe::DeclarativeMacro;
use ra_ap_span::{EditionedFileId, FileId, ROOT_ERASED_FILE_AST_ID, Span, SpanAnchor, SpanMap, SyntaxContext};
use ra_ap_syntax::{AstNode, SyntaxNode, TextRange, TextSize, ast};
use ra_ap_syntax_bridge::{DocCommentDesugarMode, SpanMapper, TopEntryPoint};

use crate::EDITION;

/// Where a token of an expansion was written: in the input of a macro call or in a macro's template, at the bytes
/// `range` of the file. A token a transcriber makes of none (the `,` it repeats between items) is at the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Written {
    pub range: TextRange,
    pub from: From,
}

/// What a token of an expansion was written in (`Written`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// The input of a call in the file (`ARGUMENT` of `flags! { ARGUMENT }`), passed on through any calls an expansion
    /// makes.
    Input,
    /// A macro's template (`const` of `$vis const $name: u32`): the definition's text, not the call's.
    Template,
}

impl From {
    fn file(self) -> FileId {
        FileId::from_raw(match self {
            From::Input => 0,
            From::Template => 1,
        })
    }
}

/// Where the tokens of a token tree were written: the file's own tree, every token at its byte (`From` names which
/// role its text has), or an expansion, every token where its span says.
#[derive(Clone, Copy)]
pub enum Origin<'e> {
    File(From),
    Expanded(&'e Expansion),
}

/// The span of each token of a tree of the file: its own range, in the role `From` gives it.
struct FileSpans(From);

impl SpanMapper for FileSpans {
    fn span_for(&self, range: TextRange) -> Span {
        file_span(self.0, range)
    }
}

fn file_span(from: From, range: TextRange) -> Span {
    Span {
        range,
        anchor: SpanAnchor { file_id: EditionedFileId::new(from.file(), EDITION), ast_id: ROOT_ERASED_FILE_AST_ID },
        ctx: SyntaxContext::root(EDITION),
    }
}

fn token_tree(tt: &ast::TokenTree, origin: Origin<'_>, call_site: Span) -> ra_ap_tt::TopSubtree {
    match origin {
        Origin::File(from) => {
            ra_ap_syntax_bridge::syntax_node_to_token_tree(tt.syntax(), FileSpans(from), call_site, DocCommentDesugarMode::Mbe)
        }
        Origin::Expanded(e) => {
            ra_ap_syntax_bridge::syntax_node_to_token_tree(tt.syntax(), &e.spans, call_site, DocCommentDesugarMode::Mbe)
        }
    }
}

/// A `macro_rules!` read from its definition: its rules, which rustc's own reading accepts or refuses alike.
pub struct Rules(DeclarativeMacro);

impl Rules {
    /// The rules of `def`, whose tokens were written where `origin` says; None for a definition without a body or
    /// one whose rules do not read (rustc refuses it as well).
    pub fn parse(def: &ast::MacroRules, origin: Origin<'_>) -> Option<Rules> {
        let tt = def.token_tree()?;
        let at = file_span(From::Template, def.syntax().text_range());
        let rules = DeclarativeMacro::parse_macro_rules(&token_tree(&tt, origin, at), |_| EDITION);
        (rules.err().is_none() && rules.num_rules() > 0).then_some(Rules(rules))
    }
}

/// The items an item macro's call writes: the tree of its expansion and where each of its tokens was written.
pub struct Expansion {
    root: SyntaxNode,
    spans: SpanMap,
    tokens: usize,
}

/// Why a call has no expansion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// No rule matches the input, or the expansion is no list of items: rustc refuses the call.
    NoMatch,
    /// The expansion has more tokens than the budget left.
    TooLarge,
    /// The expansion nests deeper than `EXPANSION_LEVELS`.
    TooDeep,
}

/// The expansion of a call `call` of `rules`, its input written where `origin` says; at most `budget` tokens.
pub fn expand(rules: &Rules, call: &ast::MacroCall, origin: Origin<'_>, budget: usize) -> Result<Expansion, Refused> {
    let (input, call_site) = input(call, origin).ok_or(Refused::NoMatch)?;
    let db = salsa::DatabaseImpl::default();
    let result = rules.0.expand(&db, &input, |_| (), ra_ap_mbe::MacroCallStyle::FnLike, call_site);
    if result.err.is_some() {
        return Err(Refused::NoMatch);
    }
    let (out, _) = result.value;
    let tokens = out.as_token_trees().len();
    if tokens > budget {
        return Err(Refused::TooLarge);
    }
    let (root, spans) = items(&out).ok_or(Refused::TooDeep)?;
    if root.children().next().is_none() && root.descendants_with_tokens().any(|e| e.kind() == ra_ap_syntax::SyntaxKind::ERROR) {
        return Err(Refused::NoMatch);
    }
    Ok(Expansion { root, spans, tokens })
}

/// The input of a call, its tokens written where `origin` says, and the span of the call itself: what is between its
/// delimiters, as a function-like call's input is (rust-analyzer's `macro_arg`).
fn input(call: &ast::MacroCall, origin: Origin<'_>) -> Option<(ra_ap_tt::TopSubtree, Span)> {
    let tt = call.token_tree()?;
    let call_site = match origin {
        Origin::File(from) => file_span(from, call.syntax().text_range()),
        Origin::Expanded(e) => e.spans.span_at(call.syntax().text_range().start()),
    };
    let mut input = token_tree(&tt, origin, call_site);
    input.set_top_subtree_delimiter_kind(ra_ap_tt::DelimiterKind::Invisible);
    Some((input, call_site))
}

/// The deepest expansion read, in `nesting::levels`: its `Parse` drops the tree on rust-analyzer's thread of 2 MiB
/// (`ParseNodeDropper`), whose recursion takes a frame of a few dozen bytes per level of the tree, and `levels` never
/// undercounts the tree's depth. A file this deep is generated; real macros nest a few levels.
pub const EXPANSION_LEVELS: usize = 4096;

/// The tree of the items a token tree writes and the span of each of its tokens, parsed where the stack holds the
/// parse (`nesting::run`, as a file is); None past `EXPANSION_LEVELS`.
fn items(tt: &ra_ap_tt::TopSubtree) -> Option<(SyntaxNode, SpanMap)> {
    parsed_as(tt, TopEntryPoint::MacroItems)
}

/// The tree a token tree parses to at `entry` (items, statements) and the span of each of its tokens, parsed where the
/// stack holds the parse; None past `EXPANSION_LEVELS`.
fn parsed_as(tt: &ra_ap_tt::TopSubtree, entry: TopEntryPoint) -> Option<(SyntaxNode, SpanMap)> {
    let text = tt.to_string();
    if crate::nesting::levels(&text) > EXPANSION_LEVELS {
        return None;
    }
    let (parse, spans) = crate::nesting::run(&text, || ra_ap_syntax_bridge::token_tree_to_syntax_node(tt, entry, &mut |_| EDITION))?;
    Some((parse.syntax_node(), spans))
}

/// The items the input of a call `call` writes as they are, its tokens written where `origin` says: the reading of a
/// call whose macro is not at hand (`cfg_net! { pub struct Socket { … } }`, tokio's gates, defined in another file)
/// when its input is a list of items without an error. None for an input that is no such list (`lazy_static! {
/// static ref X: T = …; }`), whose meaning only its macro knows.
pub fn verbatim(call: &ast::MacroCall, origin: Origin<'_>) -> Option<Expansion> {
    verbatim_as(call, origin, TopEntryPoint::MacroItems)
}

/// The statements and expressions the input of a call `call` writes as they are, its tokens written where `origin` says:
/// the reading of a call in an expression's position whose macro is not at hand (`tri!(s.serialize_struct("D", 2))`,
/// `assert_eq!(a.f(), 1)`, `format!("{}", x.name())`), when its input is a list of statements without an error. None
/// for an input that is none (a type, a half of an expression), whose calls only its macro knows; a pattern reads,
/// its `_` an expression to the parser (`UNDERSCORE_EXPR`), which the consumer tells apart.
pub fn verbatim_statements(call: &ast::MacroCall, origin: Origin<'_>) -> Option<Expansion> {
    verbatim_as(call, origin, TopEntryPoint::MacroStmts)
}

fn verbatim_as(call: &ast::MacroCall, origin: Origin<'_>, entry: TopEntryPoint) -> Option<Expansion> {
    let (input, _) = input(call, origin)?;
    let input = if matches!(entry, TopEntryPoint::MacroStmts) { statements(input) } else { input };
    let tokens = input.as_token_trees().len();
    let (root, spans) = parsed_as(&input, entry)?;
    let clean = root.children().next().is_some() && !root.descendants_with_tokens().any(|e| e.kind() == ra_ap_syntax::SyntaxKind::ERROR);
    clean.then_some(Expansion { root, spans, tokens })
}

/// A call's input read as statements: the commas between its top-level arguments (`assert_eq!(a.f(), 1)`, `vec![x, y]`)
/// become the `;` between statements, each at its comma's span; a comma inside brackets stays.
fn statements(mut tt: ra_ap_tt::TopSubtree) -> ra_ap_tt::TopSubtree {
    use ra_ap_tt::{Leaf, Punct, TokenTree};
    let mut commas: Vec<(usize, Punct)> = Vec::new();
    let mut nested: Vec<usize> = Vec::new();
    for (i, t) in tt.token_trees().iter_flat_tokens().enumerate() {
        while nested.last() == Some(&0) {
            nested.pop();
        }
        if let Some(n) = nested.last_mut() {
            *n -= 1;
        }
        match t {
            TokenTree::Subtree(s) => nested.push(s.usize_len()),
            TokenTree::Leaf(Leaf::Punct(p)) if p.char == ',' && nested.is_empty() => commas.push((i, p)),
            _ => {}
        }
    }
    // The serialized list holds the top subtree at 0, its children after it.
    for (i, p) in commas {
        tt.set_token(i + 1, Leaf::Punct(Punct { char: ';', spacing: p.spacing, span: p.span }));
    }
    tt
}

impl Expansion {
    /// The items the call writes, at offsets of the expansion's own text.
    pub fn root(&self) -> &SyntaxNode {
        &self.root
    }

    /// The tokens of the expansion, what a consumer's budget counts.
    pub fn tokens(&self) -> usize {
        self.tokens
    }

    /// Where the token at `offset` of the expansion was written (`Written`).
    pub fn written(&self, offset: TextSize) -> Written {
        let span = self.spans.span_at(offset);
        let from = if span.anchor.file_id.file_id() == From::Template.file() { From::Template } else { From::Input };
        Written { range: span.range, from }
    }
}

#[cfg(test)]
mod tests {
    use ra_ap_syntax::ast::HasName;

    use super::*;

    fn first<N: AstNode>(root: &SyntaxNode) -> N {
        root.descendants().find_map(N::cast).unwrap()
    }

    /// A recursive macro of declarations: each name of the call's input declares a constant, written at its byte of
    /// the call, its `const` at the template's.
    #[test]
    fn an_expansion_says_where_each_token_was_written() {
        let src = "macro_rules! flags {\n    ($($name:ident),*) => { flags!(@bits 0u32; $($name),*); };\n    (@bits $n:expr; $name:ident $(, $rest:ident)*) => { pub const $name: u64 = 1 << $n; flags!(@bits $n + 1; $($rest),*); };\n    (@bits $n:expr;) => {};\n}\nflags! { A, B }\n";
        let root = crate::parse(src.as_bytes());
        let rules = Rules::parse(&first::<ast::MacroRules>(&root), Origin::File(From::Template)).unwrap();
        let call = root.children().filter_map(ast::MacroCall::cast).next().unwrap();
        let outer = expand(&rules, &call, Origin::File(From::Input), 10_000).unwrap();
        // the first rule writes one call of the second
        let inner = first::<ast::MacroCall>(outer.root());
        let one = expand(&rules, &inner, Origin::Expanded(&outer), 10_000).unwrap();
        let c = first::<ast::Const>(one.root());
        let name = c.name().unwrap();
        let w = one.written(name.syntax().text_range().start());
        assert_eq!((w.from, &src[w.range]), (From::Input, "A"));
        let kw = c.const_token().unwrap();
        let w = one.written(kw.text_range().start());
        assert_eq!((w.from, &src[w.range]), (From::Template, "const"));
        assert_eq!(expand(&rules, &inner, Origin::Expanded(&outer), 3).err(), Some(Refused::TooLarge));
    }

    /// An input of items read as written, every token at its byte of the call; an input that is no list of items reads
    /// as none.
    #[test]
    fn an_input_of_items_reads_as_written() {
        let src = "cfg_net! {\n    pub struct Socket { io: u32 }\n}\nlazy_static! { static ref X: T = 1; }\n";
        let root = crate::parse(src.as_bytes());
        let calls: Vec<ast::MacroCall> = root.children().filter_map(ast::MacroCall::cast).collect();
        let items = verbatim(&calls[0], Origin::File(From::Input)).unwrap();
        let s = first::<ast::Struct>(items.root());
        let w = items.written(s.name().unwrap().syntax().text_range().start());
        assert_eq!((w.from, &src[w.range]), (From::Input, "Socket"));
        assert!(verbatim(&calls[1], Origin::File(From::Input)).is_none());
    }

    /// An input of statements read as written, each call at its byte of the file; a pattern input reads as none.
    #[test]
    fn an_input_of_statements_reads_as_written() {
        let src = "fn f(st: St) -> R { tri!(st.field(\"x\")); assert_eq!(a.g(), 1); matches!(x, Some(_)); Ok(()) }\n";
        let root = crate::parse(src.as_bytes());
        let calls: Vec<ast::MacroCall> = root.descendants().filter_map(ast::MacroCall::cast).collect();
        let stmts = verbatim_statements(&calls[0], Origin::File(From::Input)).unwrap();
        let call = first::<ast::MethodCallExpr>(stmts.root());
        let w = stmts.written(call.name_ref().unwrap().syntax().text_range().start());
        assert_eq!((w.from, &src[w.range]), (From::Input, "field"));
        assert!(verbatim_statements(&calls[1], Origin::File(From::Input)).is_some(), "arguments read as statements");
        // `_` is an expression to the parser: a pattern input reads, and the consumer tells it by its `UNDERSCORE_EXPR`.
        assert!(verbatim_statements(&calls[2], Origin::File(From::Input)).is_some());
    }
}
