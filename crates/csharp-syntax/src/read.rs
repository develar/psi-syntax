//! `csharp_syntax::read`: reads of a C# tree that every consumer needs and that hold no consumer's facts, so that none
//! of them walks Roslyn's tree by its own rules: what is trivia, a node's `Span`, its first and last token, the token a
//! name is named by, the dotted name a name spells and the type names a type spells. What a consumer makes of them
//! (symbols, call sites, imports, labels) stays in the consumer. The node properties themselves are
//! `crate::syntax_getters`.
//!
//! None recurses: Roslyn nests `A.B.C…`, `a.f().f()…` and `a + b + c…` left-deep, a level a part, and a read over
//! such a chain keeps its own stack, writes each part once and descends the edge it needs alone.

use std::ops::Range;

use psi_builder::Tree as PsiTree;

use crate::language::CSharp;
use crate::syntax_getters::{
    alias_qualified_name, generic_name, identifier_name, is_name_syntax, name, qualified_name,
};
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// A structured trivia's root: a preprocessor directive (`#if DEBUG`, whose condition holds an `IdentifierName`), a
/// documentation comment parsed into XML nodes (`DocumentationMode::Parse`, the default), or tokens the parser skipped.
/// Its subtree is trivia of the token it is attached to (`tree.is_trailing`), no part of the code around it. Over
/// `psi_builder`'s tree, which `Tree` derefs to, so it serves as the predicate of its walks (`own_range`,
/// `descendants_pruned`).
pub fn is_structured_trivia(tree: &PsiTree<CSharp>, id: NodeId) -> bool {
    tree.node_kind(id).is_some_and(|n| crate::syntax::node_type(n).is_structured_trivia)
}

/// Trivia: a leaf of whitespace, a line end, a comment, a documentation comment's exterior (`///`), disabled text
/// (`#if` branches Roslyn does not take), a preprocessing message, a conflict marker; or a structured trivia's root
/// (`is_structured_trivia`).
pub fn is_trivia(tree: &PsiTree<CSharp>, id: NodeId) -> bool {
    tree.token_kind(id).is_some_and(crate::dump::is_trivia) || is_structured_trivia(tree, id)
}

/// A token of the code: a leaf that is no trivia and spells text, so neither a missing token (an empty leaf Roslyn's
/// recovery inserts) nor an empty end token (`EndOfFileToken`, `EndOfDirectiveToken`,
/// `EndOfDocumentationCommentToken`).
pub fn is_code_token(tree: &PsiTree<CSharp>, id: NodeId) -> bool {
    tree.token_kind(id).is_some_and(|t| !crate::dump::is_trivia(t)) && !tree.range(id).is_empty()
}

/// The node's first token, its leaf: `GetFirstTerminal()` of Roslyn's green node (GreenNode.cs), which
/// `SyntaxNode.Span` starts at, so a missing token is one; the node itself for a token, None for trivia and for a
/// node without a token. A descent along the left edge: a level per node there.
pub fn first_token(tree: &PsiTree<CSharp>, id: NodeId) -> Option<NodeId> {
    // the first token in preorder that is no trivia, outside structured trivia
    let end = tree.subtree_end(id).0;
    let mut next = id.0;
    while next < end {
        let n = NodeId(next);
        if n != id && is_structured_trivia(tree, n) {
            next = tree.subtree_end(n).0;
            continue;
        }
        if tree.is_leaf(n) {
            if !tree.token_kind(n).is_some_and(crate::dump::is_trivia) {
                return Some(n);
            }
            if n == id {
                return None;
            }
        }
        next += 1;
    }
    None
}

/// The node's last token: `GetLastTerminal()` (GreenNode.cs), where `SyntaxNode.Span` ends; a descent along the right
/// edge alone, so the end of a call at the end of a left-deep chain (`a.f().f()…`) is not a walk to the chain's head.
pub fn last_token(tree: &PsiTree<CSharp>, id: NodeId) -> Option<NodeId> {
    if tree.is_leaf(id) {
        return (!tree.token_kind(id).is_some_and(crate::dump::is_trivia)).then_some(id);
    }
    let mut n = id;
    loop {
        let mut last = None;
        let mut child = tree.last_child(n);
        while let Some(c) = child {
            if !is_trivia(tree, c) && (tree.is_leaf(c) || has_token(tree, c)) {
                last = Some(c);
                break;
            }
            child = tree.previous_sibling(c);
        }
        let c = last?;
        if tree.is_leaf(c) {
            return Some(c);
        }
        n = c;
    }
}

