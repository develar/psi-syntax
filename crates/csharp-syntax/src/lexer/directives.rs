//! `Directives.cs`: `Directive` (what the lexer keeps of a directive it lexed) and `DirectiveStack`, the immutable
//! `ConsList<Directive>` of the `#if`/`#elif`/`#else`, `#region`, `#define`/`#undef` and other directives in effect,
//! with Roslyn's queries (`IsDefined`, `PreviousBranchTaken`, `HasUnfinishedIf`, ...) and `Add`.
//!
//! The cons cells live in the lexer's `DirectiveStore` (a file's lexer and the parser's resets share them): a
//! `DirectiveStack` is the index of its head cell, cell 0 being `ConsList.Empty`. Cells are never changed, so a stack
//! the parser saved with a token stays what it was. `CompleteIf` and `CompleteRegion`, recursive in Roslyn, walk to the
//! `#if` (`#region`) and rebuild the cells above it in a loop.

use crate::green::{Green, GreenArena};
use crate::syntax::types;
use crate::syntax_kind::SyntaxKind;

/// Roslyn's `DirectiveStack`: the `#if`/`#region` state after a token, which a parser that re-lexes (a mode change,
/// a reset) hands back to `Lexer::reset`. Opaque to the parsers; its meaning and storage are the lexer's.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct DirectiveStack {
    /// The cell of the stack's head in the lexer's `DirectiveStore`; 0 is `DirectiveStack.Empty` (Roslyn's `null`
    /// stack, `default(DirectiveStack)`, answers every query as the empty one).
    pub(crate) head: u32,
    /// `_seenAnyIfDirectives`.
    pub(crate) seen_any_if_directives: bool,
}

/// `Directive`: a directive's kind, `IsActive`, `BranchTaken` and, of `#define`/`#undef`, `GetIdentifier()` (the
/// name's `ValueText`, in the store's `names`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Directive {
    kind: SyntaxKind,
    is_active: bool,
    branch_taken: bool,
    name_start: u32,
    name_len: u32,
}

/// `DefineState`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum DefineState {
    Defined,
    Undefined,
    Unspecified,
}

#[derive(Copy, Clone, Debug)]
struct Cell {
    head: Directive,
    tail: u32,
}

/// The cons cells of every `DirectiveStack` of a file, and the names of its `#define`s and `#undef`s.
pub struct DirectiveStore {
    cells: Vec<Cell>,
    names: Vec<u8>,
    /// Scratch of `CompleteIf` and `CompleteRegion`.
    scratch: Vec<Directive>,
}

impl Default for DirectiveStore {
    fn default() -> Self {
        let mut store = DirectiveStore { cells: Vec::new(), names: Vec::new(), scratch: Vec::new() };
        store.clear();
        store
    }
}

const EMPTY: u32 = 0;

