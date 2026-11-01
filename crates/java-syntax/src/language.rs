//! `Java`, the `psi_builder::Language` of this crate: what the platform reads from `JavaSyntaxDefinition`
//! (whitespace and comment tokens, the file type), `JavaWhitespaceOrCommentBindingPolicy`'s left-bound types, the
//! lazily parsed element types (`CODE_BLOCK`, `DOC_COMMENT`, `DOC_MARKDOWN_COMMENT`, `DOC_REFERENCE_HOLDER`,
//! `DOC_TYPE_HOLDER`) and how files and chameleons are lexed and parsed (`JavaSyntaxDefinition.parse`, the
//! parser-partial entries, the `LazyParser`s of `element/lazyParser`).

use psi_builder::{Chameleon, Marker};

use crate::builder::{BuilderBuffers, PsiBuilderImpl, SyntaxTreeBuilder};
use crate::bundle::message;
use crate::doc_lexer;
use crate::language_level::LanguageLevel;
use crate::lexer::{self, Lexed};
use crate::node_kind::NodeKind;
use crate::parser::java_doc_parser::JavaDocParser;
use crate::parser::java_parser::JavaParser;
use crate::parser::reference_parser::ReferenceParser;
use crate::tokens::{ElementType, TokenKind};
use crate::tree::{Entry, FileKind, LazyKind};

/// The Java language (a marker type): `Tree`, `Parser` and the builder are `psi_builder`'s over it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Java;

impl psi_builder::Language for Java {
    type Token = TokenKind;
    type Node = NodeKind;
    type Binder = crate::builder::Binder;
    type Lazy = LazyKind;
    type FileKind = FileKind;
    type LayerBuffers = ();

    /// `JavaSyntaxElementType.JAVA_FILE` (printed as "PsiJavaFile:<name>").
    const FILE: NodeKind = NodeKind::JAVA_FILE;
    const ERROR_ELEMENT: NodeKind = NodeKind::ERROR_ELEMENT;

    /// `JavaSyntaxDefinition.whitespaces` + `.comments`, and DOC_COMMENT_LEADING_ASTERISKS, the comment token
    /// JavaDocParser enforces on a doc comment's builder (`enforceCommentTokens`) before it reads a token: it occurs
    /// in no other builder, and the Java comments occur in no doc comment's, so one set serves both.
    #[inline(always)]
    fn is_whitespace_or_comment(token: TokenKind) -> bool {
        token.is_whitespace_or_comment() || token == TokenKind::DOC_COMMENT_LEADING_ASTERISKS
    }

    #[inline(always)]
    fn is_left_bound(node: NodeKind) -> bool {
        node.is_left_bound()
    }

    /// The doc comments the Java lexer emits and the holders JavaDocParser remaps a token into.
    #[inline(always)]
    fn is_chameleon_token(token: TokenKind) -> bool {
        token.is_lazy()
    }

    #[inline(always)]
    fn chameleon(element_type: ElementType) -> Option<LazyKind> {
        match element_type {
            ElementType::Node(NodeKind::CODE_BLOCK) => Some(LazyKind::CodeBlock),
            ElementType::Token(TokenKind::DOC_COMMENT) => Some(LazyKind::DocComment),
            ElementType::Token(TokenKind::DOC_MARKDOWN_COMMENT) => Some(LazyKind::DocMarkdownComment),
            ElementType::Token(TokenKind::DOC_REFERENCE_HOLDER) => Some(LazyKind::DocReferenceHolder),
            ElementType::Token(TokenKind::DOC_TYPE_HOLDER) => Some(LazyKind::DocTypeHolder),
            _ => None,
        }
    }

    #[inline(always)]
    fn is_body(kind: LazyKind) -> bool {
        kind == LazyKind::CodeBlock
    }

