//! The directives of a tree under other preprocessor symbols (not in Roslyn, which lexes a file once per
//! `CSharpParseOptions`): `activity` replays the directives of a tree, in document order, as Roslyn's lexer reads them
//! (`Lexer.LexDirectiveAndExcludedTrivia`, `LexExcludedDirectivesAndTrivia`, the `DirectiveParser.Parse*Directive`
//! methods, `DirectiveStack`), with the same stack of `lexer::directives` and each `#if` and `#elif` condition
//! evaluated from its nodes (`EvaluateBool`), and answers each directive's `IsActive`, `BranchTaken` and
//! `ConditionValue` under the symbols: what a parse with them would hold, without the parse.
//!
//! The answer is exact up to the first branch the symbols take that the tree's do not. Inside it, text the tree's
//! lexer read as disabled is code, and a line of a string or a comment that spans lines is no directive there, so the
//! directives after it can differ (`tests/preprocessor.rs` counts them on the corpora). An answer that takes no branch
//! the tree does not take is exact.

use crate::lexer::{DefineState, DirectiveStack, DirectiveStore, PreprocessorSymbols, bool_try_parse};
use crate::syntax_kind::{NodeKind as N, SyntaxKind, TokenKind as T};
use crate::tree::{DirectiveFlags, NodeId, Tree};

/// Each directive of `tree` (`Tree::directives`, in its order) as a parse of the tree's text with `symbols` defined
/// would read it: its `IsActive`, `BranchTaken` and `ConditionValue`.
pub fn activity(tree: &Tree, symbols: &PreprocessorSymbols) -> Vec<DirectiveFlags> {
    let mut store = DirectiveStore::default();
    let mut stack = DirectiveStack::EMPTY;
    // the excluded text of a branch not taken (`LexExcludedDirectivesAndTrivia`): the `#if` levels nested in it, and
    // whether the level it ends at is active
    let mut excluded: Option<u32> = None;
    let mut out = Vec::with_capacity(tree.directives().len());
    for (id, _) in tree.directives() {
        let (is_active, end_is_active) = match excluded {
            None => (true, true),
            Some(0) => (false, true),
            Some(_) => (false, false),
        };
        let keyword = keyword_of(tree, id);
        let defined = |stack: DirectiveStack, store: &DirectiveStore, name: &[u8]| match stack.is_defined(store, name) {
            DefineState::Defined => true,
            DefineState::Undefined => false,
            DefineState::Unspecified => symbols.contains(name),
        };
        let (kind, flags, name): (SyntaxKind, DirectiveFlags, &[u8]) = match keyword {
            Some(SyntaxKind::IfKeyword) => {
                let is_true = condition_of(tree, id).is_some_and(|c| evaluate(tree, c, &|n| defined(stack, &store, n)));
                (SyntaxKind::IfDirectiveTrivia, DirectiveFlags::of(is_active, is_active && is_true, is_true), b"")
            }
            Some(SyntaxKind::ElifKeyword) if stack.has_previous_if_or_elif(&store) => {
                let is_true = condition_of(tree, id).is_some_and(|c| evaluate(tree, c, &|n| defined(stack, &store, n)));
                let taken = end_is_active && is_true && !stack.previous_branch_taken(&store);
                (SyntaxKind::ElifDirectiveTrivia, DirectiveFlags::of(end_is_active, taken, is_true), b"")
            }
            Some(SyntaxKind::ElseKeyword) if stack.has_previous_if_or_elif(&store) => {
                let taken = end_is_active && !stack.previous_branch_taken(&store);
                (SyntaxKind::ElseDirectiveTrivia, DirectiveFlags::of(end_is_active, taken, false), b"")
            }
            Some(SyntaxKind::EndIfKeyword) if stack.has_unfinished_if(&store) => {
                (SyntaxKind::EndIfDirectiveTrivia, DirectiveFlags::of(end_is_active, false, false), b"")
            }
            Some(SyntaxKind::EndRegionKeyword) if stack.has_unfinished_region(&store) => {
                (SyntaxKind::EndRegionDirectiveTrivia, DirectiveFlags::of(is_active, false, false), b"")
            }
            Some(
                SyntaxKind::ElifKeyword | SyntaxKind::ElseKeyword | SyntaxKind::EndIfKeyword | SyntaxKind::EndRegionKeyword,
            ) => (SyntaxKind::BadDirectiveTrivia, DirectiveFlags::of(is_active, false, false), b""),
            _ => {
                let kind = tree.node_kind(id).map_or(SyntaxKind::BadDirectiveTrivia, N::syntax_kind);
                let name = match kind {
                    SyntaxKind::DefineDirectiveTrivia | SyntaxKind::UndefDirectiveTrivia => name_of(tree, id),
                    _ => b"",
                };
                (kind, DirectiveFlags::of(is_active, false, false), name)
            }
        };
        let directive = store.directive_of(kind, flags.is_active(), flags.branch_taken(), name);
        stack = stack.add(&mut store, directive);
        out.push(flags);
        let branching = matches!(kind, SyntaxKind::IfDirectiveTrivia | SyntaxKind::ElifDirectiveTrivia | SyntaxKind::ElseDirectiveTrivia);
        excluded = match excluded {
            None if branching && !flags.branch_taken() => Some(0),
            None => None,
            Some(nested) if kind == SyntaxKind::EndIfDirectiveTrivia || (branching && flags.branch_taken()) => nested.checked_sub(1),
            Some(nested) if kind == SyntaxKind::IfDirectiveTrivia => Some(nested + 1),
            Some(nested) => Some(nested),
        };
    }
    out
}

