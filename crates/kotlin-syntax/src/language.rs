//! `Kotlin`, the `psi_builder::Language` of this crate: what the platform reads from `KotlinParserDefinition`
//! (whitespace and comment tokens, the file type), `KtNodeTypes`' left-bound types, the lazily parsed element types
//! (`BlockExpressionElementType`, `LambdaExpressionElementType`, `KDocTokens.KDOC`, `KDocTokens.MARKDOWN_LINK`), the
//! semantic layer's joined tokens, and how files and chameleons are lexed and parsed.

use psi_builder::Chameleon;

use crate::builder::{Binder, BuilderBuffers, PsiBuilderImpl, SemanticBuffers, SemanticWhitespaceAwarePsiBuilderImpl};
use crate::counters::{Counter, add};
use crate::lexer::{self, Lexed};
use crate::node_kind::NodeKind;
use crate::parsing::kotlin_parser;
use crate::tokens::{ElementType, TokenKind};
use crate::tree::{FileKind, LazyKind};

/// The Kotlin language (a marker type): `Tree`, `Parser` and the builder are `psi_builder`'s over it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Kotlin;

impl psi_builder::Language for Kotlin {
    type Token = TokenKind;
    type Node = NodeKind;
    type Binder = Binder;
    type Lazy = LazyKind;
    type FileKind = FileKind;
    type LayerBuffers = SemanticBuffers;

    /// `KtNodeTypes.KT_FILE` (printed as "KtFile: <name>").
    const FILE: NodeKind = NodeKind::KT_FILE;
    const ERROR_ELEMENT: NodeKind = NodeKind::ERROR_ELEMENT;

    /// `KtTokens.WHITESPACES` or `KtTokens.COMMENTS`.
    #[inline(always)]
    fn is_whitespace_or_comment(token: TokenKind) -> bool {
        token.is_whitespace_or_comment()
    }

    #[inline(always)]
    fn is_left_bound(node: NodeKind) -> bool {
        node.is_left_bound()
    }

    /// A doc comment (`KDocTokens.KDOC`) or a markdown link inside one.
    #[inline(always)]
    fn is_chameleon_token(token: TokenKind) -> bool {
        matches!(token, TokenKind::DOC_COMMENT | TokenKind::KDOC_MARKDOWN_LINK)
    }

    #[inline(always)]
    fn chameleon(element_type: ElementType) -> Option<LazyKind> {
        match element_type {
            ElementType::Token(TokenKind::DOC_COMMENT) => Some(LazyKind::KDoc),
            ElementType::Token(TokenKind::KDOC_MARKDOWN_LINK) => Some(LazyKind::KDocLink),
            ElementType::Node(NodeKind::BLOCK) => Some(LazyKind::Block),
            ElementType::Node(NodeKind::LAMBDA_EXPRESSION) => Some(LazyKind::Lambda),
            _ => None,
        }
    }

    #[inline(always)]
    fn is_body(kind: LazyKind) -> bool {
        matches!(kind, LazyKind::Block | LazyKind::Lambda)
    }

    #[inline]
    fn joined_token_type(builder: &PsiBuilderImpl, raw: Option<TokenKind>, raw_lookup_steps: i32) -> Option<TokenKind> {
        crate::builder::joined_token_type(builder, raw, raw_lookup_steps)
    }

    /// `(`, `[`, `{`, `${` and a string's opening quote open; their closers close.
    #[inline]
    fn bracket_depth_change(token: TokenKind) -> i64 {
        use TokenKind::*;
        match token {
            LPAR | LBRACKET | LBRACE | LONG_TEMPLATE_ENTRY_START | OPEN_QUOTE => 1,
            RPAR | RBRACKET | RBRACE | LONG_TEMPLATE_ENTRY_END | CLOSING_QUOTE => -1,
            _ => 0,
        }
    }

    #[inline]
    fn tokenize_into(text: &[u8], lexed: &mut Lexed) {
        lexer::tokenize_into(text, lexed);
    }

    /// `KotlinParser.parse(psiBuilder, psiFile)` on the semantic layer over `builder`.
    #[inline]
    fn parse_file(builder: PsiBuilderImpl, bufs: &mut BuilderBuffers, file_kind: FileKind) -> PsiBuilderImpl {
        let mut builder = SemanticWhitespaceAwarePsiBuilderImpl::new(builder, bufs);
        kotlin_parser::parse(&mut builder, file_kind);
        builder.into_delegate(bufs)
    }

    /// `parseContents` of the lazy element types: a block or lambda over its tokens (the file's when reusable) with
    /// `KotlinParser`, a doc comment lexed with `KDocLexer` and parsed with `KDocParser`, a markdown link lexed with
    /// `KotlinLexer` and parsed with `KDocLinkParser`.
    #[inline]
    fn parse_chameleon(kind: LazyKind, chameleon: &mut Chameleon<'_, Kotlin>) -> PsiBuilderImpl {
        match kind {
            LazyKind::Block | LazyKind::Lambda => {
                add(if kind == LazyKind::Block { Counter::BlockChameleons } else { Counter::LambdaChameleons }, 1);
                let delegate = chameleon.body_builder();
                let mut builder = SemanticWhitespaceAwarePsiBuilderImpl::new(delegate, chameleon.bufs);
                if kind == LazyKind::Block {
                    kotlin_parser::parse_block_expression(&mut builder);
                } else {
                    kotlin_parser::parse_lambda_expression(&mut builder);
                }
                builder.into_delegate(chameleon.bufs)
            }
            LazyKind::KDoc => {
                let text = chameleon.text();
                let bufs = &mut *chameleon.bufs;
                crate::kdoc::lexer::tokenize_into(text, &mut bufs.lex_types, &mut bufs.starts);
                add(Counter::KDocChameleons, 1);
                add(Counter::KDocTokens, chameleon.bufs.lex_types.len() as u64);
                let mut builder = chameleon.builder();
                crate::kdoc::parser::parse(ElementType::Token(TokenKind::DOC_COMMENT), &mut builder);
                builder
            }
            LazyKind::KDocLink => {
                add(Counter::KDocLinkChameleons, 1);
                let mut builder = chameleon.relexed_builder();
                crate::kdoc::parser::parse_markdown_link(
                    ElementType::Token(TokenKind::KDOC_MARKDOWN_LINK),
                    &mut builder,
                );
                builder
            }
        }
    }
}