    /// `(`, `[`, `{` and a template's begin open; their closers close.
    #[inline]
    fn bracket_depth_change(token: TokenKind) -> i64 {
        use TokenKind::*;
        match token {
            LPARENTH | LBRACKET | LBRACE | STRING_TEMPLATE_BEGIN | TEXT_BLOCK_TEMPLATE_BEGIN => 1,
            RPARENTH | RBRACKET | RBRACE | STRING_TEMPLATE_END | TEXT_BLOCK_TEMPLATE_END => -1,
            _ => 0,
        }
    }

    /// `JavaLexer(level)` at the file's level.
    #[inline]
    fn tokenize_into(text: &[u8], file_kind: FileKind, lexed: &mut Lexed) {
        lexer::tokenize_into(text, file_kind.level, lexed);
    }

    /// `JavaSyntaxDefinition.parse(level, builder)` for a file; for a parser entry, what
    /// `JavaParsingTestConfiguratorBase` runs: the entry under a root `test.java.file`, and an error "Unparsed
    /// tokens" over what it left.
    #[inline]
    fn parse_file(mut builder: PsiBuilderImpl, _bufs: &mut BuilderBuffers, file_kind: FileKind) -> PsiBuilderImpl {
        let mut b = SyntaxTreeBuilder::new(&mut builder);
        let parser = JavaParser::new(file_kind.level);
        if file_kind.entry == Entry::File {
            crate::parser::java_parser::parse(file_kind.level, &mut b);
        } else {
            parse_entry(&parser, file_kind.entry, &mut b);
        }
        builder
    }

    /// The `LazyParser`s: a code block over its tokens (the file's when reusable) with
    /// `StatementParser.parseCodeBlockDeep(builder, true)` at the file's level (`getLanguageLevel`: the IDE's provider
    /// gives the file's level); a doc comment lexed with `JavaDocLexer(HIGHEST)` and parsed with
    /// `JavaDocParser.parseDocCommentText()`; a reference or type holder lexed with `JavaLexer(JDK_1_3)` (the type's
    /// through `JavaTypeEscapeLexer`) and parsed with `parseJavadocReference` / `parseJavadocType`.
    #[inline]
    fn parse_chameleon(kind: LazyKind, chameleon: &mut Chameleon<'_, Java>) -> PsiBuilderImpl {
        let level = chameleon.file_kind().level;
        match kind {
            LazyKind::CodeBlock => {
                let mut builder = chameleon.body_builder();
                let mut b = SyntaxTreeBuilder::new(&mut builder);
                let java_parser = JavaParser::new(level);
                java_parser.statement_parser().parse_code_block_deep(&mut b, true);
                builder
            }
            LazyKind::DocComment | LazyKind::DocMarkdownComment => {
                let text = chameleon.text();
                let bufs = &mut *chameleon.bufs;
                doc_lexer::tokenize_into(
                    text,
                    LanguageLevel::HIGHEST.is_at_least(LanguageLevel::JDK_1_5),
                    &mut bufs.lex_types,
                    &mut bufs.starts,
                );
                let mut builder = chameleon.builder();
                let mut b = SyntaxTreeBuilder::new(&mut builder);
                let root_type = if kind == LazyKind::DocMarkdownComment {
                    TokenKind::DOC_MARKDOWN_COMMENT
                } else {
                    TokenKind::DOC_COMMENT
                };
                parse_fragment(&mut b, root_type.into(), false, |b| {
                    JavaDocParser::new(b, LanguageLevel::HIGHEST).parse_doc_comment_text();
                });
                builder
            }
            LazyKind::DocReferenceHolder | LazyKind::DocTypeHolder => {
                let level = LanguageLevel::JDK_1_3;
                let text = chameleon.text();
                let bufs = &mut *chameleon.bufs;
                let mut lexed = Lexed::default();
                if kind == LazyKind::DocTypeHolder {
                    lexer::tokenize_type_escaped_into(text, level, &mut lexed);
                } else {
                    lexer::tokenize_into(text, level, &mut lexed);
                }
                bufs.lex_types.clear();
                bufs.lex_types.extend_from_slice(&lexed.types);
                bufs.starts.clear();
                bufs.starts.extend_from_slice(&lexed.starts);
                let mut builder = chameleon.builder();
                let mut b = SyntaxTreeBuilder::new(&mut builder);
                let java_parser = JavaParser::new(level);
                if kind == LazyKind::DocTypeHolder {
                    parse_fragment(&mut b, TokenKind::DOC_TYPE_HOLDER.into(), false, |b| {
                        JavaDocParser::new(b, level).parse_javadoc_type(&java_parser);
                    });
                } else {
                    parse_fragment(&mut b, TokenKind::DOC_REFERENCE_HOLDER.into(), false, |b| {
                        JavaDocParser::new(b, level).parse_javadoc_reference(&java_parser);
                    });
                }
                builder
            }
        }
    }
}

