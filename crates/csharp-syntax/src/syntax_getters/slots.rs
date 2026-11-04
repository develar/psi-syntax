//! Which slot of its node type each child of a node fills. The flat tree keeps no slots (`flatten`): a null slot is no
//! child, a list's elements are its parent's children, a token is its leading trivia leaves, its leaf and its trailing
//! trivia leaves, all children of the token's parent, and a structured trivia is a subtree among them. So the slots are
//! read back from the kinds, in Syntax.xml order: past the trivia, each child goes to the first slot from the current
//! one that takes its kind (`fields::SPECS`, made by `tools/csharp-gen/gen.py`'s `slot_specs`), a token or node slot
//! taking one child, a list as many as follow. Roslyn's red tree enumerates its children in that order
//! (`ChildNodesAndTokens()`), so the assignment is its slots, checked against Roslyn's own properties on every node of
//! the gate's inputs (`getters`, `tools/csharp-oracle` `getters`). A getter reads the children up to its slot and no
//! further; a list stops at the first child past it.

use psi_builder::tree::Children;
use psi_builder::{ElementType, Tree};

use super::fields::{SPECS, TRIVIA, TYPE_OF_NODE, WORDS};
use crate::language::CSharp;
use crate::syntax_kind::{NodeKind, TokenKind};
use crate::tree::NodeId;

/// A set of kinds: a bit per `TokenKind` and per `NodeKind` (`TokenKind::COUNT` + its index).
pub(crate) struct KindSet(pub(crate) [u64; WORDS]);

impl KindSet {
    #[inline]
    pub(crate) fn contains(&self, index: usize) -> bool {
        self.0.get(index / 64).is_some_and(|w| w & (1 << (index % 64)) != 0)
    }
}

/// A slot of a node type: the kinds of the children it takes (any token or node for one that is never null), the kinds
/// Syntax.xml declares for it, whether it takes several (a list), whether it is a separated list (whose separators may
/// be any token), and whether it is a token or node that is never null.
pub(crate) struct SlotSpec {
    pub(crate) accept: &'static KindSet,
    pub(crate) declared: &'static KindSet,
    pub(crate) many: bool,
    pub(crate) separated: bool,
    pub(crate) required: bool,
}

/// The index of a node kind in a `KindSet`.
#[inline]
pub(crate) fn node_index(kind: NodeKind) -> usize {
    TokenKind::COUNT + kind as usize
}

/// The index of a child's kind in a `KindSet`.
#[inline]
fn kind_index(tree: &Tree<CSharp>, id: NodeId) -> usize {
    match tree.kind(id) {
        ElementType::Token(t) => t as usize,
        ElementType::Node(n) => node_index(n),
    }
}

/// The node type of a node kind: its index in `syntax::NODE_TYPES`.
#[inline]
pub(crate) fn node_type_index(kind: NodeKind) -> u8 {
    TYPE_OF_NODE[kind as usize]
}

/// The children of a node that are no trivia, each with the slot it fills: its index in its node type's
/// `syntax::NodeTypeInfo::slots` (`syntax::types::ClassDeclarationSyntax::IDENTIFIER`), or `usize::MAX` for a child no
/// slot after the previous one takes (none on a tree Roslyn's parser made, `tests/syntax_getters.rs`).
pub struct SlotChildren<'t> {
    tree: &'t Tree<CSharp>,
    specs: &'static [SlotSpec],
    children: Children<'t>,
    slot: usize,
}

impl<'t> SlotChildren<'t> {
    pub(crate) fn of_type(tree: &'t Tree<CSharp>, id: NodeId, ty: u8) -> SlotChildren<'t> {
        SlotChildren { tree, specs: SPECS[ty as usize], children: tree.children(id), slot: 0 }
    }
}

impl Iterator for SlotChildren<'_> {
    type Item = (usize, NodeId);

    fn next(&mut self) -> Option<(usize, NodeId)> {
        for child in self.children.by_ref() {
            let kind = kind_index(self.tree, child);
            if TRIVIA.contains(kind) {
                continue;
            }
            while let Some(spec) = self.specs.get(self.slot) {
                if spec.accept.contains(kind) {
                    // a token a separated list takes as a separator goes to a later slot that takes it when the next
                    // child fits after that slot: `>` after `<A` is the closing token, `;` before a missing `}` in
                    // `enum C { A ; }` a separator
                    if spec.separated
                        && self.tree.is_leaf(child)
                        && let Some(later) = self.reachable(self.slot + 1, kind)
                        && self.next_fits(child, later)
                    {
                        self.slot = later;
                        continue;
                    }
                    let slot = self.slot;
                    if !spec.many {
                        self.slot += 1;
                    }
                    return Some((slot, child));
                }
                self.slot += 1;
            }
            return Some((usize::MAX, child));
        }
        None
    }
}

