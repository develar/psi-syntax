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

    /// A leaf of `SyntaxFacts.IsTrivia` (whitespace, a line end, a comment, a documentation comment's exterior
    /// `///`, disabled text, a preprocessing message, a conflict marker), or the root of a structured trivia
    /// (`SyntaxNode.IsStructuredTrivia`: a directive, a parsed documentation comment, the tokens the parser skipped),
    /// whose whole subtree is trivia of the token it hangs on.
    #[inline]
    fn is_trivia(tree: &psi_builder::Tree<CSharp>, id: psi_builder::NodeId) -> bool {
        tree.token_kind(id).is_some_and(crate::dump::is_trivia) || crate::read::is_structured_trivia(tree, id)
    }

    /// `SyntaxFacts.IsDocumentationCommentTrivia` of a parsed documentation comment, and a `///` or `/** */` comment
    /// that `DocumentationMode::None` left a comment (`read::is_documentation_comment`).
    #[inline]
    fn is_doc_comment(tree: &psi_builder::Tree<CSharp>, id: psi_builder::NodeId) -> bool {
        crate::read::is_documentation_comment(tree, id)
    }
}