/// The keyword after the `#` of the directive `id`, whatever node the tree made of it (a `BadDirectiveTrivia` of an
/// `#elif` without its `#if` still names `elif`): its first token after the hash.
fn keyword_of(tree: &Tree, id: NodeId) -> Option<SyntaxKind> {
    let token = tree
        .descendants(id)
        .filter(|&t| tree.token_kind(t).is_some_and(|k| !crate::dump::is_trivia(k)))
        .nth(1)?;
    let kind = tree.token_kind(token)?.syntax_kind();
    Some(match kind {
        SyntaxKind::IdentifierToken => crate::facts::get_preprocessor_keyword_kind(tree.value_text(token).unwrap_or("").as_bytes()),
        _ => kind,
    })
}

/// The condition of an `#if` or `#elif` directive `id`: its expression node.
fn condition_of(tree: &Tree, id: NodeId) -> Option<NodeId> {
    tree.children(id).find(|&c| tree.node_kind(c).is_some())
}

/// The name of a `#define` or `#undef` directive `id` (its identifier's `ValueText`).
fn name_of(tree: &Tree, id: NodeId) -> &[u8] {
    tree.children(id).find(|&c| tree.token_kind(c) == Some(T::IdentifierToken)).and_then(|c| tree.value_text(c)).map_or(b"", str::as_bytes)
}

/// `EvaluateBool(expr)` of the condition `expr`, as `DirectiveParser::evaluate` evaluates its green node, on a stack of
/// its own: `defined` is `IsDefined`.
fn evaluate(tree: &Tree, expr: NodeId, defined: &dyn Fn(&[u8]) -> bool) -> bool {
    enum Step {
        Evaluate(NodeId),
        Combine(N),
    }
    let operands = |id: NodeId| tree.children(id).filter(|&c| tree.node_kind(c).is_some());
    let mut steps = vec![Step::Evaluate(expr)];
    let mut values: Vec<bool> = Vec::new();
    while let Some(step) = steps.pop() {
        match step {
            Step::Evaluate(id) => match tree.node_kind(id) {
                Some(N::ParenthesizedExpression) => match operands(id).next() {
                    Some(inner) => steps.push(Step::Evaluate(inner)),
                    None => values.push(false),
                },
                Some(N::TrueLiteralExpression) => values.push(true),
                Some(N::FalseLiteralExpression) => values.push(false),
                Some(
                    kind @ (N::LogicalAndExpression
                    | N::BitwiseAndExpression
                    | N::LogicalOrExpression
                    | N::BitwiseOrExpression
                    | N::EqualsExpression
                    | N::NotEqualsExpression),
                ) => {
                    let mut both = operands(id);
                    match (both.next(), both.next()) {
                        (Some(left), Some(right)) => {
                            steps.push(Step::Combine(kind));
                            steps.push(Step::Evaluate(right));
                            steps.push(Step::Evaluate(left));
                        }
                        _ => values.push(false),
                    }
                }
                Some(N::LogicalNotExpression) => match operands(id).next() {
                    Some(operand) => {
                        steps.push(Step::Combine(N::LogicalNotExpression));
                        steps.push(Step::Evaluate(operand));
                    }
                    None => values.push(false),
                },
                Some(N::IdentifierName) => {
                    let token = tree.children(id).find(|&c| tree.token_kind(c) == Some(T::IdentifierToken));
                    let name = token.and_then(|t| tree.value_text(t)).unwrap_or("").as_bytes();
                    values.push(bool_try_parse(name).unwrap_or_else(|| defined(name)));
                }
                _ => values.push(false),
            },
            Step::Combine(N::LogicalNotExpression) => {
                let operand = values.pop().unwrap_or(false);
                values.push(!operand);
            }
            Step::Combine(kind) => {
                let right = values.pop().unwrap_or(false);
                let left = values.pop().unwrap_or(false);
                values.push(match kind {
                    N::LogicalAndExpression | N::BitwiseAndExpression => left && right,
                    N::LogicalOrExpression | N::BitwiseOrExpression => left || right,
                    N::EqualsExpression => left == right,
                    _ => left != right,
                });
            }
        }
    }
    values.pop().unwrap_or(false)
}
