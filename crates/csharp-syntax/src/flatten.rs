//! Green tree to `psi_builder::Tree<CSharp>`: Roslyn's red tree, walked in `ChildNodesAndTokens()` order without
//! recursion. A node is a composite over its full span; a list's elements are its parent's children and a null slot
//! is skipped; a token is its leading trivia's leaves, its own leaf over its span (`set_missing` when missing) and its
//! trailing trivia's leaves (`set_trailing`), all children of the token's parent; a structured trivia is a subtree whose
//! root has the structure's kind, its children the structure's. Positions are running sums of full widths, as the red
//! tree's are, so missing, merged and nested-lexer tokens land where Roslyn puts them.
//!
//! The same walk collects the diagnostics in Roslyn's order (`SyntaxTreeDiagnosticEnumerator`: a node's before its
//! children's, a token's after its leading trivia's), each at its element's start plus its offset, clamped to the
//! tree's length.

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
}

impl Flattener {
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
        let mut walk =
            Walk { arena, source, builder, diagnostics: Vec::new(), length, position: 0, last_error_node: None };
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
        let tree = walk.builder.finish(source.clone(), file_kind);
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
        let mut walk =
            Walk { arena, source, builder, diagnostics: Vec::new(), length, position: 0, last_error_node: None };
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

    /// Records the diagnostics `ids` of the tree node `node`, at `start` plus each one's offset.
    fn report(&mut self, node: NodeId, start: u32, ids: &[DiagnosticId]) {
        for &d in ids {
            let info = self.arena.diagnostic(d);
            let s = (start as i64 + info.offset as i64).clamp(0, self.length as i64) as u32;
            let e = (s as i64 + info.width.max(0) as i64).min(self.length as i64) as u32;
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
