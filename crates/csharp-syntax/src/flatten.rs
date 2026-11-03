//! Green tree to `psi_builder::Tree<CSharp>`: Roslyn's red tree, walked in `ChildNodesAndTokens()` order without
//! recursion. A node is a composite over its full span; a list's elements are its parent's children and a null slot
//! is skipped; a token is its leading trivia's leaves, its own leaf over its span (`set_missing` when missing) and its
//! trailing trivia's leaves (`set_trailing`), all children of the token's parent; a structured trivia is a subtree whose
//! root has the structure's kind, its children the structure's. Positions are running sums of full widths, as the red
//! tree's are, so missing, merged and nested-lexer tokens land where Roslyn puts them.
//!
//! The same walk collects the diagnostics in Roslyn's order (`SyntaxTreeDiagnosticEnumerator`: a node's before its
//! children's, a token's after its leading trivia's), each at its element's start plus its offset, clamped to the
//! tree's length. Roslyn's offsets are UTF-16 widths and ours byte widths of the same text, so the sums agree but for
//! a diagnostic `SyntaxFirstTokenReplacer` shifted by the width of skipped text placed before the element (Roslyn
//! counts it although the element's start does not move): its location is summed in UTF-16 over those texts and
//! mapped back to bytes as the oracle maps Roslyn's positions.
//!
//! The tree's text is the source, unless the green tree does not spell it at the positions the walk gives (Roslyn
//! drops a token now and then, as `ParseConversionOperatorDeclaration` the modifier it replaces by a skipped one):
//! then it is the green tree's own text from there on, `ToFullString()`, so every element shows its own text at its
//! red position.
//!
//! A file's tree is written in place: `run_columns` walks the arena's records (`Records`) and writes each node by its
//! id into the tree builder's columns, sized up front to `GreenArena::tree_node_estimate` (`TreeBuilder::writer`), so
//! the walk keeps the next id, the position and the run of slots it is in in registers and pushes only what it comes
//! back to. A tree of more nodes than the estimate (records shared by two parents) is walked again into columns of its
//! exact size, which the first walk counted. `flatten_tokens` (a lexer's tokens, Roslyn's tests) walks frames through
//! the builder's push API.

use std::borrow::Cow;
use std::sync::Arc;

use psi_builder::{ColumnWriter, ElementType, NodeId, TreeBuilder};

use crate::diagnostic;
use crate::green::{
    DiagnosticId, FIXED_TEXT, Green, GreenArena, IS_NOT_MISSING, NO_VALUE, OWNED_TEXT, Records, TAG_LIST, TAG_NODE,
    TAG_TOKEN, Trivia,
};
use crate::language::CSharp;
use crate::syntax_kind::{NodeKind, TokenKind};
use crate::tree::{Diagnostic, FileKind};

/// Where the walk is.
#[derive(Copy, Clone, Debug)]
enum Frame {
    /// The children of `green` (a node, or the structure of a structured trivia) under the tree node `id`: the next is
    /// slot `slot`, element `element` of it when that slot is a list.
    Children { green: Green, id: NodeId, slot: u32, element: u32 },
    /// A token under `parent`: `phase` 0 its leading trivia (the next at `index`), 1 the token, 2 its trailing trivia.
    Token { green: Green, parent: NodeId, phase: u8, index: u32 },
}

/// Where the column walk resumes (`Flattener::run_columns`).
#[derive(Copy, Clone, Debug)]
enum Fast {
    /// A run of slots to come back to.
    Slots(Slots),
    /// The rest of a token's trivia after a structured one: trivia `at..end` of the arena's trivia references,
    /// trailing or not; when `token` is not null they are its leading trivia, and the token and its trailing trivia
    /// follow.
    Trivia { at: u32, end: u32, parent: u32, trailing: bool, token: Green },
}

/// The elements `at..end` of the arena's slots, children of the tree node `parent`; `close`, when not `NONE`, is the
/// tree node to close after them; `in_list`: they are a list's elements, so a list among them is a composite of its
/// own (`next_child`).
#[derive(Copy, Clone, Debug)]
struct Slots {
    at: u32,
    end: u32,
    parent: u32,
    close: u32,
    in_list: bool,
}

const NONE: u32 = u32::MAX;

/// The walk's buffers, kept by a `Parser` from one file to the next.
#[derive(Default)]
pub(crate) struct Flattener {
    stack: Vec<Frame>,
    /// The column walk's stack (`run_columns`).
    fast: Vec<Fast>,
}

/// What a walk returns: the tree, its diagnostics in Roslyn's order, and the value texts of its tokens whose
/// `ValueText` is not their text (`Tree::value_text`).
pub(crate) struct Flat {
    pub(crate) tree: psi_builder::Tree<CSharp>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) values: Values,
}

/// The `ValueText` of each token leaf whose value the arena keeps (`GreenArena::token_value`): the leaf's id and the
/// end of its value in `text`, in id order, each value starting where the one before ends.
#[derive(Default)]
pub(crate) struct Values {
    pub(crate) ends: Vec<(u32, u32)>,
    pub(crate) text: String,
}