/// `ParsingUtil.parseFragment(builder, type, eatAll, block)`: `block` under a root of `element_type`. Tokens it
/// leaves are an error "Unexpected tokens" with `eat_all`; without it the Kotlin code throws
/// `IncompleteFragmentParsingException`, which none of the lazy parsers reaches (each eats to the end), so the port
/// eats them the same way instead of failing.
pub fn parse_fragment(
    builder: &mut SyntaxTreeBuilder<'_>,
    element_type: ElementType,
    eat_all: bool,
    block: impl FnOnce(&mut SyntaxTreeBuilder<'_>),
) {
    let root = builder.mark();
    block(builder);
    if !builder.eof() {
        let _ = eat_all;
        let extras = builder.mark();
        while !builder.eof() {
            builder.advance_lexer();
        }
        extras.error(builder, message("unexpected.tokens"));
    }
    root.done(builder, element_type);
}

/// A parser-partial entry as `JavaParsingTestConfiguratorBase` runs it: `root = mark(); ENTRY; if (!eof) { error
/// "Unparsed tokens" over the rest }; root.done(test.java.file)`.
fn parse_entry(parser: &JavaParser, entry: Entry, builder: &mut SyntaxTreeBuilder<'_>) {
    let root: Marker = builder.mark();
    match entry {
        Entry::File => parser.file_parser().parse(builder),
        Entry::Annotations => {
            parser.declaration_parser().parse_annotations(builder);
        }
        Entry::ClassBody => parser.declaration_parser().parse_class_body_with_braces(builder, false, false),
        Entry::ClassBodyAnnotation => parser.declaration_parser().parse_class_body_with_braces(builder, true, false),
        Entry::ClassBodyEnum => parser.declaration_parser().parse_class_body_with_braces(builder, false, true),
        Entry::Expression => {
            parser.expression_parser().parse(builder);
        }
        Entry::FileParser => parser.file_parser().parse(builder),
        Entry::Pattern => {
            if parser.pattern_parser().is_pattern(builder) {
                parser.pattern_parser().parse_pattern(builder);
            }
        }
        Entry::Reference => {
            parser.reference_parser().parse_java_code_reference(builder, false, false, false, false);
        }
        Entry::ReferenceIncomplete => {
            parser.reference_parser().parse_java_code_reference(builder, true, false, false, false);
        }
        Entry::Type => {
            parser.reference_parser().parse_type(
                builder,
                ReferenceParser::ELLIPSIS | ReferenceParser::DIAMONDS | ReferenceParser::DISJUNCTIONS,
            );
        }
        Entry::TypeParams => {
            parser.reference_parser().parse_type_parameters(builder);
        }
        Entry::CodeBlockDeep => {
            parser.statement_parser().parse_code_block_deep(builder, true);
        }
        Entry::Statements => parser.statement_parser().parse_statements(builder),
    }
    if !builder.eof() {
        let unparsed = builder.mark();
        while !builder.eof() {
            builder.advance_lexer();
        }
        unparsed.error(builder, "Unparsed tokens");
    }
    root.done(builder, NodeKind::TEST_JAVA_FILE);
}
