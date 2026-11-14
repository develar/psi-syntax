//! Which slot of its node type each child of a node fills: the tree keeps it (`Tree::slot`), the index of the green
//! node's slot as the flatten met it, the same for each element of a list, for a token's leaf and none for its trivia
//! (`flatten`), so the slots are Roslyn's own. A getter reads the children up to its slot and no further; a list stops
//! at the first child past it. Checked against Roslyn's own properties on every node of the gate's inputs (`getters`,
//! `tools/csharp-oracle` `getters`).

use psi_builder::tree::Children;
use psi_builder::{ElementType, Tree};

use super::fields::{TRIVIA, TYPE_OF_NODE, WORDS};
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
/// `syntax::NodeTypeInfo::slots` (`syntax::types::ClassDeclarationSyntax::IDENTIFIER`), or `usize::MAX` for a child the
/// tree gives none (a token of `Parser::parse_tokens` under its compilation unit, no tree Roslyn's parser makes).
pub struct SlotChildren<'t> {
    tree: &'t Tree<CSharp>,
    children: Children<'t>,
}

impl<'t> SlotChildren<'t> {
    pub(crate) fn of(tree: &'t Tree<CSharp>, id: NodeId) -> SlotChildren<'t> {
        SlotChildren { tree, children: tree.children(id) }
    }
}

impl Iterator for SlotChildren<'_> {
    type Item = (usize, NodeId);

    fn next(&mut self) -> Option<(usize, NodeId)> {
        for child in self.children.by_ref() {
            if TRIVIA.contains(kind_index(self.tree, child)) {
                continue;
            }
            let slot = self.tree.slot(child);
            return Some((if slot == psi_builder::tree::NO_SLOT { usize::MAX } else { slot as usize }, child));
        }
        None
    }
}

/// The slot children of a node of type `ty`, None for a leaf or a node of another type.
#[inline]
fn walk(tree: &Tree<CSharp>, id: NodeId, ty: u8) -> Option<SlotChildren<'_>> {
    let kind = tree.node_kind(id)?;
    (node_type_index(kind) == ty).then(|| SlotChildren::of(tree, id))
}

/// The slot children of a node whose type is in `table` (`(type, slot)`), with the slot of its type.
#[inline]
fn walk_in<'t>(tree: &'t Tree<CSharp>, id: NodeId, table: &[(u8, u8)]) -> Option<(SlotChildren<'t>, u8)> {
    let ty = node_type_index(tree.node_kind(id)?);
    let &(_, slot) = table.iter().find(|&&(t, _)| t == ty)?;
    Some((SlotChildren::of(tree, id), slot))
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
    let walk = tree.node_kind(id).map(|_| SlotChildren::of(tree, id));
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