/// Where the error messages of the tree go (`TreeBuilder::set_error`, `ColumnWriter::set_error`).
trait ErrorSink {
    fn set_error(&mut self, node: NodeId, message: Cow<'static, str>);
}

impl ErrorSink for TreeBuilder<CSharp> {
    fn set_error(&mut self, node: NodeId, message: Cow<'static, str>) {
        TreeBuilder::set_error(self, node, message);
    }
}

impl ErrorSink for ColumnWriter<'_, CSharp> {
    fn set_error(&mut self, node: NodeId, message: Cow<'static, str>) {
        ColumnWriter::set_error(self, node, message);
    }
}

/// What a walk keeps besides the tree's nodes: the diagnostics, the values, the text.
struct Walk<'a> {
    arena: &'a GreenArena,
    source: &'a [u8],
    diagnostics: Vec<Diagnostic>,
    /// The tree's length: diagnostics are clamped to it.
    length: u32,
    /// The running position: the full start of what comes next.
    position: u32,
    /// The last node that got an error message (`TreeBuilder::set_error` takes one per node, in id order).
    last_error_node: Option<NodeId>,
    /// UTF-16 positions of `source` for the shifted diagnostics.
    utf16: Utf16Cursor,
    /// Where in `source` the text of the next leaf is when it does not say (a token whose kind has no fixed text but
    /// is made by kind, as a discard turned back into an identifier): after the last leaf's.
    source_next: u32,
    /// The tree's text from the first leaf whose text is not the source's at its position on (`ToFullString()`).
    text: Option<Vec<u8>>,
    values: Values,
}

/// A byte offset of a text and its UTF-16 position, moved to the next one asked (diagnostics come in walk order, so
/// near each other). A lead byte counts its character's code units (2 for a 4-byte sequence), any other byte none.
#[derive(Default)]
struct Utf16Cursor {
    byte: usize,
    unit: i64,
    /// The text's UTF-16 length, once asked.
    length: Option<i64>,
}

/// The UTF-16 code units of the character `b` leads (none for a continuation byte).
fn utf16_units(b: u8) -> i64 {
    match b {
        0x80..=0xBF => 0,
        0xF0..=0xFF => 2,
        _ => 1,
    }
}

impl Utf16Cursor {
    /// The UTF-16 position of byte `byte` of `text` (at most its end).
    fn units_at(&mut self, text: &[u8], byte: usize) -> i64 {
        let byte = byte.min(text.len());
        while self.byte < byte {
            self.unit += utf16_units(text[self.byte]);
            self.byte += 1;
        }
        while self.byte > byte {
            self.byte -= 1;
            self.unit -= utf16_units(text[self.byte]);
        }
        self.unit
    }

    /// The text's UTF-16 length.
    fn length(&mut self, text: &[u8]) -> i64 {
        *self.length.get_or_insert_with(|| text.iter().map(|&b| utf16_units(b)).sum())
    }

    /// The byte offset of UTF-16 position `unit` of `text` as the oracle maps one (`tools/csharp-oracle/README.md`,
    /// "Offsets"): a character's offset, or that offset + 2 between the halves of a surrogate pair; the text's end past
    /// it, its start before it.
    fn byte_at(&mut self, text: &[u8], unit: i64) -> usize {
        // to the start of the character the cursor is in: its lead byte holds its units
        while self.byte > 0 && self.byte < text.len() && utf16_units(text[self.byte]) == 0 {
            self.byte -= 1;
            self.unit -= utf16_units(text[self.byte]);
        }
        while self.unit > unit && self.byte > 0 {
            self.byte -= 1;
            self.unit -= utf16_units(text[self.byte]);
            if self.unit < unit {
                return self.byte + 2;
            }
        }
        while self.unit < unit && self.byte < text.len() {
            let units = utf16_units(text[self.byte]);
            if self.unit + units > unit {
                return self.byte + 2;
            }
            self.unit += units;
            self.byte += 1;
            while self.byte < text.len() && utf16_units(text[self.byte]) == 0 {
                self.byte += 1;
            }
        }
        self.byte
    }
}

/// The nodes of the columns `flatten` writes a tree into first (`TreeBuilder::writer` sizes and zeroes them): the
/// arena's estimate (`GreenArena::tree_node_estimate`), which counts the garbage of speculation too, at most 2 nodes a
/// source byte (the corpora's files have at most 1.41) and 16 for an abandoned parse, whose garbage is up to the arena
/// budget while its tree is a few nodes. A tree of more nodes takes a second walk into columns of its size.
fn first_capacity(estimate: usize, text_len: usize, abandoned: bool) -> usize {
    if abandoned { 16 } else { estimate.min(text_len.saturating_mul(2) + 64) }
}

impl Flattener {
    /// The heap bytes of the walk's stacks, by capacity.
    pub(crate) fn heap_bytes(&self) -> usize {
        psi_builder::buffers::vec_bytes(&self.stack) + psi_builder::buffers::vec_bytes(&self.fast)
    }

    /// Empties the stacks, shrunk to their share of `shrink`.
    pub(crate) fn shrink(&mut self, shrink: psi_builder::buffers::Shrink) {
        shrink.vec(&mut self.stack);
        shrink.vec(&mut self.fast);
    }