impl DirectiveStore {
    /// Forgets every stack but the empty one.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.names.clear();
        self.scratch.clear();
        let none =
            Directive { kind: SyntaxKind::None, is_active: false, branch_taken: false, name_start: 0, name_len: 0 };
        self.cells.push(Cell { head: none, tail: EMPTY });
    }

    /// `new Directive(node)` of a directive trivia `node` of `arena` (whose tokens index `source`).
    pub(crate) fn directive(&mut self, arena: &GreenArena, source: &[u8], node: Green) -> Directive {
        let kind = arena.kind(node);
        let extra = arena.extra(node);
        // every directive's IsActive is its first boolean field, a branching one's BranchTaken its second
        let is_active = extra & types::IfDirectiveTriviaSyntax::IS_ACTIVE != 0;
        let branch_taken = matches!(
            kind,
            SyntaxKind::IfDirectiveTrivia | SyntaxKind::ElifDirectiveTrivia | SyntaxKind::ElseDirectiveTrivia
        ) && extra & types::IfDirectiveTriviaSyntax::BRANCH_TAKEN != 0;
        let (mut name_start, mut name_len) = (0, 0);
        if matches!(kind, SyntaxKind::DefineDirectiveTrivia | SyntaxKind::UndefDirectiveTrivia) {
            let name = arena.slot(node, types::DefineDirectiveTriviaSyntax::NAME);
            let value = arena.token_value_text(name, source);
            name_start = self.names.len() as u32;
            name_len = value.len() as u32;
            self.names.extend_from_slice(value);
        }
        Directive { kind, is_active, branch_taken, name_start, name_len }
    }

    fn cell(&self, at: u32) -> Option<&Cell> {
        if at == EMPTY { None } else { self.cells.get(at as usize) }
    }

    fn cons(&mut self, head: Directive, tail: u32) -> u32 {
        self.cells.push(Cell { head, tail });
        self.cells.len() as u32 - 1
    }

    /// `GetIdentifier()`: the name of a `#define` or `#undef`, else `None`.
    fn identifier(&self, d: &Directive) -> Option<&[u8]> {
        match d.kind {
            SyntaxKind::DefineDirectiveTrivia | SyntaxKind::UndefDirectiveTrivia => {
                self.names.get(d.name_start as usize..(d.name_start + d.name_len) as usize)
            }
            _ => None,
        }
    }

    /// `GetPreviousIf(directives)`: the cell of the closest `#if`, or empty.
    fn get_previous_if(&self, directives: u32) -> u32 {
        let mut current = directives;
        while let Some(c) = self.cell(current) {
            if c.head.kind == SyntaxKind::IfDirectiveTrivia {
                return current;
            }
            current = c.tail;
        }
        current
    }

    /// `GetPreviousIfElifElseOrRegion(directives)`.
    fn get_previous_if_elif_else_or_region(&self, directives: u32) -> u32 {
        let mut current = directives;
        while let Some(c) = self.cell(current) {
            if matches!(
                c.head.kind,
                SyntaxKind::IfDirectiveTrivia
                    | SyntaxKind::ElifDirectiveTrivia
                    | SyntaxKind::ElseDirectiveTrivia
                    | SyntaxKind::RegionDirectiveTrivia
            ) {
                return current;
            }
            current = c.tail;
        }
        current
    }

    /// `GetPreviousRegion(directives)`.
    fn get_previous_region(&self, directives: u32) -> u32 {
        let mut current = directives;
        while let Some(c) = self.cell(current) {
            if c.head.kind == SyntaxKind::RegionDirectiveTrivia {
                break;
            }
            current = c.tail;
        }
        current
    }

    /// `CompleteIf(stack, out include)`: removes unfinished if & related directives from stack and leaves active
    /// branch directives.
    fn complete_if(&mut self, stack: u32) -> u32 {
        // the cells down to the #if (Roslyn's recursion), then rebuilt from the bottom up
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        let mut current = stack;
        let (mut new_stack, mut include);
        loop {
            let Some(&c) = self.cell(current) else {
                // if we get to the top, the default rule is to include anything that follows
                include = true;
                new_stack = current;
                break;
            };
            // if we reach the #if directive, then we stop unwinding and start rebuilding the stack w/o the
            // #if/#elif/#else/#endif directives only including content from sections that are considered included
            if c.head.kind == SyntaxKind::IfDirectiveTrivia {
                include = c.head.branch_taken;
                new_stack = c.tail;
                break;
            }
            scratch.push(c.head);
            current = c.tail;
        }
        for &head in scratch.iter().rev() {
            match head.kind {
                SyntaxKind::ElifDirectiveTrivia | SyntaxKind::ElseDirectiveTrivia => include = head.branch_taken,
                _ => {
                    if include {
                        new_stack = self.cons(head, new_stack);
                    }
                }
            }
        }
        self.scratch = scratch;
        new_stack
    }

    /// `CompleteRegion(stack)`: removes region directives from stack but leaves everything else.
    fn complete_region(&mut self, stack: u32) -> u32 {
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        let mut current = stack;
        let mut new_stack = loop {
            // if we get to the top, the default rule is to include anything that follows
            let Some(&c) = self.cell(current) else { break current };
            if c.head.kind == SyntaxKind::RegionDirectiveTrivia {
                break c.tail;
            }
            scratch.push(c.head);
            current = c.tail;
        };
        for &head in scratch.iter().rev() {
            new_stack = self.cons(head, new_stack);
        }
        self.scratch = scratch;
        new_stack
    }
}