/// Whether a node holds a token outside structured trivia: every node but one whose every token is trivia.
fn has_token(tree: &PsiTree<CSharp>, id: NodeId) -> bool {
    first_token(tree, id).is_some()
}

/// `SyntaxNode.Span` (SyntaxNode.cs:92): the node's full span (its range) without the leading trivia of its first
/// token (`first_token`) and the trailing trivia of its last (`last_token`), so from its first attribute, modifier or
/// keyword to its closing token; a missing token at an edge counts (its position), as in Roslyn. A token's is its own
/// range. The text is `&tree.source()[span]` (`ToString()`); a declaration without the missing tokens at its edges is
/// `tree.own_range(id, |c| is_trivia(tree, c), |_| false)`.
pub fn span(tree: &PsiTree<CSharp>, id: NodeId) -> Range<usize> {
    let range = tree.range(id);
    if range.is_empty() {
        return range;
    }
    match (first_token(tree, id), last_token(tree, id)) {
        (Some(first), Some(last)) => tree.range(first).start..tree.range(last).end,
        _ => range,
    }
}

/// The end of `span`, from the right edge alone (`last_token`).
pub fn span_end(tree: &PsiTree<CSharp>, id: NodeId) -> usize {
    let range = tree.range(id);
    if range.is_empty() {
        return range.end;
    }
    last_token(tree, id).map_or(range.end, |last| tree.range(last).end)
}

/// The `Span` of every node of a tree, made in one pass from the leaves up (`span` of each node alone costs its edge's
/// depth, which a left-deep chain makes as long as the chain).
pub(crate) struct Spans {
    /// The first and last token of each node (`u32::MAX`: none).
    edges: Vec<[u32; 2]>,
}

impl Spans {
    pub(crate) fn new(tree: &PsiTree<CSharp>) -> Spans {
        const NONE: u32 = u32::MAX;
        let mut edges = vec![[NONE, NONE]; tree.len()];
        for i in (0..tree.len()).rev() {
            let id = NodeId(i as u32);
            if tree.is_leaf(id) {
                if !tree.token_kind(id).is_some_and(crate::dump::is_trivia) {
                    edges[i] = [id.0, id.0];
                }
                continue;
            }
            let mut edge = [NONE, NONE];
            for c in tree.children(id) {
                if is_structured_trivia(tree, c) {
                    continue;
                }
                let [first, last] = edges[c.0 as usize];
                if first != NONE {
                    if edge[0] == NONE {
                        edge[0] = first;
                    }
                    edge[1] = last;
                }
            }
            edges[i] = edge;
        }
        Spans { edges }
    }

    /// `span(tree, id)`.
    pub(crate) fn span(&self, tree: &PsiTree<CSharp>, id: NodeId) -> Range<usize> {
        let range = tree.range(id);
        match self.edges[id.0 as usize] {
            [first, last] if first != u32::MAX && !range.is_empty() => {
                tree.range(NodeId(first)).start..tree.range(NodeId(last)).end
            }
            _ => range,
        }
    }
}

/// The identifier token a name is named by: of its unqualified name (`name::get_unqualified_name`: `C` of `A.B.C`,
/// `List` of `List<Foo>`, `A` of `global::A`); None for a node that is no name.
pub fn name_leaf(tree: &Tree, name: NodeId) -> Option<NodeId> {
    let simple = name::get_unqualified_name(tree, name)?;
    match tree.node_kind(simple)? {
        NodeKind::IdentifierName => identifier_name::identifier(tree, simple),
        NodeKind::GenericName => generic_name::identifier(tree, simple),
        _ => None,
    }
}

/// The name an identifier token binds or uses: Roslyn's `ValueText` (`Tree::value_text`), by which its binder looks
/// every name up (`@class` is `class`, `a\u0062c` is `abc`); None for a missing identifier, which names nothing, for an
/// empty value and for a token that is not UTF-8.
pub fn word(tree: &Tree, token: NodeId) -> Option<&str> {
    if tree.is_missing(token) {
        return None;
    }
    tree.value_text(token).filter(|value| !value.is_empty())
}