impl SlotChildren<'_> {
    /// The first slot from `from` whose declared kinds hold `kind`, past the slots that may be empty and up to the
    /// first one that is never null.
    fn reachable(&self, from: usize, kind: usize) -> Option<usize> {
        for (slot, spec) in self.specs.iter().enumerate().skip(from) {
            if spec.declared.contains(kind) {
                return Some(slot);
            }
            if spec.required {
                return None;
            }
        }
        None
    }

    /// Whether the child after `child` that is no trivia, if any, has a slot after `child` takes slot `slot`.
    fn next_fits(&self, child: NodeId, slot: usize) -> bool {
        let mut next = self.tree.next_sibling(child);
        while let Some(n) = next {
            let kind = kind_index(self.tree, n);
            if !TRIVIA.contains(kind) {
                let from = if self.specs[slot].many { slot } else { slot + 1 };
                return self.reachable(from, kind).is_some();
            }
            next = self.tree.next_sibling(n);
        }
        true
    }
}

/// The slot children of a node of type `ty`, None for a leaf or a node of another type.
#[inline]
fn walk(tree: &Tree<CSharp>, id: NodeId, ty: u8) -> Option<SlotChildren<'_>> {
    let kind = tree.node_kind(id)?;
    (node_type_index(kind) == ty).then(|| SlotChildren::of_type(tree, id, ty))
}

/// The slot children of a node whose type is in `table` (`(type, slot)`), with the slot of its type.
#[inline]
fn walk_in<'t>(tree: &'t Tree<CSharp>, id: NodeId, table: &[(u8, u8)]) -> Option<(SlotChildren<'t>, u8)> {
    let ty = node_type_index(tree.node_kind(id)?);
    let &(_, slot) = table.iter().find(|&&(t, _)| t == ty)?;
    Some((SlotChildren::of_type(tree, id, ty), slot))
}

/// The child in token or node slot `slot`, read up to it.
fn first_in(walk: SlotChildren<'_>, slot: usize) -> Option<NodeId> {
    for (s, child) in walk {
        if s == slot {
            return Some(child);
        }
        if s > slot {
            return None;
        }
    }
    None
}

/// Token or node slot `slot` of a node of type `ty`.
pub(crate) fn read_field(tree: &Tree<CSharp>, id: NodeId, ty: u8, slot: usize) -> Option<NodeId> {
    first_in(walk(tree, id, ty)?, slot)
}

/// The token or node slot of a node of a type of `table`.
pub(crate) fn read_field_in(tree: &Tree<CSharp>, id: NodeId, table: &[(u8, u8)]) -> Option<NodeId> {
    let (walk, slot) = walk_in(tree, id, table)?;
    first_in(walk, slot as usize)
}

/// Token list `slot` of a node of type `ty`.
pub(crate) fn read_tokens(tree: &Tree<CSharp>, id: NodeId, ty: u8, slot: usize) -> Elements<'_> {
    Elements { walk: walk(tree, id, ty), slot, nodes_only: false }
}

/// The token list of a node of a type of `table`.
pub(crate) fn read_tokens_in<'t>(tree: &'t Tree<CSharp>, id: NodeId, table: &[(u8, u8)]) -> Elements<'t> {
    match walk_in(tree, id, table) {
        Some((walk, slot)) => Elements { walk: Some(walk), slot: slot as usize, nodes_only: false },
        None => Elements::empty(),
    }
}

/// List or separated list `slot` of a node of type `ty`: its nodes, without a separated list's separators.
pub(crate) fn read_nodes(tree: &Tree<CSharp>, id: NodeId, ty: u8, slot: usize) -> Elements<'_> {
    Elements { walk: walk(tree, id, ty), slot, nodes_only: true }
}

/// The list or separated list of a node of a type of `table`: its nodes.
pub(crate) fn read_nodes_in<'t>(tree: &'t Tree<CSharp>, id: NodeId, table: &[(u8, u8)]) -> Elements<'t> {
    match walk_in(tree, id, table) {
        Some((walk, slot)) => Elements { walk: Some(walk), slot: slot as usize, nodes_only: true },
        None => Elements::empty(),
    }
}

/// Every child in slot `slot` of a node of any type, in order: a separated list with its separators.
pub(crate) fn slot_children(tree: &Tree<CSharp>, id: NodeId, slot: usize) -> Elements<'_> {
    let walk = tree.node_kind(id).map(|kind| SlotChildren::of_type(tree, id, node_type_index(kind)));
    Elements { walk, slot, nodes_only: false }
}

/// The children of a list slot in order (a token list's tokens, a list's nodes, a separated list's elements), read
/// from the node's first child to the first child past the slot.
pub struct Elements<'t> {
    walk: Option<SlotChildren<'t>>,
    slot: usize,
    nodes_only: bool,
}

impl Elements<'_> {
    /// No children: the list of a node that has none, or of a node of a type without the field.
    pub fn empty() -> Self {
        Elements { walk: None, slot: 0, nodes_only: false }
    }
}

impl Iterator for Elements<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let walk = self.walk.as_mut()?;
        loop {
            let Some((slot, child)) = walk.next() else {
                self.walk = None;
                return None;
            };
            if slot < self.slot {
                continue;
            }
            if slot > self.slot {
                self.walk = None;
                return None;
            }
            if self.nodes_only && walk.tree.is_leaf(child) {
                continue;
            }
            return Some(child);
        }
    }
}