impl DirectiveStack {
    /// `DirectiveStack.Empty`.
    pub const EMPTY: DirectiveStack = DirectiveStack { head: EMPTY, seen_any_if_directives: false };

    /// `SeenAnyIfDirectives`.
    pub fn seen_any_if_directives(self) -> bool {
        self.seen_any_if_directives
    }

    /// `IsDefined(id)`.
    pub(crate) fn is_defined(self, store: &DirectiveStore, id: &[u8]) -> DefineState {
        let mut current = self.head;
        while let Some(c) = store.cell(current) {
            match c.head.kind {
                SyntaxKind::DefineDirectiveTrivia => {
                    if store.identifier(&c.head) == Some(id) {
                        return DefineState::Defined;
                    }
                }
                SyntaxKind::UndefDirectiveTrivia => {
                    if store.identifier(&c.head) == Some(id) {
                        return DefineState::Undefined;
                    }
                }
                SyntaxKind::ElifDirectiveTrivia | SyntaxKind::ElseDirectiveTrivia => {
                    // Skip directives from previous branches of the same #if.
                    loop {
                        current = store.cell(current).map_or(EMPTY, |c| c.tail);
                        match store.cell(current) {
                            None => return DefineState::Unspecified,
                            Some(c) if c.head.kind == SyntaxKind::IfDirectiveTrivia => break,
                            Some(_) => {}
                        }
                    }
                }
                _ => {}
            }
            current = store.cell(current).map_or(EMPTY, |c| c.tail);
        }
        DefineState::Unspecified
    }

    /// `PreviousBranchTaken()`: true if any previous section of the closest #if has its branch taken.
    pub(crate) fn previous_branch_taken(self, store: &DirectiveStore) -> bool {
        let mut current = self.head;
        while let Some(c) = store.cell(current) {
            if c.head.branch_taken {
                return true;
            } else if c.head.kind == SyntaxKind::IfDirectiveTrivia {
                return false;
            }
            current = c.tail;
        }
        false
    }

    /// `HasUnfinishedIf()`.
    pub(crate) fn has_unfinished_if(self, store: &DirectiveStore) -> bool {
        let prev = store.get_previous_if_elif_else_or_region(self.head);
        store.cell(prev).is_some_and(|c| c.head.kind != SyntaxKind::RegionDirectiveTrivia)
    }

    /// `HasPreviousIfOrElif()`.
    pub(crate) fn has_previous_if_or_elif(self, store: &DirectiveStore) -> bool {
        let prev = store.get_previous_if_elif_else_or_region(self.head);
        store
            .cell(prev)
            .is_some_and(|c| matches!(c.head.kind, SyntaxKind::IfDirectiveTrivia | SyntaxKind::ElifDirectiveTrivia))
    }

    /// `HasUnfinishedRegion()`.
    pub(crate) fn has_unfinished_region(self, store: &DirectiveStore) -> bool {
        let prev = store.get_previous_if_elif_else_or_region(self.head);
        store.cell(prev).is_some_and(|c| c.head.kind == SyntaxKind::RegionDirectiveTrivia)
    }

    /// `Add(directive)`.
    pub(crate) fn add(self, store: &mut DirectiveStore, directive: Directive) -> DirectiveStack {
        match directive.kind {
            SyntaxKind::EndIfDirectiveTrivia if store.cell(store.get_previous_if(self.head)).is_some() => {
                DirectiveStack {
                    head: store.complete_if(self.head),
                    seen_any_if_directives: self.seen_any_if_directives,
                }
            }
            // remove region directives from stack but leave everything else
            SyntaxKind::EndRegionDirectiveTrivia if store.cell(store.get_previous_region(self.head)).is_some() => {
                DirectiveStack {
                    head: store.complete_region(self.head),
                    seen_any_if_directives: self.seen_any_if_directives,
                }
            }
            // no matching if (region) directive !! leave directive alone
            _ => DirectiveStack {
                head: store.cons(directive, self.head),
                seen_any_if_directives: self.seen_any_if_directives || directive.kind == SyntaxKind::IfDirectiveTrivia,
            },
        }
    }
}
