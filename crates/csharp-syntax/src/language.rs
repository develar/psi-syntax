//! `CSharp`, the `psi_builder::TreeLanguage` of C# trees: its leaves are `TokenKind`s (tokens and unstructured trivia),
//! its nodes `NodeKind`s (structured trivia among them), node indices starting above the token kinds.

use psi_builder::{NodeType, TokenType, TreeLanguage};

use crate::syntax_kind::{NodeKind, TokenKind};
use crate::tree::FileKind;

/// The C# language of `psi_builder`'s tree. It implements only `TreeLanguage`: Roslyn's parser builds green nodes
/// bottom up, and the tree is filled in preorder by `flatten` through `psi_builder::TreeBuilder`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CSharp;

impl TokenType for TokenKind {
    const NODE_BASE: usize = TokenKind::COUNT;

    #[inline]
    fn index(self) -> u16 {
        self as u16
    }

    #[inline]
    fn from_index(index: usize) -> Option<Self> {
        TokenKind::from_index(index)
    }

    #[inline]
    fn debug_name(self) -> &'static str {
        TokenKind::debug_name(self)
    }
}

impl NodeType for NodeKind {
    #[inline]
    fn index(self) -> u16 {
        self as u16
    }

    #[inline]
    fn from_index(index: usize) -> Option<Self> {
        NodeKind::from_index(index)
    }

    #[inline]
    fn debug_name(self) -> &'static str {
        NodeKind::debug_name(self)
    }
}

impl TreeLanguage for CSharp {
    type Token = TokenKind;
    type Node = NodeKind;
    type FileKind = FileKind;
    /// Roslyn has no error node; a kind index that is no kind (never stored) reads as skipped tokens.
    const ERROR_ELEMENT: NodeKind = NodeKind::SkippedTokensTrivia;
}