    /// The tree of the green `root` (a node) over `source`; its diagnostics in Roslyn's order. `abandoned`: the parse
    /// was abandoned (`ParseWithStackGuard`), so the tree is the text skipped and a few nodes.
    pub(crate) fn flatten(
        &mut self,
        arena: &GreenArena,
        root: Green,
        source: &Arc<[u8]>,
        builder: &mut TreeBuilder<CSharp>,
        file_kind: FileKind,
        abandoned: bool,
    ) -> Flat {
        let capacity = first_capacity(arena.tree_node_estimate(), source.len(), abandoned);
        self.flatten_with_capacity(arena, root, source, builder, file_kind, capacity)
    }

    /// `flatten` into columns of `capacity` nodes: again into columns of the tree's size when it has more.
    pub(crate) fn flatten_with_capacity(
        &mut self,
        arena: &GreenArena,
        root: Green,
        source: &Arc<[u8]>,
        builder: &mut TreeBuilder<CSharp>,
        file_kind: FileKind,
        capacity: usize,
    ) -> Flat {
        let length = arena.full_width(root);
        let start = builder.len();
        let mut walk = Walk::new(arena, source, length);
        let end = self.run_columns(&mut walk, &mut builder.writer(capacity), root, start);
        let nodes = (end - start) as usize;
        if nodes > capacity {
            // a tree of more nodes than the estimate (records shared): the walk gave every node its id, so it counted
            // them, and the same walk fits columns of that size
            builder.truncate(start);
            walk = Walk::new(arena, source, length);
            let end_again = self.run_columns(&mut walk, &mut builder.writer(nodes), root, start);
            debug_assert_eq!(end_again, end);
        }
        builder.truncate(end);
        let text = walk.text.take().map_or_else(|| source.clone(), Arc::from);
        let tree = builder.finish(text, file_kind);
        Flat { tree, diagnostics: walk.diagnostics, values: walk.values }
    }

    /// The tree of `tokens` (a lexer's, `SyntaxFactory.ParseTokens`) as children of a `CompilationUnit` root, the
    /// diagnostics of each in token order.
    pub(crate) fn flatten_tokens(
        &mut self,
        arena: &GreenArena,
        tokens: &[Green],
        source: &Arc<[u8]>,
        builder: &mut TreeBuilder<CSharp>,
        file_kind: FileKind,
    ) -> Flat {
        let length = tokens.iter().map(|&t| arena.full_width(t)).fold(0u32, u32::wrapping_add);
        let mut walk = Walk::new(arena, source, length);
        let root = builder.open(None, ElementType::Node(NodeKind::CompilationUnit), 0);
        for &token in tokens {
            self.stack.clear();
            self.stack.push(Frame::Token { green: token, parent: root, phase: 0, index: 0 });
            self.run(&mut walk, builder);
        }
        builder.close(root, walk.position);
        let tree = builder.finish(source.clone(), file_kind);
        Flat { tree, diagnostics: walk.diagnostics, values: walk.values }
    }

    fn run(&mut self, walk: &mut Walk<'_>, builder: &mut TreeBuilder<CSharp>) {
        let arena = walk.arena;
        while let Some(&frame) = self.stack.last() {
            match frame {
                Frame::Children { green, id, slot, element } => {
                    let (child, next_slot, next_element) = next_child(arena, green, slot, element);
                    if child.is_null() {
                        builder.close(id, walk.position);
                        self.stack.pop();
                        continue;
                    }
                    if let Some(Frame::Children { slot, element, .. }) = self.stack.last_mut() {
                        *slot = next_slot;
                        *element = next_element;
                    }
                    if child.is_token() {
                        self.stack.push(Frame::Token { green: child, parent: id, phase: 0, index: 0 });
                    } else {
                        let child_id = walk.open(builder, Some(id), child);
                        self.stack.push(Frame::Children { green: child, id: child_id, slot: 0, element: 0 });
                    }
                }
                Frame::Token { green, parent, phase, index } => {
                    let list = match phase {
                        0 => arena.leading_trivia(green),
                        2 => arena.trailing_trivia(green),
                        _ => {
                            walk.token(builder, parent, green);
                            if let Some(Frame::Token { phase, index, .. }) = self.stack.last_mut() {
                                *phase = 2;
                                *index = 0;
                            }
                            continue;
                        }
                    };
                    let items = arena.trivia_items(list);
                    let Some(&trivia) = items.get(index as usize) else {
                        if phase == 0 {
                            if let Some(Frame::Token { phase, .. }) = self.stack.last_mut() {
                                *phase = 1;
                            }
                        } else {
                            self.stack.pop();
                        }
                        continue;
                    };
                    if let Some(Frame::Token { index, .. }) = self.stack.last_mut() {
                        *index += 1;
                    }
                    if let Some(frame) = walk.trivia(builder, parent, trivia, phase == 2) {
                        self.stack.push(frame);
                    }
                }
            }
        }
    }

