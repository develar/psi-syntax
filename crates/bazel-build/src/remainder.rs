//! A BUILD file's text without the values of the arguments that only write labels, in a canonical form: what is left
//! when its dependency lists are taken out, so that two revisions of a file whose remainders are equal differ in those
//! lists alone (Branch Deck folds such a file in a review and says which labels moved, from `crate::read`).
//!
//! An argument is cut, its name, `=`, value and one comma beside it, when it is a keyword argument of a top-level call
//! that declares a target (`written::declares_no_target`), its name is no dependency's exception
//! (`written::not_a_dependency`: `name`, `visibility`, `tags`, …) and its value writes labels only: a string that is
//! label-shaped (`label::shaped`) or names a target the file declares, a list or tuple of such values, a `+` of two,
//! a `select({…})` whose keys and values are such values, or a variable of the file whose value is one. `[]` is one,
//! so a file that gains its first dependency of an attribute stays equal; a glob, a call, a number, `True`, a loaded
//! name or a computed label is none, so `srcs = glob([…])`, `srcs = ["A.kt"]`, `testonly = True` and
//! `jvm_flags = OPENS` stay in the remainder. A variable's assignment stays too: a changed `DEPS = […]` is a change,
//! and so do the arguments of a call inside a comprehension (`[jvm_library(name = n, deps = […]) for n in …]`).
//!
//! The canonical form is the file's tokens but whitespace, each followed by `\n`, so a change of indentation or
//! line breaks is no change, a change of a comment is one, and a comma before the `)` of a call's arguments is dropped
//! (`f(a = 1,)` is `f(a = 1)`; a tuple's comma stays).
use std::collections::HashSet;
use std::ops::Range;

use starlark_syntax::{NodeId, NodeKind as N, TokenKind as T};

use crate::label;
use crate::statements::{self, Expr, File};
use crate::written::{declares_no_target, not_a_dependency};

/// The remainder of the BUILD file `src`: its tokens without its label-only arguments, each followed by `\n`.
pub fn remainder(src: &[u8]) -> Vec<u8> {
    statements::read(src, |tree, parsed| {
        let file = &parsed.file;
        let names: HashSet<&str> =
            file.calls.iter().filter(|c| !declares_no_target(&c.func)).filter_map(|c| c.kw_str("name")).collect();
        let mut cut: Vec<Range<usize>> = Vec::new();
        for (c, ranges) in file.calls.iter().zip(&parsed.args) {
            if declares_no_target(&c.func) {
                continue;
            }
            for (a, range) in c.args.iter().zip(ranges) {
                let dependency = a.name.as_deref().is_some_and(|n| !not_a_dependency(n) && !n.starts_with('*'));
                if dependency && labels_only(&a.value, file, &names, &mut HashSet::new()) {
                    cut.push(range.clone());
                }
            }
        }
        // The tokens in order, one `None` where an argument was cut.
        let mut pieces: Vec<Option<NodeId>> = Vec::new();
        let mut next_cut = 0;
        let mut marked = usize::MAX;
        for id in tree.tokens() {
            let range = tree.range(id);
            if range.is_empty() || tree.token_kind(id).is_none_or(T::is_whitespace) {
                continue;
            }
            while cut.get(next_cut).is_some_and(|c| c.end <= range.start) {
                next_cut += 1;
            }
            if cut.get(next_cut).is_some_and(|c| c.start <= range.start) {
                if marked != next_cut {
                    pieces.push(None);
                    marked = next_cut;
                }
                continue;
            }
            pieces.push(Some(id));
        }
        let is = |p: Option<NodeId>, kind: T| p.is_some_and(|id| tree.token_kind(id) == Some(kind));
        // The comma after a cut argument, else the one before it.
        let mut kept: Vec<bool> = pieces.iter().map(Option::is_some).collect();
        for i in (0..pieces.len()).filter(|&i| pieces[i].is_none()) {
            let after = (i + 1..pieces.len()).find(|&j| pieces[j].is_some());
            let before = (0..i).rev().find(|&j| kept[j]);
            match (after, before) {
                (Some(j), _) if is(pieces[j], T::COMMA) => kept[j] = false,
                (_, Some(j)) if is(pieces[j], T::COMMA) => kept[j] = false,
                _ => {}
            }
        }
        let tokens: Vec<NodeId> = pieces.iter().zip(&kept).filter(|&(_, &k)| k).filter_map(|(p, _)| *p).collect();
        let mut out = Vec::with_capacity(src.len());
        for (i, &id) in tokens.iter().enumerate() {
            let trailing = tree.token_kind(id) == Some(T::COMMA)
                && tree.parent(id).and_then(|p| tree.node_kind(p)) == Some(N::ARGUMENT_LIST)
                && tokens.get(i + 1).is_some_and(|&n| tree.token_kind(n) == Some(T::RPAR));
            if !trailing {
                out.extend_from_slice(tree.text(id));
                out.push(b'\n');
            }
        }
        out
    })
}

/// Does the value write labels only (the module's rule)? `names` are the targets the file declares.
fn labels_only(e: &Expr, file: &File, names: &HashSet<&str>, seen: &mut HashSet<String>) -> bool {
    match e {
        Expr::Str(s) => label::shaped(s) || names.contains(s.as_str()),
        Expr::List(v) => v.iter().all(|e| labels_only(e, file, names, seen)),
        Expr::Bin("+", a, b) => labels_only(a, file, names, seen) && labels_only(b, file, names, seen),
        Expr::Call(c) if c.func == "select" && c.args.len() == 1 && c.args[0].name.is_none() => {
            match &c.args[0].value {
                Expr::Dict(d) => {
                    d.iter().all(|(k, v)| labels_only(k, file, names, seen) && labels_only(v, file, names, seen))
                }
                _ => false,
            }
        }
        Expr::Name(n) => match file.assigns.get(n) {
            Some(v) if seen.insert(n.clone()) => labels_only(v, file, names, seen),
            _ => false,
        },
        _ => false,
    }
}
