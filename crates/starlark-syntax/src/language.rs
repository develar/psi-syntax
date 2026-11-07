//! `psi-builder`'s language contract for Starlark.

use crate::lexer::{self, StarlarkLexed};
use crate::node_kind::NodeKind;
use crate::tokens::TokenKind;
use crate::tree::FileKind;
use psi_builder::builder::{PsiBuilderImpl, WhitespacesAndCommentsBinder};
use psi_builder::lexer::Lexed;
use psi_builder::{ElementTypeOf, Language, TreeLanguage};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Starlark;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Binder {
    DefaultLeft,
    DefaultRight,
}

impl WhitespacesAndCommentsBinder<TokenKind> for Binder {
    const DEFAULT_LEFT_BINDER: Self = Binder::DefaultLeft;
    const DEFAULT_RIGHT_BINDER: Self = Binder::DefaultRight;

    fn get_edge_position<'t>(
        self,
        tokens: &[TokenKind],
        _at_stream_edge: bool,
        _getter: &dyn Fn(usize) -> &'t [u8],
    ) -> usize {
        match self {
            Binder::DefaultLeft => tokens.len(),
            Binder::DefaultRight => 0,
        }
    }

    fn negative_size_edge<'t>(
        self,
        size: i32,
        _get: impl Fn(i32) -> Option<TokenKind>,
        _text: &dyn Fn(i32) -> &'t [u8],
    ) -> i32 {
        match self {
            Binder::DefaultLeft => size,
            Binder::DefaultRight => 0,
        }
    }
}

impl TreeLanguage for Starlark {
    type Token = TokenKind;
    type Node = NodeKind;
    type FileKind = FileKind;
    const ERROR_ELEMENT: NodeKind = NodeKind::ERROR_ELEMENT;

    /// `StarlarkTokenSets.WHITESPACE` and `COMMENT`, what the parser definition names as whitespace and comments: the
    /// PSI of the node is a `PsiWhiteSpace` or a `PsiComment`.
    #[inline(always)]
    fn is_trivia(tree: &psi_builder::Tree<Starlark>, id: psi_builder::NodeId) -> bool {
        tree.kind(id).token().is_some_and(|token| token.is_whitespace() || token.is_comment())
    }

    /// Starlark has no doc comment: a docstring is a string expression, code of its own.
    #[inline(always)]
    fn is_doc_comment(_tree: &psi_builder::Tree<Starlark>, _id: psi_builder::NodeId) -> bool {
        false
    }
}

impl Language for Starlark {
    type Binder = Binder;
    type Lazy = ();
    type Frame = PsiBuilderImpl<Starlark>;

    #[inline(always)]
    fn frame_builder(frame: &mut Self::Frame) -> &mut PsiBuilderImpl<Starlark> {
        frame
    }

    #[inline(always)]
    fn frame_builder_ref(frame: &Self::Frame) -> &PsiBuilderImpl<Starlark> {
        frame
    }

    const FILE: NodeKind = NodeKind::FILE;

    #[inline(always)]
    fn is_whitespace_or_comment(token: TokenKind) -> bool {
        token.is_whitespace() || token.is_comment()
    }

    #[inline(always)]
    fn is_left_bound(node: NodeKind) -> bool {
        node.is_left_bound()
    }

    #[inline(always)]
    fn is_chameleon_token(_token: TokenKind) -> bool {
        false
    }

    #[inline(always)]
    fn chameleon(_element_type: ElementTypeOf<Self>) -> Option<()> {
        None
    }

    #[inline(always)]
    fn is_body(_kind: ()) -> bool {
        false
    }

    #[inline(always)]
    fn bracket_depth_change(token: TokenKind) -> i64 {
        if crate::token_sets::OPEN_BRACKETS.contains(token) {
            1
        } else if crate::token_sets::CLOSE_BRACKETS.contains(token) {
            -1
        } else {
            0
        }
    }

    #[inline]
    fn tokenize_into(text: &[u8], _file_kind: FileKind, lexed: &mut Lexed<TokenKind>) {
        lexer::tokenize_into(text, lexed);
    }

    #[inline]
    fn parse_file(frame: &mut Self::Frame, file_kind: FileKind) {
        crate::parser::parse_file(Self::frame_builder(frame), file_kind);
    }

    fn parse_chameleon(_kind: (), _chameleon: &mut psi_builder::Chameleon<'_, Self>) {
        unreachable!("Starlark has no lazy elements")
    }
}

const _: fn(&[u8], &mut StarlarkLexed) = lexer::tokenize_into;