    /// The tree of `root` from id `first` into columns written by id (`ColumnWriter`), in the order of `run`'s
    /// frames: the ids, the running position and where the next source text is stay in registers, the current run of
    /// slots too, and only the runs to come back to go on the stack; a token's trivia are walked in place unless one
    /// is structured. The tree's nodes are `first..` the returned end, which is past the writer's columns when the
    /// tree does not fit them (the writes past them dropped).
    fn run_columns(&mut self, walk: &mut Walk<'_>, w: &mut ColumnWriter<'_, CSharp>, root: Green, first: u32) -> u32 {
        let r = walk.arena.records();
        let stack = &mut self.fast;
        stack.clear();
        let mut hot = Hot { id: first, pos: 0, src_next: 0, diverged: false };
        let mut cur;
        if root.is_token() {
            // not a tree Roslyn makes: the token under a compilation unit
            let id = hot.id;
            w.open(id, None, ElementType::Node(NodeKind::CompilationUnit), 0, false);
            hot.id += 1;
            match hot.trivia_run(walk, w, r, Run::leading_of(r, root, id), stack) {
                Some(next) => {
                    stack.insert(0, Fast::Slots(Slots { at: 0, end: 0, parent: id, close: id, in_list: false }));
                    cur = next;
                }
                None => cur = Slots { at: 0, end: 0, parent: id, close: id, in_list: false },
            }
        } else {
            cur = hot.open(walk, w, r, NONE, root, false);
        }
        loop {
            while cur.at < cur.end {
                let child = r.slots[cur.at as usize];
                cur.at += 1;
                match child.tag() {
                    TAG_TOKEN => {
                        let below = stack.len();
                        if let Some(next) = hot.trivia_run(walk, w, r, Run::leading_of(r, child, cur.parent), stack) {
                            // under the rest of the token: the slots after it come back after the token is done
                            stack.insert(below, Fast::Slots(cur));
                            cur = next;
                        }
                    }
                    TAG_LIST if !cur.in_list => {
                        let d = &r.nodes[child.index()];
                        stack.push(Fast::Slots(cur));
                        cur = Slots {
                            at: d.slot_start,
                            end: d.slot_start + d.slot_count,
                            parent: cur.parent,
                            close: NONE,
                            in_list: true,
                        };
                    }
                    TAG_NODE | TAG_LIST => {
                        let next = hot.open(walk, w, r, cur.parent, child, false);
                        stack.push(Fast::Slots(cur));
                        cur = next;
                    }
                    _ => {}
                }
            }
            if cur.close != NONE {
                w.close(cur.close, hot.pos, hot.id);
            }
            loop {
                match stack.pop() {
                    None => {
                        walk.position = hot.pos;
                        return hot.id;
                    }
                    Some(Fast::Slots(s)) => {
                        cur = s;
                        break;
                    }
                    Some(Fast::Trivia { at, end, parent, trailing, token }) => {
                        let run = Run { at, end, parent, trailing, token };
                        if let Some(next) = hot.trivia_run(walk, w, r, run, stack) {
                            cur = next;
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// The column walk's running state (`Flattener::run_columns`), kept in registers: the next node's id, the running
/// position, where the next leaf's source text is (`Walk::source_next`), and whether the tree's text left the source.
#[derive(Copy, Clone)]
struct Hot {
    id: u32,
    pos: u32,
    src_next: u32,
    diverged: bool,
}

/// A run of trivia of `trivia_refs` (`Fast::Trivia`).
#[derive(Copy, Clone)]
struct Run {
    at: u32,
    end: u32,
    parent: u32,
    trailing: bool,
    token: Green,
}

impl Run {
    /// The leading trivia of `token` under `parent`, the token and its trailing trivia to follow.
    #[inline(always)]
    fn leading_of(r: Records<'_>, token: Green, parent: u32) -> Run {
        let d = &r.tokens[token.index()];
        Run { at: d.leading.start, end: d.leading.start + d.leading.len, parent, trailing: false, token }
    }
}

impl Hot {
    /// Opens the composite of the node or list `green` under `parent` (`NONE`: the root), `trailing` as a trailing
    /// structured trivia, and reports its diagnostics; its slots are the run to walk.
    #[inline(always)]
    fn open(
        &mut self,
        walk: &mut Walk<'_>,
        w: &mut ColumnWriter<'_, CSharp>,
        r: Records<'_>,
        parent: u32,
        green: Green,
        trailing: bool,
    ) -> Slots {
        let id = self.id;
        self.id += 1;
        let (kind, at, end, diags) = match green.tag() {
            TAG_NODE | TAG_LIST => {
                let d = &r.nodes[green.index()];
                (d.kind, d.slot_start, d.slot_start + d.slot_count, d.diags.len)
            }
            // a token as a structure: a composite of no children (`next_child`)
            _ => (walk.arena.kind(green), 0, 0, walk.arena.diagnostics(green).len() as u32),
        };
        let kind = kind.node_kind().unwrap_or(NodeKind::SkippedTokensTrivia);
        let parent = (parent != NONE).then_some(NodeId(parent));
        w.open(id, parent, ElementType::Node(kind), self.pos, trailing);
        if diags != 0 {
            walk.report_node(w, NodeId(id), self.pos, green);
        }
        Slots { at, end, parent: id, close: id, in_list: green.is_list() }
    }

    /// The trivia of `run`, then its token and the token's trailing trivia (`Fast::Trivia`). At a structured trivia it
    /// opens the structure's composite, leaves the rest of the run on `stack` and returns the structure's slots.
    #[inline(always)]
    fn trivia_run(
        &mut self,
        walk: &mut Walk<'_>,
        w: &mut ColumnWriter<'_, CSharp>,
        r: Records<'_>,
        mut run: Run,
        stack: &mut Vec<Fast>,
    ) -> Option<Slots> {
        loop {
            while run.at < run.end {
                let t = r.trivia_refs[run.at as usize];
                run.at += 1;
                let td = &r.trivia[t.0 as usize];
                if !td.structure.is_null() {
                    let structure = td.structure;
                    let next = self.open(walk, w, r, run.parent, structure, run.trailing);
                    stack.push(Fast::Trivia {
                        at: run.at,
                        end: run.end,
                        parent: run.parent,
                        trailing: run.trailing,
                        token: run.token,
                    });
                    return Some(next);
                }
                let kind = td.kind.token_kind().unwrap_or(TokenKind::BadToken);
                let start = self.pos;
                let end = start.wrapping_add(td.width);
                let leaf = self.id;
                w.leaf(leaf, NodeId(run.parent), kind, start, end, run.trailing, false);
                self.id += 1;
                self.source_text(walk, start, td.start, td.width);
                if td.diags.len != 0 {
                    walk.report_trivia(w, NodeId(leaf), start, t);
                }
                self.pos = end;
            }
            if run.token.is_null() {
                return None;
            }
            let token = run.token;
            self.token_leaf(walk, w, r, NodeId(run.parent), token);
            let d = &r.tokens[token.index()];
            run = Run {
                at: d.trailing.start,
                end: d.trailing.start + d.trailing.len,
                parent: run.parent,
                trailing: true,
                token: Green::NULL,
            };
        }
    }

    /// `token`'s own leaf (`Walk::token` over the records).
    #[inline(always)]
    fn token_leaf(
        &mut self,
        walk: &mut Walk<'_>,
        w: &mut ColumnWriter<'_, CSharp>,
        r: Records<'_>,
        parent: NodeId,
        green: Green,
    ) {
        let d = &r.tokens[green.index()];
        let kind = d.kind.token_kind().unwrap_or(TokenKind::BadToken);
        let start = self.pos;
        let end = start.wrapping_add(d.width);
        let leaf = self.id;
        w.leaf(leaf, parent, kind, start, end, false, d.flags & IS_NOT_MISSING == 0);
        self.id += 1;
        if d.value != NO_VALUE {
            walk.value(NodeId(leaf), green);
        }
        if d.flags & (IS_NOT_MISSING | FIXED_TEXT | OWNED_TEXT) == IS_NOT_MISSING {
            self.source_text(walk, start, d.start, d.width);
        } else {
            self.src_next = walk.own_token_text(self.src_next, start, end, green);
            self.diverged = walk.text.is_some();
        }
        if d.diags.len != 0 {
            walk.report_token(w, NodeId(leaf), start, green);
        }
        self.pos = end;
    }

    /// `Walk::leaf_text(start, at, width, None)`, the usual case (the source's text where the leaf is) inline.
    #[inline(always)]
    fn source_text(&mut self, walk: &mut Walk<'_>, start: u32, at: u32, width: u32) {
        if !self.diverged && at == start {
            self.src_next = at.saturating_add(width);
            return;
        }
        self.src_next = walk.source_text(self.src_next, start, at, width);
        self.diverged = walk.text.is_some();
    }
}

/// The next child of `green` from slot `slot` (element `element` of a list slot): the child, and where the one after
/// it is. Null when there is none.
fn next_child(arena: &GreenArena, green: Green, mut slot: u32, mut element: u32) -> (Green, u32, u32) {
    let count = arena.slot_count(green) as u32;
    while slot < count {
        let s = arena.slot(green, slot as usize);
        if s.is_null() {
            slot += 1;
            continue;
        }
        if s.is_list() && !green.is_list() {
            let elements = arena.slot_count(s) as u32;
            while element < elements {
                let e = arena.slot(s, element as usize);
                element += 1;
                if !e.is_null() {
                    return (e, slot, element);
                }
            }
            slot += 1;
            element = 0;
            continue;
        }
        return (s, slot + 1, 0);
    }
    (Green::NULL, slot, element)
}

impl<'a> Walk<'a> {
    fn new(arena: &'a GreenArena, source: &'a [u8], length: u32) -> Walk<'a> {
        Walk {
            arena,
            source,
            diagnostics: Vec::new(),
            length,
            position: 0,
            last_error_node: None,
            utf16: Utf16Cursor::default(),
            source_next: 0,
            text: None,
            values: Values::default(),
        }
    }
}

impl Walk<'_> {
    /// Opens the composite of the node `green` at the running position and reports the node's diagnostics.
    fn open(&mut self, builder: &mut TreeBuilder<CSharp>, parent: Option<NodeId>, green: Green) -> NodeId {
        let kind = self.arena.kind(green).node_kind().unwrap_or(NodeKind::SkippedTokensTrivia);
        let id = builder.open(parent, ElementType::Node(kind), self.position);
        if self.arena.diagnostics(green).len() != 0 {
            let start = self.position.wrapping_add(self.arena.leading_trivia_width(green));
            let ids: Vec<DiagnosticId> = self.arena.diagnostics(green).collect();
            self.report(builder, id, start, &ids);
        }
        id
    }

    /// The token's leaf over its span, its diagnostics.
    fn token(&mut self, builder: &mut TreeBuilder<CSharp>, parent: NodeId, green: Green) {
        let kind = self.arena.kind(green).token_kind().unwrap_or(TokenKind::BadToken);
        let start = self.position;
        let end = start.wrapping_add(self.arena.width(green));
        let leaf = builder.leaf(parent, kind, start, end);
        if self.arena.is_missing(green) {
            builder.set_missing(leaf);
        }
        if let Some(value) = self.arena.token_value(green) {
            self.values.text.push_str(value);
            self.values.ends.push((leaf.0, self.values.text.len() as u32));
        }
        match self.arena.token_source_range(green) {
            Some((at, width)) => self.leaf_text(start, at, width, None),
            None => {
                let own = self.arena.token_text(green, self.source);
                if own.len() == (end - start) as usize {
                    self.leaf_text(start, start, own.len() as u32, Some(own));
                } else {
                    self.leaf_text(start, self.source_next, end - start, None);
                }
            }
        }
        if self.arena.diagnostics(green).len() != 0 {
            let ids: Vec<DiagnosticId> = self.arena.diagnostics(green).collect();
            self.report(builder, leaf, start, &ids);
        }
        self.position = end;
    }

    /// A trivia: a leaf, or the root of its structure's subtree (then the frame that walks the structure's children).
    fn trivia(
        &mut self,
        builder: &mut TreeBuilder<CSharp>,
        parent: NodeId,
        trivia: Trivia,
        trailing: bool,
    ) -> Option<Frame> {
        let structure = self.arena.trivia_structure(trivia);
        let own: Vec<DiagnosticId> = self.arena.trivia_diagnostics(trivia).collect();
        if structure.is_null() {
            let kind = self.arena.trivia_kind(trivia).token_kind().unwrap_or(TokenKind::BadToken);
            let start = self.position;
            let end = start.wrapping_add(self.arena.trivia_width(trivia));
            let leaf = builder.leaf(parent, kind, start, end);
            if trailing {
                builder.set_trailing(leaf);
            }
            self.leaf_text(start, self.arena.trivia_start(trivia), end - start, None);
            if !own.is_empty() {
                self.report(builder, leaf, start, &own);
            }
            self.position = end;
            return None;
        }
        let id = self.open(builder, Some(parent), structure);
        if trailing {
            builder.set_trailing(id);
        }
        Some(Frame::Children { green: structure, id, slot: 0, element: 0 })
    }

    // The column walk's cold paths: `Hot` keeps the position and `source_next` itself and passes them in.

    /// `leaf_text(start, at, width, None)` with `source_next` given; the new `source_next`.
    #[cold]
    #[inline(never)]
    fn source_text(&mut self, source_next: u32, start: u32, at: u32, width: u32) -> u32 {
        self.source_next = source_next;
        self.leaf_text(start, at, width, None);
        self.source_next
    }

    /// The text of a token that is not the source's (`token`'s `None` arm); the new `source_next`.
    #[cold]
    #[inline(never)]
    fn own_token_text(&mut self, source_next: u32, start: u32, end: u32, green: Green) -> u32 {
        self.source_next = source_next;
        let own = self.arena.token_text(green, self.source);
        if own.len() == (end - start) as usize {
            self.leaf_text(start, start, own.len() as u32, Some(own));
        } else {
            self.leaf_text(start, self.source_next, end - start, None);
        }
        self.source_next
    }

    /// The value of the token leaf `leaf` of `green`, when the arena keeps one.
    #[cold]
    #[inline(never)]
    fn value(&mut self, leaf: NodeId, green: Green) {
        if let Some(value) = self.arena.token_value(green) {
            self.values.text.push_str(value);
            self.values.ends.push((leaf.0, self.values.text.len() as u32));
        }
    }

    /// The diagnostics of the node `green` opened as `id` at `position`.
    #[cold]
    #[inline(never)]
    fn report_node(&mut self, sink: &mut impl ErrorSink, id: NodeId, position: u32, green: Green) {
        let start = position.wrapping_add(self.arena.leading_trivia_width(green));
        let ids: Vec<DiagnosticId> = self.arena.diagnostics(green).collect();
        self.report(sink, id, start, &ids);
    }

    /// The diagnostics of the token `green`, its leaf `leaf` at `start`.
    #[cold]
    #[inline(never)]
    fn report_token(&mut self, sink: &mut impl ErrorSink, leaf: NodeId, start: u32, green: Green) {
        let ids: Vec<DiagnosticId> = self.arena.diagnostics(green).collect();
        self.report(sink, leaf, start, &ids);
    }

    /// The diagnostics of the unstructured `trivia`, its leaf `leaf` at `start`.
    #[cold]
    #[inline(never)]
    fn report_trivia(&mut self, sink: &mut impl ErrorSink, leaf: NodeId, start: u32, trivia: Trivia) {
        let ids: Vec<DiagnosticId> = self.arena.trivia_diagnostics(trivia).collect();
        self.report(sink, leaf, start, &ids);
    }

    /// The text of a leaf at `start` (a token or an unstructured trivia): `own`, or the source's `width` bytes at
    /// `at`. The tree's text starts its own copy at the first leaf whose text is not the source's at `start`.
    #[inline(never)]
    fn leaf_text(&mut self, start: u32, at: u32, width: u32, own: Option<&[u8]>) {
        let source = self.source;
        let bytes =
            own.unwrap_or_else(|| source.get(at as usize..(at as usize).saturating_add(width as usize)).unwrap_or(b""));
        if own.is_none() {
            self.source_next = at.saturating_add(width);
        } else {
            self.source_next = self.source_next.saturating_add(width);
        }
        if self.text.is_none() {
            let here = source.get(start as usize..(start as usize).saturating_add(width as usize)).unwrap_or(b"");
            if (own.is_none() && at == start) || (own.is_some() && bytes == here) {
                return;
            }
            self.text = Some(source.get(..start as usize).unwrap_or(source).to_vec());
        }
        if let Some(text) = &mut self.text {
            text.extend_from_slice(bytes);
        }
    }

    /// The span of a diagnostic `SyntaxFirstTokenReplacer` shifted, of an element at `start`, as Roslyn computes it in
    /// UTF-16: the text from `start` its offset measured without the shift, plus the shift's text, its width that of
    /// the text it was made for; then clamped to the tree's length and mapped back to bytes.
    fn shifted_span(&mut self, start: u32, info: &crate::green::DiagnosticInfo) -> (u32, u32) {
        let text = self.source;
        let end = (self.length as usize).min(text.len()) as i64;
        let at = |offset: i64| offset.clamp(0, end) as usize;
        let intended = at(start as i64 + info.offset as i64 - info.shift as i64);
        let from = at(start as i64 + info.shift_from as i64);
        let to = at(start as i64 + info.shift_from as i64 + info.shift as i64);
        let shift = self.utf16.units_at(text, to.max(from)) - self.utf16.units_at(text, to.min(from));
        let shift = if info.shift < 0 { -shift } else { shift };
        let width_end = at(intended as i64 + info.width.max(0) as i64);
        let width = self.utf16.units_at(text, width_end) - self.utf16.units_at(text, intended);
        let length = self.utf16.length(&text[..end as usize]);
        let s = (self.utf16.units_at(text, intended) + shift).clamp(0, length);
        let e = (s + width).min(length);
        let s = self.utf16.byte_at(text, s) as u32;
        (s, self.utf16.byte_at(text, e) as u32)
    }

    /// Records the diagnostics `ids` of the tree node `node`, at `start` plus each one's offset.
    #[cold]
    #[inline(never)]
    fn report(&mut self, sink: &mut impl ErrorSink, node: NodeId, start: u32, ids: &[DiagnosticId]) {
        for &d in ids {
            let info = *self.arena.diagnostic(d);
            let (s, e) = if info.shift == 0 {
                let s = (start as i64 + info.offset as i64).clamp(0, self.length as i64) as u32;
                (s, (s as i64 + info.width.max(0) as i64).min(self.length as i64) as u32)
            } else {
                self.shifted_span(start, &info)
            };
            let diagnostic::Message { text: message, lone_surrogates } =
                diagnostic::message(self.arena, self.source, d);
            if self.last_error_node != Some(node) {
                sink.set_error(node, Cow::Owned(message.clone()));
                self.last_error_node = Some(node);
            }
            self.diagnostics.push(Diagnostic { node, code: info.code, start: s, end: e, message, lone_surrogates });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;
    use crate::green::{DiagArg, TokenText, TriviaList};
    use crate::syntax_kind::SyntaxKind;

    /// What a flattened tree holds, node by node, with its diagnostics, values and text.
    #[derive(PartialEq, Debug)]
    struct Facts {
        nodes: Vec<String>,
        diagnostics: Vec<Diagnostic>,
        values: Vec<(u32, u32)>,
        value_text: String,
        text: Vec<u8>,
    }

    fn facts(flat: &Flat) -> Facts {
        let t = &flat.tree;
        let nodes = (0..t.len() as u32)
            .map(NodeId)
            .map(|id| {
                format!(
                    "{:?} {:?} {:?} {:?} {} {} {:?}",
                    t.kind(id),
                    t.range(id),
                    t.parent(id),
                    t.subtree_end(id),
                    t.is_trailing(id),
                    t.is_missing(id),
                    t.error_message(id)
                )
            })
            .collect();
        Facts {
            nodes,
            diagnostics: flat.diagnostics.clone(),
            values: flat.values.ends.clone(),
            value_text: flat.values.text.clone(),
            text: t.text(NodeId(0)).to_vec(),
        }
    }

    /// `x = y \n` with the semicolon missing, `@` skipped into the end of file's leading trivia, the global statement
    /// `copies` times in the members (records shared: one more `x = y` each, the text no longer the source's).
    fn build(arena: &mut GreenArena, copies: usize) -> Green {
        let space = |a: &mut GreenArena, at| {
            let t = a.trivia(SyntaxKind::WhitespaceTrivia, at, 1);
            a.trivia_list(&[t])
        };
        let token = |a: &mut GreenArena, kind, start, leading, trailing| {
            a.create_token(leading, kind, kind, TokenText::Source { start, width: 1 }, trailing)
        };
        let s1 = space(arena, 1);
        let x = token(arena, SyntaxKind::IdentifierToken, 0, TriviaList::EMPTY, s1);
        let s3 = space(arena, 3);
        let eq = arena.token_with_trivia(TriviaList::EMPTY, SyntaxKind::EqualsToken, s3);
        let ws = arena.trivia(SyntaxKind::WhitespaceTrivia, 5, 1);
        let nl = arena.trivia(SyntaxKind::EndOfLineTrivia, 6, 1);
        let trailing = arena.trivia_list(&[ws, nl]);
        let y = token(arena, SyntaxKind::IdentifierToken, 4, TriviaList::EMPTY, trailing);
        arena.set_token_value(y, "why");
        let left = arena.identifier_name(x);
        let right = arena.identifier_name(y);
        let assign = arena.assignment_expression(SyntaxKind::SimpleAssignmentExpression, left, eq, right);
        let semi = arena.missing_token(SyntaxKind::SemicolonToken);
        let error = arena.make_diagnostic(0, 0, ErrorCode::ERR_SemicolonExpected, &[]);
        let semi = arena.with_additional_diagnostics(semi, &[error]);
        let statement = arena.expression_statement(Green::NULL, assign, semi);
        let global = arena.global_statement(Green::NULL, Green::NULL, statement);
        let bad = token(arena, SyntaxKind::BadToken, 7, TriviaList::EMPTY, TriviaList::EMPTY);
        let unexpected = arena.make_diagnostic(0, 1, ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::TokenText(bad)]);
        let bad = arena.with_additional_diagnostics(bad, &[unexpected]);
        let skipped = arena.skipped_tokens_trivia(bad);
        let skipped = arena.structured_trivia(skipped);
        let leading = arena.trivia_list(&[skipped]);
        let eof = arena.token_with_trivia(leading, SyntaxKind::EndOfFileToken, TriviaList::EMPTY);
        let members = arena.list(&vec![global; copies]);
        arena.compilation_unit(Green::NULL, Green::NULL, Green::NULL, members, eof)
    }

    fn flatten(arena: &GreenArena, root: Green, capacity: Option<usize>) -> Flat {
        let source: Arc<[u8]> = Arc::from(&b"x = y \n@"[..]);
        let mut builder = TreeBuilder::new();
        let mut flattener = Flattener::default();
        match capacity {
            None => flattener.flatten(arena, root, &source, &mut builder, FileKind::default(), false),
            Some(c) => flattener.flatten_with_capacity(arena, root, &source, &mut builder, FileKind::default(), c),
        }
    }

    /// The tree of `root` by `run`'s frames through the builder's push API (`flatten_tokens`' walk).
    fn by_frames(arena: &GreenArena, root: Green) -> Flat {
        let source: Arc<[u8]> = Arc::from(&b"x = y \n@"[..]);
        let mut builder = TreeBuilder::new();
        let mut flattener = Flattener::default();
        let mut walk = Walk::new(arena, &source, arena.full_width(root));
        if root.is_token() {
            let id = builder.open(None, ElementType::Node(NodeKind::CompilationUnit), 0);
            flattener.stack.push(Frame::Token { green: root, parent: id, phase: 0, index: 0 });
            flattener.run(&mut walk, &mut builder);
            builder.close(id, walk.position);
        } else {
            let id = walk.open(&mut builder, None, root);
            flattener.stack.push(Frame::Children { green: root, id, slot: 0, element: 0 });
            flattener.run(&mut walk, &mut builder);
        }
        let text = walk.text.take().map_or_else(|| source.clone(), Arc::from);
        Flat { tree: builder.finish(text, FileKind::default()), diagnostics: walk.diagnostics, values: walk.values }
    }

    /// Columns too small for the tree, as the estimate is for a tree that shares records: the walk runs again into
    /// columns of the size it counted, and the tree is the one roomy columns get, the frames' tree.
    #[test]
    fn a_tree_past_its_columns_is_walked_again() {
        for copies in [1, 3, 40] {
            let mut arena = GreenArena::new();
            let root = build(&mut arena, copies);
            let roomy = facts(&flatten(&arena, root, Some(100_000)));
            assert!(roomy.nodes.len() > 10 * copies);
            assert_eq!(facts(&by_frames(&arena, root)), roomy, "{copies} copies by frames");
            assert_eq!(facts(&flatten(&arena, root, None)), roomy, "{copies} copies at the estimate");
            for capacity in [0, 1, 2, roomy.nodes.len() - 1, roomy.nodes.len()] {
                assert_eq!(facts(&flatten(&arena, root, Some(capacity))), roomy, "{copies} copies, {capacity}");
            }
            if copies == 40 {
                // the shared statement makes the tree bigger than the estimate: the second walk is the usual path
                assert!(roomy.nodes.len() > arena.tree_node_estimate());
            }
        }
        // a lone token (no tree Roslyn makes) under a compilation unit, its skipped trivia a structure
        let mut arena = GreenArena::new();
        let root = build(&mut arena, 1);
        let eof = arena.last_token(root);
        let roomy = facts(&flatten(&arena, eof, Some(100)));
        assert_eq!(facts(&by_frames(&arena, eof)), roomy);
        for capacity in 0..roomy.nodes.len() {
            assert_eq!(facts(&flatten(&arena, eof, Some(capacity))), roomy, "a token, {capacity}");
        }
    }
}
