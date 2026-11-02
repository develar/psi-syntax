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

use std::borrow::Cow;
use std::sync::Arc;

use psi_builder::{ElementType, NodeId, TreeBuilder};

use crate::diagnostic;
use crate::green::{DiagnosticId, Green, GreenArena, Trivia};
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

/// The walk's buffers, kept by a `Parser` from one file to the next.
#[derive(Default)]
pub(crate) struct Flattener {
    stack: Vec<Frame>,
}

/// What a walk builds: the tree's nodes in `builder`, and the diagnostics.
struct Walk<'a> {
    arena: &'a GreenArena,
    source: &'a [u8],
    builder: &'a mut TreeBuilder<CSharp>,
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

impl Flattener {
    /// The heap bytes of the walk's stack, by capacity.
    pub(crate) fn heap_bytes(&self) -> usize {
        psi_builder::buffers::vec_bytes(&self.stack)
    }

    /// Empties the stack, shrunk to its share of `shrink`.
    pub(crate) fn shrink(&mut self, shrink: psi_builder::buffers::Shrink) {
        shrink.vec(&mut self.stack);
    }

    /// The tree of the green `root` (a node) over `source`; its diagnostics in Roslyn's order.
    pub(crate) fn flatten(
        &mut self,
        arena: &GreenArena,
        root: Green,
        source: &Arc<[u8]>,
        builder: &mut TreeBuilder<CSharp>,
        file_kind: FileKind,
    ) -> (psi_builder::Tree<CSharp>, Vec<Diagnostic>) {
        let length = arena.full_width(root);
        let mut walk = Walk {
            arena,
            source,
            builder,
            diagnostics: Vec::new(),
            length,
            position: 0,
            last_error_node: None,
            utf16: Utf16Cursor::default(),
            source_next: 0,
            text: None,
        };
        self.stack.clear();
        if root.is_token() {
            // not a tree Roslyn makes: the token under a compilation unit
            let id = walk.builder.open(None, ElementType::Node(NodeKind::CompilationUnit), 0);
            self.stack.push(Frame::Token { green: root, parent: id, phase: 0, index: 0 });
            self.run(&mut walk);
            walk.builder.close(id, walk.position);
        } else {
            let id = walk.open(None, root);
            self.stack.push(Frame::Children { green: root, id, slot: 0, element: 0 });
            self.run(&mut walk);
        }
        let text = walk.text.take().map_or_else(|| source.clone(), Arc::from);
        let tree = walk.builder.finish(text, file_kind);
        (tree, walk.diagnostics)
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
    ) -> (psi_builder::Tree<CSharp>, Vec<Diagnostic>) {
        let length = tokens.iter().map(|&t| arena.full_width(t)).fold(0u32, u32::wrapping_add);
        let mut walk = Walk {
            arena,
            source,
            builder,
            diagnostics: Vec::new(),
            length,
            position: 0,
            last_error_node: None,
            utf16: Utf16Cursor::default(),
            source_next: 0,
            text: None,
        };
        let root = walk.builder.open(None, ElementType::Node(NodeKind::CompilationUnit), 0);
        for &token in tokens {
            self.stack.clear();
            self.stack.push(Frame::Token { green: token, parent: root, phase: 0, index: 0 });
            self.run(&mut walk);
        }
        walk.builder.close(root, walk.position);
        let tree = walk.builder.finish(source.clone(), file_kind);
        (tree, walk.diagnostics)
    }

    fn run(&mut self, walk: &mut Walk<'_>) {
        let arena = walk.arena;
        while let Some(&frame) = self.stack.last() {
            match frame {
                Frame::Children { green, id, slot, element } => {
                    let (child, next_slot, next_element) = next_child(arena, green, slot, element);
                    if child.is_null() {
                        walk.builder.close(id, walk.position);
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
                        let child_id = walk.open(Some(id), child);
                        self.stack.push(Frame::Children { green: child, id: child_id, slot: 0, element: 0 });
                    }
                }
                Frame::Token { green, parent, phase, index } => {
                    let list = match phase {
                        0 => arena.leading_trivia(green),
                        2 => arena.trailing_trivia(green),
                        _ => {
                            walk.token(parent, green);
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
                    if let Some(frame) = walk.trivia(parent, trivia, phase == 2) {
                        self.stack.push(frame);
                    }
                }
            }
        }
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

impl Walk<'_> {
    /// Opens the composite of the node `green` at the running position and reports the node's diagnostics.
    fn open(&mut self, parent: Option<NodeId>, green: Green) -> NodeId {
        let kind = self.arena.kind(green).node_kind().unwrap_or(NodeKind::SkippedTokensTrivia);
        let id = self.builder.open(parent, ElementType::Node(kind), self.position);
        if self.arena.diagnostics(green).len() != 0 {
            let start = self.position.wrapping_add(self.arena.leading_trivia_width(green));
            let ids: Vec<DiagnosticId> = self.arena.diagnostics(green).collect();
            self.report(id, start, &ids);
        }
        id
    }

    /// The token's leaf over its span, its diagnostics.
    fn token(&mut self, parent: NodeId, green: Green) {
        let kind = self.arena.kind(green).token_kind().unwrap_or(TokenKind::BadToken);
        let start = self.position;
        let end = start.wrapping_add(self.arena.width(green));
        let leaf = self.builder.leaf(parent, kind, start, end);
        if self.arena.is_missing(green) {
            self.builder.set_missing(leaf);
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
            self.report(leaf, start, &ids);
        }
        self.position = end;
    }

    /// A trivia: a leaf, or the root of its structure's subtree (then the frame that walks the structure's children).
    fn trivia(&mut self, parent: NodeId, trivia: Trivia, trailing: bool) -> Option<Frame> {
        let structure = self.arena.trivia_structure(trivia);
        let own: Vec<DiagnosticId> = self.arena.trivia_diagnostics(trivia).collect();
        if structure.is_null() {
            let kind = self.arena.trivia_kind(trivia).token_kind().unwrap_or(TokenKind::BadToken);
            let start = self.position;
            let end = start.wrapping_add(self.arena.trivia_width(trivia));
            let leaf = self.builder.leaf(parent, kind, start, end);
            if trailing {
                self.builder.set_trailing(leaf);
            }
            self.leaf_text(start, self.arena.trivia_start(trivia), end - start, None);
            if !own.is_empty() {
                self.report(leaf, start, &own);
            }
            self.position = end;
            return None;
        }
        let id = self.open(Some(parent), structure);
        if trailing {
            self.builder.set_trailing(id);
        }
        Some(Frame::Children { green: structure, id, slot: 0, element: 0 })
    }

    /// The text of a leaf at `start` (a token or an unstructured trivia): `own`, or the source's `width` bytes at
    /// `at`. The tree's text starts its own copy at the first leaf whose text is not the source's at `start`.
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
    fn report(&mut self, node: NodeId, start: u32, ids: &[DiagnosticId]) {
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
                self.builder.set_error(node, Cow::Owned(message.clone()));
                self.last_error_node = Some(node);
            }
            self.diagnostics.push(Diagnostic { node, code: info.code, start: s, end: e, message, lone_surrogates });
        }
    }
}