/// The name a name node spells as Roslyn's tree nests it, each identifier a `word`: a `QualifiedName` its left and
/// right joined by `.` (`A.B.C` is `(A.B).C`), an `AliasQualifiedName` its name alone (`global::A` and `E::A` are `A`:
/// the alias names an assembly's root, no namespace; `name::get_alias_qualifier_opt` has it), a `GenericName` its
/// identifier (`List<Foo>` is `List`, `Outer<int>.Inner` is `Outer.Inner`); a missing identifier is no word (`A.` is
/// `A`). None for a node that is no name and when no word is left. Unlike `ToString()` (`&tree.source()[span]`), no
/// trivia, no `@`, no escapes, no type arguments.
pub fn dotted_name(tree: &Tree, name: NodeId) -> Option<String> {
    if !tree.node_kind(name).is_some_and(is_name_syntax) {
        return None;
    }
    let mut out = String::new();
    let mut pending = vec![name];
    while let Some(n) = pending.pop() {
        match tree.node_kind(n) {
            Some(NodeKind::IdentifierName | NodeKind::GenericName) => {
                if let Some(w) = name_leaf(tree, n).and_then(|leaf| word(tree, leaf)) {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(w);
                }
            }
            // the left side's words, then the right side's
            Some(NodeKind::QualifiedName) => {
                pending.extend(qualified_name::right(tree, n));
                pending.extend(qualified_name::left(tree, n));
            }
            Some(NodeKind::AliasQualifiedName) => pending.extend(alias_qualified_name::name(tree, n)),
            _ => {}
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The identifier tokens of a name in order (`A`, `B`, `C` of `A.B.C`), without type arguments (`List<Foo>` is `List`)
/// and without the alias of an alias qualifier wherever Roslyn nests it (`global::A`, `E::A.B` is `(E::A).B`): the
/// segments `dotted_name` joins, as tokens. Missing identifiers included.
pub fn name_segments(tree: &Tree, name: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    if !tree.node_kind(name).is_some_and(is_name_syntax) {
        return out;
    }
    let mut pending = vec![name];
    while let Some(n) = pending.pop() {
        match tree.node_kind(n) {
            Some(NodeKind::IdentifierName | NodeKind::GenericName) => out.extend(name_leaf(tree, n)),
            Some(NodeKind::QualifiedName) => {
                pending.extend(qualified_name::right(tree, n));
                pending.extend(qualified_name::left(tree, n));
            }
            Some(NodeKind::AliasQualifiedName) => pending.extend(alias_qualified_name::name(tree, n)),
            _ => {}
        }
    }
    out
}

/// A subtree no type name of the type around it is read from (`type_names`): its type arguments (`List<Foo>` is
/// `List`), an array's sizes, an attribute, a base type's constructor arguments (`Base(x)`), a function pointer's
/// calling convention and parameter and return types (`delegate* unmanaged[Cdecl]<IntPtr, uint>` names no `IntPtr`, as
/// `Func<IntPtr, uint>` names no `IntPtr` either), and structured trivia.
fn names_no_type(tree: &Tree, id: NodeId) -> bool {
    matches!(
        tree.node_kind(id),
        Some(
            NodeKind::TypeArgumentList
                | NodeKind::ArrayRankSpecifier
                | NodeKind::AttributeList
                | NodeKind::ArgumentList
                | NodeKind::FunctionPointerCallingConvention
                | NodeKind::FunctionPointerParameterList
        )
    ) || is_structured_trivia(tree, id)
}

/// The type names under `id`, in order, each a `dotted_name` of the name node outermost (`Ns.IBaz` whole,
/// `Outer<int>.Inner` one name `Outer.Inner`, `global::` and an extern alias left out): a tuple names its elements'
/// types (a tuple element's name, `(Foo a, Bar b)`, is none), an array, nullable, pointer, `ref` or `scoped` type its
/// element's; nothing under a type argument list, an array's sizes, an attribute, an argument list or a function
/// pointer's parameters (`names_no_type`). A predefined type (`int`) names none.
pub fn type_names(tree: &Tree, id: NodeId, out: &mut Vec<String>) {
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        match tree.node_kind(n) {
            None => {}
            Some(
                NodeKind::IdentifierName
                | NodeKind::GenericName
                | NodeKind::QualifiedName
                | NodeKind::AliasQualifiedName,
            ) => {
                out.extend(dotted_name(tree, n));
            }
            Some(_) if names_no_type(tree, n) => {}
            Some(_) => {
                let first = stack.len();
                stack.extend(tree.children(n));
                stack[first..].reverse();
            }
        }
    }
}

/// The first of `type_names`: the name a type is of (`List` of `List<Foo>`, `Foo` of `Foo[]?`).
pub fn type_head(tree: &Tree, id: NodeId) -> Option<String> {
    let mut names = Vec::new();
    type_names(tree, id, &mut names);
    names.into_iter().next()
}
