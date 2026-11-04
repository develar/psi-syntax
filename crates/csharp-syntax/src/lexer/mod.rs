//! Roslyn's lexer (`Parser/Lexer.cs` and its partial files), ported method by method in C# order, one Rust file per C#
//! file: `mod.rs` is `Lexer.cs`, `abstract_lexer.rs` `AbstractLexer.cs`, `string_literal.rs` `Lexer_StringLiteral.cs`,
//! `raw_string_literal.rs` `Lexer_RawStringLiteral.cs`, `interpolation.rs` `Lexer.Interpolation.cs`,
//! `sliding_text_window.rs` `SlidingTextWindow.cs`, `character_info.rs` `CharacterInfo.cs`, `directives.rs`
//! `Directives.cs`, `directive_parser.rs` `DirectiveParser.cs`, `quick_scanner.rs` `QuickScanner.cs`, `lexer_cache.rs`
//! the token map of `LexerCache.cs` (its trivia map and keyword cache are not ported: the keyword lookup is
//! `keyword_kind`, a perfect hash, and a whitespace trivia is a range of the source either way).
//!
//! The text is the file's bytes read as UTF-16 code units (`SlidingTextWindow`): positions and widths are bytes, a
//! supplementary character two code units of 2 bytes each. Tokens and trivia are made in the green arena, their texts
//! ranges of the source; a token's `ValueText` is set only where it is not its text (an identifier with an escape, a
//! formatting character or `@`; a string in a directive; an XML entity). The lexer's buffers (trivia lists, the
//! directive stacks' cells, the directive parser's token window) belong to the `Parser` and are reused per file.
//!
//! Recursion: the scanners Roslyn recurses in without a guard (a hole of an interpolated string inside another's, a
//! bracket inside a hole, a directive expression's operands) recurse here up to `MAX_NESTED_SCAN_DEPTH` levels (a
//! construct deeper is scanned as plain content, see each place); excluded `#if` sections run in a loop. So any input
//! lexes on a 1 MB stack.

mod abstract_lexer;
pub mod character_info;
mod directive_parser;
mod directives;
mod interpolation;
mod lexer_cache;
mod quick_scanner;
mod raw_string_literal;
pub mod sliding_text_window;
mod string_literal;

pub use directives::{DirectiveStack, DirectiveStore};
pub use interpolation::Interpolation;
pub(crate) use lexer_cache::{EOL_BIT, QuickToken};
pub use string_literal::InterpolatedStringKind;

use character_info as facts;
use directive_parser::DirectiveParser;
use sliding_text_window::{INVALID_CHARACTER, SlidingTextWindow};

use crate::error_code::{ErrorCode, MessageID, XmlParseErrorCode};
use crate::green::{DiagArg, DiagnosticId, Green, GreenArena, TokenText, Trivia, TriviaList};
use crate::language_version::LanguageVersion;
use crate::parser::doc_comment_parser::{DocumentationCommentParser, DocumentationCommentParserBuffers};
use crate::parser::interpolated_string::InterpolatedStringBuffers;
use crate::syntax_kind::SyntaxKind;
use crate::syntax_parser::SyntaxParserBuffers;
use crate::tree::DocumentationMode;
use crate::xml_char_type;

/// How deep the lexer's own recursive scanners go (nested holes and brackets of interpolated strings, directive
/// expressions) before a deeper construct is taken as plain content: Roslyn has no limit there (its stack has), real
/// code nests a few levels.
pub(crate) const MAX_NESTED_SCAN_DEPTH: u32 = 128;

/// `LexerMode` (Lexer.cs): the lexing mode in its low 16 bits, and the lexer-driven documentation comment location and
/// style above them.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct LexerMode(pub u32);

impl LexerMode {
    pub const NONE: LexerMode = LexerMode(0);
    pub const SYNTAX: LexerMode = LexerMode(0x0001);
    pub const DEBUGGER_SYNTAX: LexerMode = LexerMode(0x0002);
    pub const DIRECTIVE: LexerMode = LexerMode(0x0004);
    pub const XML_DOC_COMMENT: LexerMode = LexerMode(0x0008);
    pub const XML_ELEMENT_TAG: LexerMode = LexerMode(0x0010);
    pub const XML_ATTRIBUTE_TEXT_QUOTE: LexerMode = LexerMode(0x0020);
    pub const XML_ATTRIBUTE_TEXT_DOUBLE_QUOTE: LexerMode = LexerMode(0x0040);
    pub const XML_CREF_QUOTE: LexerMode = LexerMode(0x0080);
    pub const XML_CREF_DOUBLE_QUOTE: LexerMode = LexerMode(0x0100);
    pub const XML_NAME_QUOTE: LexerMode = LexerMode(0x0200);
    pub const XML_NAME_DOUBLE_QUOTE: LexerMode = LexerMode(0x0400);
    pub const XML_C_DATA_SECTION_TEXT: LexerMode = LexerMode(0x0800);
    pub const XML_COMMENT_TEXT: LexerMode = LexerMode(0x1000);
    pub const XML_PROCESSING_INSTRUCTION_TEXT: LexerMode = LexerMode(0x2000);
    pub const XML_CHARACTER: LexerMode = LexerMode(0x4000);
    pub const MASK_LEX_MODE: LexerMode = LexerMode(0xFFFF);
    pub const XML_DOC_COMMENT_LOCATION_START: LexerMode = LexerMode(0x00000);
    pub const XML_DOC_COMMENT_LOCATION_INTERIOR: LexerMode = LexerMode(0x10000);
    pub const XML_DOC_COMMENT_LOCATION_EXTERIOR: LexerMode = LexerMode(0x20000);
    pub const XML_DOC_COMMENT_LOCATION_END: LexerMode = LexerMode(0x40000);
    pub const MASK_XML_DOC_COMMENT_LOCATION: LexerMode = LexerMode(0xF0000);
    pub const XML_DOC_COMMENT_STYLE_SINGLE_LINE: LexerMode = LexerMode(0x000000);
    pub const XML_DOC_COMMENT_STYLE_DELIMITED: LexerMode = LexerMode(0x100000);
    pub const MASK_XML_DOC_COMMENT_STYLE: LexerMode = LexerMode(0x300000);

    /// `mode & other`.
    #[inline]
    pub fn and(self, other: LexerMode) -> LexerMode {
        LexerMode(self.0 & other.0)
    }

    /// `mode | other`.
    #[inline]
    pub fn or(self, other: LexerMode) -> LexerMode {
        LexerMode(self.0 | other.0)
    }
}

/// `XmlDocCommentLocation`: needs to match `LexerMode.XmlDocCommentLocation*`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum XmlDocCommentLocation {
    Start = 0,
    Interior = 1,
    Exterior = 2,
    End = 4,
}

/// `XmlDocCommentStyle`: needs to match `LexerMode.XmlDocCommentStyle*`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum XmlDocCommentStyle {
    SingleLine = 0,
    Delimited = 1,
}

/// What a lexer is configured with (`CSharpParseOptions` of `SourceCodeKind.Regular`): the language version, the
/// documentation mode and the preprocessor symbols defined outside the file (`PreprocessorSymbols`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct LexerOptions<'s> {
    pub version: LanguageVersion,
    pub doc_mode: DocumentationMode,
    /// `PreprocessorSymbols`: the symbols an `#if` reads as defined where the file's own `#define` and `#undef` say
    /// nothing of them (`DirectiveParser.IsDefined`).
    pub preprocessor_symbols: &'s PreprocessorSymbols,
}

impl Default for LexerOptions<'_> {
    fn default() -> Self {
        LexerOptions { version: LanguageVersion::default(), doc_mode: DocumentationMode::default(), preprocessor_symbols: PreprocessorSymbols::none() }
    }
}

/// `CSharpParseOptions.PreprocessorSymbols`: the names of the symbols a parse defines (`WithPreprocessorSymbols`), each
/// as the UTF-8 of its `string`; `Contains` compares them ordinally with an identifier's `ValueText`.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct PreprocessorSymbols {
    /// Sorted, without duplicates.
    names: Vec<Box<[u8]>>,
}

/// The empty set, `ImmutableArray<string>.Empty`: what `CSharpParseOptions.Default` defines.
static NO_SYMBOLS: PreprocessorSymbols = PreprocessorSymbols { names: Vec::new() };

impl PreprocessorSymbols {
    /// No symbol defined, `CSharpParseOptions.Default`'s.
    pub fn none() -> &'static PreprocessorSymbols {
        &NO_SYMBOLS
    }

    /// The set of `names`, in any order, duplicates once.
    pub fn new<N: AsRef<[u8]>>(names: impl IntoIterator<Item = N>) -> PreprocessorSymbols {
        let mut names: Vec<Box<[u8]>> = names.into_iter().map(|n| Box::from(n.as_ref())).collect();
        names.sort_unstable();
        names.dedup();
        PreprocessorSymbols { names }
    }

    /// `PreprocessorSymbols.Contains(id)`.
    pub fn contains(&self, id: &[u8]) -> bool {
        self.names.binary_search_by(|n| n.as_ref().cmp(id)).is_ok()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The names in byte order.
    pub fn names(&self) -> impl Iterator<Item = &[u8]> {
        self.names.iter().map(|n| n.as_ref())
    }
}

/// The lexer's buffers kept by a `Parser` from one file to the next.
#[derive(Default)]
pub struct LexerBuffers {
    /// `_leadingTriviaCache`, `_trailingTriviaCache`, `_directiveTriviaCache`.
    leading_trivia: Vec<Trivia>,
    trailing_trivia: Vec<Trivia>,
    directive_trivia: Vec<Trivia>,
    /// The leading trivia of an XML token (a fresh `SyntaxListBuilder` in Roslyn).
    xml_trivia: Vec<Trivia>,
    /// `_errors` and the copy `GetErrors()` makes of them.
    errors: Vec<DiagnosticId>,
    token_errors: Vec<DiagnosticId>,
    /// `_builder` and `_identBuffer`, in UTF-16 code units; the value of the token being scanned.
    builder: Vec<u16>,
    ident_buffer: Vec<u16>,
    value: Vec<u16>,
    /// A token's `ValueText` as UTF-8, before it goes to the arena.
    value_text: String,
    /// The cells of the directive stacks.
    directives: DirectiveStore,
    /// The token window of `_directiveParser`, the buffers of `_xmlParser`.
    directive_parser: SyntaxParserBuffers,
    xml_parser: DocumentationCommentParserBuffers,
    /// What the parser of an interpolated or raw string token over this lexer reuses: the interpolations, the
    /// buffers of the lexers and parsers it makes for the holes and texts.
    interpolated_string: InterpolatedStringBuffers,
    /// `_cache`: the token map of the quick scanner, kept warm from one file to the next.
    cache: lexer_cache::LexerCache,
}

impl LexerBuffers {
    /// The heap bytes of the buffers, by capacity: the trivia caches, errors, the scanners' code units, the directive
    /// store and the nested parsers' buffers (directives, documentation comments, interpolated strings).
    pub(crate) fn heap_bytes(&self) -> usize {
        use psi_builder::buffers::vec_bytes;
        vec_bytes(&self.leading_trivia)
            + vec_bytes(&self.trailing_trivia)
            + vec_bytes(&self.directive_trivia)
            + vec_bytes(&self.xml_trivia)
            + vec_bytes(&self.errors)
            + vec_bytes(&self.token_errors)
            + vec_bytes(&self.builder)
            + vec_bytes(&self.ident_buffer)
            + vec_bytes(&self.value)
            + self.value_text.capacity()
            + self.directives.heap_bytes()
            + self.directive_parser.heap_bytes()
            + self.xml_parser.heap_bytes()
            + self.interpolated_string.heap_bytes()
            + self.cache.heap_bytes()
    }

    /// Empties the buffers, each shrunk to its share of `shrink`.
    pub(crate) fn shrink(&mut self, shrink: psi_builder::buffers::Shrink) {
        shrink.vec(&mut self.leading_trivia);
        shrink.vec(&mut self.trailing_trivia);
        shrink.vec(&mut self.directive_trivia);
        shrink.vec(&mut self.xml_trivia);
        shrink.vec(&mut self.errors);
        shrink.vec(&mut self.token_errors);
        shrink.vec(&mut self.builder);
        shrink.vec(&mut self.ident_buffer);
        shrink.vec(&mut self.value);
        shrink.string(&mut self.value_text);
        self.directives.shrink(shrink);
        self.directive_parser.shrink(shrink);
        self.xml_parser.shrink(shrink);
        self.interpolated_string.shrink(shrink);
        self.cache.shrink(shrink);
    }
}

/// `Lexer.TokenInfo`: what the scanners found. The text is always the lexeme (`lexeme_start..position`); the
/// `StringValue`, when it is not the text, is in `Lexer::value` (`has_value`). The numeric and character values are
/// not kept: no tree depends on them (the directive parser reads its numbers from the text).
#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct TokenInfo {
    pub(crate) kind: SyntaxKind,
    pub(crate) contextual_kind: SyntaxKind,
    pub(crate) requires_text_for_xml_entity: bool,
    pub(crate) has_identifier_escape_sequence: bool,
    pub(crate) is_verbatim: bool,
    /// `StringValue` is not `Text`: it is `Lexer::value`.
    pub(crate) has_value: bool,
}

/// `SpecialType` of a numeric literal's value (`TokenInfo.ValueKind`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ValueKind {
    None,
    Single,
    Double,
    Decimal,
}

/// The first byte of a code unit for the `switch` statements (`0x80` for anything above ASCII, the end included).
#[inline(always)]
pub(crate) fn ascii(ch: u16) -> u8 {
    if ch < 0x80 { ch as u8 } else { 0x80 }
}

/// A code unit of an ASCII character.
pub(crate) const fn c(b: u8) -> u16 {
    b as u16
}

/// All conflict markers consist of the same character repeated seven times. If it is a <<<<<<< or >>>>>>> marker then
/// it is also followed by a space.
const CONFLICT_MARKER_LENGTH: u32 = 7;

/// `LexerCache.MaxKeywordLength`.
const MAX_KEYWORD_LENGTH: usize = 10;

/// What `ScanSyntaxToken`'s switch does after a case: done, `goto case '_'` (an identifier) or `goto default`.
enum Then {
    Done,
    Identifier,
    Default,
}

/// Roslyn's `Lexer` over a window of a file's bytes.
pub struct Lexer<'t> {
    /// `TextWindow`.
    pub(crate) text_window: SlidingTextWindow<'t>,
    /// `LexemeStartPosition`.
    pub(crate) lexeme_start_position: u32,
    /// `_errors` (empty is `null`).
    pub(crate) errors: Vec<DiagnosticId>,
    /// `_options`.
    options: LexerOptions<'t>,
    /// `_mode`.
    pub(crate) mode: LexerMode,
    /// `_builder`.
    pub(crate) builder: Vec<u16>,
    /// `_identBuffer` (its length is `_identLen`).
    ident_buffer: Vec<u16>,
    /// The `StringValue` of the token being scanned when it is not its text (`TokenInfo::has_value`).
    pub(crate) value: Vec<u16>,
    /// `_directives`.
    directives: DirectiveStack,
    /// `_allowPreprocessorDirectives`.
    allow_preprocessor_directives: bool,
    /// `_interpolationFollowedByColon`.
    interpolation_followed_by_colon: bool,
    /// `_badTokenCount`: cumulative count of bad tokens produced.
    bad_token_count: i32,
    pub(crate) arena: GreenArena,
    buffers: LexerBuffers,
    /// The nesting of the recursive scanners (`MAX_NESTED_SCAN_DEPTH`).
    pub(crate) scan_depth: u32,
    /// The run of `@`s `TryScanAtStringToken` measured last.
    at_run: std::ops::Range<u32>,
}

impl<'t> Lexer<'t> {
    /// `new Lexer(text, options)` over a whole file.
    pub fn new(text: &'t [u8], options: LexerOptions<'t>, arena: GreenArena, buffers: LexerBuffers) -> Lexer<'t> {
        let end = text.len() as u32;
        Lexer::with_window(text, 0, end, options, true, false, arena, buffers)
    }

    /// `new Lexer(SourceText.From(text[start..end]), options, allowPreprocessorDirectives, interpolationFollowedByColon)`
    /// with positions kept as offsets into `text`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_window(
        text: &'t [u8],
        start: u32,
        end: u32,
        options: LexerOptions<'t>,
        allow_preprocessor_directives: bool,
        interpolation_followed_by_colon: bool,
        arena: GreenArena,
        mut buffers: LexerBuffers,
    ) -> Lexer<'t> {
        let mut errors = std::mem::take(&mut buffers.errors);
        errors.clear();
        let mut builder = std::mem::take(&mut buffers.builder);
        builder.clear();
        let mut ident_buffer = std::mem::take(&mut buffers.ident_buffer);
        ident_buffer.clear();
        let mut value = std::mem::take(&mut buffers.value);
        value.clear();
        buffers.directives.clear();
        let text_window = SlidingTextWindow::new(text, start, end);
        Lexer {
            lexeme_start_position: text_window.position(),
            text_window,
            errors,
            options,
            mode: LexerMode::NONE,
            builder,
            ident_buffer,
            value,
            directives: DirectiveStack::EMPTY,
            allow_preprocessor_directives,
            interpolation_followed_by_colon,
            bad_token_count: 0,
            arena,
            buffers,
            scan_depth: 0,
            at_run: 0..0,
        }
    }

    /// The arena and the reusable buffers, back to the `Parser` (`Dispose()`).
    pub fn into_parts(self) -> (GreenArena, LexerBuffers) {
        let mut buffers = self.buffers;
        buffers.errors = self.errors;
        buffers.builder = self.builder;
        buffers.ident_buffer = self.ident_buffer;
        buffers.value = self.value;
        (self.arena, buffers)
    }

    /// The green arena tokens are made in.
    #[inline]
    pub fn arena(&mut self) -> &mut GreenArena {
        &mut self.arena
    }

    /// The green arena, read-only.
    #[inline]
    pub fn arena_ref(&self) -> &GreenArena {
        &self.arena
    }

    /// `TextWindow.Text`: the file's bytes (the window's offsets index them).
    #[inline]
    pub fn text(&self) -> &'t [u8] {
        self.text_window.text()
    }

    /// The start of the window (0 for a file's lexer).
    #[inline]
    pub fn window_start(&self) -> u32 {
        self.text_window.text_start()
    }

    /// The end of the window (`TextWindow.Text.Length` of the window's own text, as an offset into `text()`).
    #[inline]
    pub fn end(&self) -> u32 {
        self.text_window.text_end()
    }

    /// `TextWindow.Position`.
    #[inline]
    pub fn position(&self) -> u32 {
        self.text_window.position()
    }

    /// The buffers the parser of an interpolated or raw string token reuses (`parser::interpolated_string`).
    pub(crate) fn interpolated_string_buffers(&mut self) -> &mut InterpolatedStringBuffers {
        &mut self.buffers.interpolated_string
    }

    /// The cells of the directive stacks (`DirectiveStack`'s queries take them).
    pub fn directive_store(&self) -> &DirectiveStore {
        &self.buffers.directives
    }

    /// `SuppressDocumentationCommentParse`.
    pub fn suppress_documentation_comment_parse(&self) -> bool {
        self.options.doc_mode < DocumentationMode::Parse
    }

    /// `Options`.
    #[inline]
    pub fn options(&self) -> LexerOptions<'t> {
        self.options
    }

    /// `Directives`.
    #[inline]
    pub fn directives(&self) -> DirectiveStack {
        self.directives
    }

    /// `InterpolationFollowedByColon`: the lexer is for the contents of an interpolation that is followed by a colon
    /// that signals the start of the format string.
    pub fn interpolation_followed_by_colon(&self) -> bool {
        self.interpolation_followed_by_colon
    }

    /// `AllowPreprocessorDirectives`.
    pub fn allow_preprocessor_directives(&self) -> bool {
        self.allow_preprocessor_directives
    }

    /// `Reset(position, directives)`.
    pub fn reset(&mut self, position: u32, directives: DirectiveStack) {
        self.text_window.reset(position);
        self.directives = directives;
    }

    /// `ModeOf(mode)`.
    fn mode_of(mode: LexerMode) -> LexerMode {
        mode.and(LexerMode::MASK_LEX_MODE)
    }

    /// `ModeIs(mode)`.
    #[inline]
    pub(crate) fn mode_is(&self, mode: LexerMode) -> bool {
        Self::mode_of(self.mode) == mode
    }

    /// `LocationOf(mode)`.
    fn location_of(mode: LexerMode) -> u32 {
        mode.and(LexerMode::MASK_XML_DOC_COMMENT_LOCATION).0 >> 16
    }

    /// `LocationIs(location)`.
    pub fn location_is(&self, location: XmlDocCommentLocation) -> bool {
        Self::location_of(self.mode) == location as u32
    }

    /// `MutateLocation(location)`.
    fn mutate_location(&mut self, location: XmlDocCommentLocation) {
        self.mode = LexerMode(self.mode.0 & !LexerMode::MASK_XML_DOC_COMMENT_LOCATION.0);
        self.mode = self.mode.or(LexerMode((location as u32) << 16));
    }

    /// `StyleOf(mode)`.
    fn style_of(mode: LexerMode) -> u32 {
        mode.and(LexerMode::MASK_XML_DOC_COMMENT_STYLE).0 >> 20
    }

    /// `StyleIs(style)`.
    fn style_is(&self, style: XmlDocCommentStyle) -> bool {
        Self::style_of(self.mode) == style as u32
    }

    /// `InDocumentationComment`.
    fn in_documentation_comment(&self) -> bool {
        matches!(
            Self::mode_of(self.mode),
            LexerMode::XML_DOC_COMMENT
                | LexerMode::XML_ELEMENT_TAG
                | LexerMode::XML_ATTRIBUTE_TEXT_QUOTE
                | LexerMode::XML_ATTRIBUTE_TEXT_DOUBLE_QUOTE
                | LexerMode::XML_CREF_QUOTE
                | LexerMode::XML_CREF_DOUBLE_QUOTE
                | LexerMode::XML_NAME_QUOTE
                | LexerMode::XML_NAME_DOUBLE_QUOTE
                | LexerMode::XML_C_DATA_SECTION_TEXT
                | LexerMode::XML_COMMENT_TEXT
                | LexerMode::XML_PROCESSING_INSTRUCTION_TEXT
                | LexerMode::XML_CHARACTER
        )
    }

    /// `Lex(ref LexerMode)`: the next token; `mode` gets the lexer-driven bits back.
    pub fn lex_ref(&mut self, mode: &mut LexerMode) -> Green {
        let result = self.lex(*mode);
        *mode = self.mode;
        result
    }

    /// `Lex(LexerMode)`: the next token in `mode`, with its trivia.
    pub fn lex(&mut self, mode: LexerMode) -> Green {
        self.mode = mode;
        match self.mode {
            LexerMode::SYNTAX | LexerMode::DEBUGGER_SYNTAX => {
                return self.quick_scan_syntax_token().unwrap_or_else(|| self.lex_syntax_token());
            }
            LexerMode::DIRECTIVE => return self.lex_directive_token(),
            _ => {}
        }
        match Self::mode_of(self.mode) {
            LexerMode::XML_DOC_COMMENT => self.lex_xml_token(),
            LexerMode::XML_ELEMENT_TAG => self.lex_xml_element_tag_token(),
            LexerMode::XML_ATTRIBUTE_TEXT_QUOTE | LexerMode::XML_ATTRIBUTE_TEXT_DOUBLE_QUOTE => {
                self.lex_xml_attribute_text_token()
            }
            LexerMode::XML_C_DATA_SECTION_TEXT => self.lex_xml_c_data_section_text_token(),
            LexerMode::XML_COMMENT_TEXT => self.lex_xml_comment_text_token(),
            LexerMode::XML_PROCESSING_INSTRUCTION_TEXT => self.lex_xml_processing_instruction_text_token(),
            LexerMode::XML_CREF_QUOTE | LexerMode::XML_CREF_DOUBLE_QUOTE => self.lex_xml_cref_or_name_token(),
            // Same lexing as a cref attribute, just treat the identifiers a little differently.
            LexerMode::XML_NAME_QUOTE | LexerMode::XML_NAME_DOUBLE_QUOTE => self.lex_xml_cref_or_name_token(),
            LexerMode::XML_CHARACTER => self.lex_xml_character(),
            // `throw ExceptionUtilities.UnexpectedValue`: the parsers never ask it; an end of file, which ends any loop
            _ => {
                let position = self.text_window.position();
                self.arena.create_token(
                    TriviaList::EMPTY,
                    SyntaxKind::EndOfFileToken,
                    SyntaxKind::EndOfFileToken,
                    TokenText::Source { start: position, width: 0 },
                    TriviaList::EMPTY,
                )
            }
        }
    }

    /// `LexSyntaxToken()`.
    fn lex_syntax_token(&mut self) -> Green {
        let mut leading = std::mem::take(&mut self.buffers.leading_trivia);
        leading.clear();
        let is_following_token = self.text_window.position() > self.text_window.text_start();
        self.lex_syntax_trivia(is_following_token, false, &mut leading);

        let mut token_info = TokenInfo::default();

        self.start();
        let start = self.lexeme_start_position;
        self.scan_syntax_token(&mut token_info);
        let end = self.text_window.position();
        let mut errors = std::mem::take(&mut self.buffers.token_errors);
        self.get_errors(&mut errors);

        // Roslyn keeps the token's `StringValue` in its `TokenInfo`; here it is `self.value`, which a directive in the
        // trailing trivia (one that is not first on its line: `@a #line 1 "b"`) overwrites with its own tokens' values.
        let value = token_info.has_value.then(|| std::mem::take(&mut self.value));
        let mut trailing = std::mem::take(&mut self.buffers.trailing_trivia);
        trailing.clear();
        self.lex_syntax_trivia(true, true, &mut trailing);
        if let Some(value) = value {
            self.value = value;
        }

        let leading_list = self.arena.trivia_list(&leading);
        let trailing_list = self.arena.trivia_list(&trailing);
        let token = self.create(&token_info, start, end, leading_list, trailing_list, &errors);
        self.buffers.leading_trivia = leading;
        self.buffers.trailing_trivia = trailing;
        self.buffers.token_errors = errors;
        token
    }

    /// `LexSyntaxLeadingTrivia()`: the leading trivia at the position, as a list.
    pub fn lex_syntax_leading_trivia(&mut self) -> TriviaList {
        let mut leading = std::mem::take(&mut self.buffers.leading_trivia);
        leading.clear();
        let is_following_token = self.text_window.position() > self.text_window.text_start();
        self.lex_syntax_trivia(is_following_token, false, &mut leading);
        let list = self.arena.trivia_list(&leading);
        self.buffers.leading_trivia = leading;
        list
    }

    /// `LexSyntaxTrailingTrivia()`.
    pub fn lex_syntax_trailing_trivia(&mut self) -> TriviaList {
        let mut trailing = std::mem::take(&mut self.buffers.trailing_trivia);
        trailing.clear();
        self.lex_syntax_trivia(true, true, &mut trailing);
        let list = self.arena.trivia_list(&trailing);
        self.buffers.trailing_trivia = trailing;
        list
    }

    /// `Create(in info, leading, trailing, errors)`: the token of `start..end` (its text is the source's there).
    fn create(
        &mut self,
        info: &TokenInfo,
        start: u32,
        end: u32,
        leading: TriviaList,
        trailing: TriviaList,
        errors: &[DiagnosticId],
    ) -> Green {
        let text = TokenText::Source { start, width: end - start };
        let token = match info.kind {
            // `SyntaxFactory.BadToken`
            SyntaxKind::None if !info.requires_text_for_xml_entity => {
                self.arena.create_token(leading, SyntaxKind::BadToken, SyntaxKind::BadToken, text, trailing)
            }
            // `SyntaxFactory.Identifier(info.ContextualKind, ...)`
            SyntaxKind::IdentifierToken if !info.requires_text_for_xml_entity => {
                let token = self.arena.create_token(leading, info.kind, info.contextual_kind, text, trailing);
                self.set_value(token, info);
                token
            }
            // the literals, the XML texts, the punctuation and keywords: `SyntaxFactory.Token`, `Literal`, ...
            kind => {
                let token = self.arena.create_token(leading, kind, kind, text, trailing);
                self.set_value(token, info);
                token
            }
        };

        if !errors.is_empty()
            && (self.options.doc_mode >= DocumentationMode::Diagnose || !self.in_documentation_comment())
        {
            return self.arena.with_diagnostics(token, errors);
        }
        token
    }

    /// The token's `ValueText`, when the scanners made one that is not its text.
    fn set_value(&mut self, token: Green, info: &TokenInfo) {
        if !info.has_value {
            return;
        }
        let mut text = std::mem::take(&mut self.buffers.value_text);
        text.clear();
        text.extend(char::decode_utf16(self.value.iter().copied()).map(|r| r.unwrap_or('\u{FFFD}')));
        self.arena.set_token_value(token, &text);
        self.buffers.value_text = text;
    }

    /// Sets `info`'s `StringValue` to `units` (when it is not the text).
    pub(crate) fn set_string_value(&mut self, info: &mut TokenInfo, units: &[u16]) {
        self.value.clear();
        self.value.extend_from_slice(units);
        info.has_value = true;
    }

    /// `ScanSyntaxToken(ref info)`.
    fn scan_syntax_token(&mut self, info: &mut TokenInfo) {
        // Initialize for new token scan
        info.kind = SyntaxKind::None;
        info.contextual_kind = SyntaxKind::None;
        info.has_value = false;

        let mut is_escaped = false;
        let starting_position = self.text_window.position();

        // Start scanning the token
        let mut character = self.text_window.peek_char();
        macro_rules! one {
            ($kind:ident) => {{
                self.text_window.advance_char();
                info.kind = SyntaxKind::$kind;
                Then::Done
            }};
        }
        // `c` then `=`: the kind with `=`, else the one without
        macro_rules! with_equals {
            ($equals:ident, $plain:ident) => {{
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') { SyntaxKind::$equals } else { SyntaxKind::$plain };
                Then::Done
            }};
        }
        let then = match ascii(character) {
            b'"' | b'\'' => {
                self.scan_string_literal(info, false);
                Then::Done
            }
            b'/' => with_equals!(SlashEqualsToken, SlashToken),
            b'.' => {
                let mut done = false;
                if matches!(self.text_window.peek_char_at(1), 0x30..=0x39) {
                    let at_dot_position = self.text_window.position();
                    if at_dot_position > self.text_window.text_start() && at_dot_position == self.lexeme_start_position
                    {
                        // We have something like: .0
                        //
                        // This could be a fp number *except* the case where we have `..0` in that case, we want two
                        // dots followed by an integer (which will be treated as a range expression).
                        //
                        // Move back one space to see what's before this dot and adjust accordingly.
                        if self.text_window.previous_char() == c(b'.') {
                            // We have two dots in a row.  Treat the second dot as a dot, not the start of a number
                            // literal.
                            self.text_window.advance_char();
                            info.kind = SyntaxKind::DotToken;
                            done = true;
                        }
                        // Fall through naturally and scan the number out as a floating point number.
                    }
                }
                if !done && !self.scan_numeric_literal(info) {
                    self.text_window.advance_char();
                    info.kind = SyntaxKind::DotToken;
                }
                Then::Done
            }
            b',' => one!(CommaToken),
            b':' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b':') {
                    SyntaxKind::ColonColonToken
                } else {
                    SyntaxKind::ColonToken
                };
                Then::Done
            }
            b';' => one!(SemicolonToken),
            b'~' => one!(TildeToken),
            b'!' => with_equals!(ExclamationEqualsToken, ExclamationToken),
            b'=' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') {
                    SyntaxKind::EqualsEqualsToken
                } else if self.text_window.try_advance(b'>') {
                    SyntaxKind::EqualsGreaterThanToken
                } else {
                    SyntaxKind::EqualsToken
                };
                Then::Done
            }
            b'*' => with_equals!(AsteriskEqualsToken, AsteriskToken),
            b'(' => one!(OpenParenToken),
            b')' => one!(CloseParenToken),
            b'{' => one!(OpenBraceToken),
            b'}' => one!(CloseBraceToken),
            b'[' => one!(OpenBracketToken),
            b']' => one!(CloseBracketToken),
            b'?' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'?') {
                    if self.text_window.try_advance(b'=') {
                        SyntaxKind::QuestionQuestionEqualsToken
                    } else {
                        SyntaxKind::QuestionQuestionToken
                    }
                } else {
                    SyntaxKind::QuestionToken
                };
                Then::Done
            }
            b'+' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') {
                    SyntaxKind::PlusEqualsToken
                } else if self.text_window.try_advance(b'+') {
                    SyntaxKind::PlusPlusToken
                } else {
                    SyntaxKind::PlusToken
                };
                Then::Done
            }
            b'-' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') {
                    SyntaxKind::MinusEqualsToken
                } else if self.text_window.try_advance(b'-') {
                    SyntaxKind::MinusMinusToken
                } else if self.text_window.try_advance(b'>') {
                    SyntaxKind::MinusGreaterThanToken
                } else {
                    SyntaxKind::MinusToken
                };
                Then::Done
            }
            b'%' => with_equals!(PercentEqualsToken, PercentToken),
            b'&' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') {
                    SyntaxKind::AmpersandEqualsToken
                } else if self.text_window.try_advance(b'&') {
                    SyntaxKind::AmpersandAmpersandToken
                } else {
                    SyntaxKind::AmpersandToken
                };
                Then::Done
            }
            b'^' => with_equals!(CaretEqualsToken, CaretToken),
            b'|' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') {
                    SyntaxKind::BarEqualsToken
                } else if self.text_window.try_advance(b'|') {
                    SyntaxKind::BarBarToken
                } else {
                    SyntaxKind::BarToken
                };
                Then::Done
            }
            b'<' => {
                self.text_window.advance_char();
                info.kind = if self.text_window.try_advance(b'=') {
                    SyntaxKind::LessThanEqualsToken
                } else if self.text_window.try_advance(b'<') {
                    if self.text_window.try_advance(b'=') {
                        SyntaxKind::LessThanLessThanEqualsToken
                    } else {
                        SyntaxKind::LessThanLessThanToken
                    }
                } else {
                    SyntaxKind::LessThanToken
                };
                Then::Done
            }
            b'>' => with_equals!(GreaterThanEqualsToken, GreaterThanToken),
            b'@' => {
                if !self.try_scan_at_string_token(info) && !self.scan_identifier_or_keyword(info) {
                    debug_assert!(self.text_window.peek_char() == c(b'@'));
                    if self.text_window.peek_char_at(1) == c(b':') {
                        // Razor HTML transition. For best consumption by razor, we want to simply pretend it's a token
                        // and consume all the way to the end of the line.
                        info.kind = SyntaxKind::RazorContentToken;
                        let position = self.text_window.position();
                        self.add_error_at(position + 1, 1, ErrorCode::ERR_ExpectedVerbatimLiteral, &[]);
                        self.scan_to_end_of_line();
                    } else {
                        self.consume_at_sign_sequence();
                        self.add_error_code(ErrorCode::ERR_ExpectedVerbatimLiteral, &[]);
                    }
                }
                Then::Done
            }
            b'$' => {
                if self.try_scan_interpolated_string(info) {
                    Then::Done
                } else if self.mode_is(LexerMode::DEBUGGER_SYNTAX) {
                    Then::Identifier
                } else {
                    Then::Default
                }
            }
            // All the 'common' identifier characters are represented directly in these switch cases for optimal
            // perf.  Calling IsIdentifierChar() functions is relatively expensive.
            b'_' | b'a'..=b'z' | b'A'..=b'Z' => Then::Identifier,
            b'0'..=b'9' => {
                self.scan_numeric_literal(info);
                Then::Done
            }
            b'\\' => {
                // Could be unicode escape. Try that.
                is_escaped = true;
                let mut surrogate = INVALID_CHARACTER;
                character = self.peek_char_or_unicode_escape(&mut surrogate);
                if facts::is_identifier_start_character(character) { Then::Identifier } else { Then::Default }
            }
            _ if character == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                if self.directives.has_unfinished_if(&self.buffers.directives) {
                    self.add_error_code(ErrorCode::ERR_EndifDirectiveExpected, &[]);
                }
                if self.directives.has_unfinished_region(&self.buffers.directives) {
                    self.add_error_code(ErrorCode::ERR_EndRegionDirectiveExpected, &[]);
                }
                info.kind = SyntaxKind::EndOfFileToken;
                Then::Done
            }
            _ => Then::Default,
        };
        match then {
            Then::Done => {}
            Then::Identifier => {
                self.scan_identifier_or_keyword(info);
            }
            Then::Default => {
                if facts::is_identifier_start_character(character) {
                    self.scan_identifier_or_keyword(info);
                    return;
                }
                self.scan_bad_character(info, character, is_escaped, starting_position);
            }
        }
    }

    /// The `default:` of `ScanSyntaxToken` for a character that starts no token: a bad token of it (with a
    /// surrogate pair whole), or of the rest of the file after 200 of them.
    fn scan_bad_character(&mut self, info: &mut TokenInfo, character: u16, is_escaped: bool, starting_position: u32) {
        if is_escaped {
            let mut surrogate = INVALID_CHARACTER;
            let mut error = None;
            self.next_char_or_unicode_escape(&mut surrogate, &mut error);
            self.add_error(error);
        } else {
            self.text_window.advance_char();

            // If we ran into the start of a surrogate pair, then see if we have the whole pair.  If so, skip the pair
            // in its entirety so we can produce a good error message that treats the two as a single entity to tell
            // the user about.
            if is_high_surrogate(character) && is_low_surrogate(self.text_window.peek_char()) {
                self.text_window.advance_char();
            }
        }

        // If we get too many characters that we cannot make sense of, treat the entire rest of the file as a single
        // invalid character, so we can bail out of parsing early without producing an unbounded number of errors.
        let count = self.bad_token_count;
        self.bad_token_count += 1;
        if count > 200 {
            let end = self.text_window.text_end();
            self.text_window.reset(end);
        }
        let _ = starting_position;

        // if the original text wasn't already escaped, then escape it in the error message so that it's clear what the
        // issue is.  i.e. if the users source had the literal six characters in order like so:  `\` `u` `D` `C` `E`
        // `7`, then there's no need to escape that again when reporting the error. However, if the user's code has the
        // actual System.Char \uDCE7 char in it, then we want to print that out in escaped form so they have an actual
        // clue about what the character value is that we have a problem with.
        let message = if is_escaped {
            let text = self.get_lexeme_text();
            let (start, len) = self.arena.intern(&String::from_utf8_lossy(text));
            DiagArg::Owned { start, len }
        } else {
            let mut text = std::mem::take(&mut self.buffers.value_text);
            text.clear();
            self.format_literal_escaping(self.lexeme_start_position, self.text_window.position(), &mut text);
            let (start, len) = self.arena.intern(&text);
            self.buffers.value_text = text;
            DiagArg::Owned { start, len }
        };
        self.add_error_code(ErrorCode::ERR_UnexpectedCharacter, &[message]);
        info.kind = SyntaxKind::None;
    }

    /// `ObjectDisplay.FormatLiteral(text, ObjectDisplayOptions.EscapeNonPrintableCharacters)` of the code units
    /// `start..end`: the C# escapes for the control characters, `\uxxxx` for a lone surrogate and a character
    /// `NeedsEscaping` says, `\Uxxxxxxxx` for such a surrogate pair.
    fn format_literal_escaping(&self, start: u32, end: u32, out: &mut String) {
        use std::fmt::Write;
        let w = &self.text_window;
        let mut p = start;
        while p < end {
            let unit = w.char_at(p);
            let next = w.next_position(p);
            if is_high_surrogate(unit) || is_low_surrogate(unit) {
                let low = if is_high_surrogate(unit) && next < end { w.char_at(next) } else { 0 };
                if is_high_surrogate(unit) && is_low_surrogate(low) {
                    let (cp, _) = w.code_point_at(p);
                    let after = w.next_position(next);
                    if facts::needs_escaping(cp) {
                        let _ = write!(out, "\\U{cp:08x}");
                    } else if let Some(ch) = char::from_u32(cp) {
                        out.push(ch);
                    }
                    p = after;
                } else {
                    // an unpaired surrogate
                    let _ = write!(out, "\\u{unit:04x}");
                    p = next;
                }
                continue;
            }
            match unit {
                0x5C => out.push_str("\\\\"),
                0x00 => out.push_str("\\0"),
                0x07 => out.push_str("\\a"),
                0x08 => out.push_str("\\b"),
                0x0C => out.push_str("\\f"),
                0x0A => out.push_str("\\n"),
                0x0D => out.push_str("\\r"),
                0x09 => out.push_str("\\t"),
                0x0B => out.push_str("\\v"),
                _ if facts::needs_escaping(unit as u32) => {
                    let _ = write!(out, "\\u{unit:04x}");
                }
                _ => out.push(char::from_u32(unit as u32).unwrap_or('\u{FFFD}')),
            }
            p = next;
        }
    }

    /// `TryScanAtStringToken(ref info)`.
    pub(crate) fn try_scan_at_string_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(self.text_window.peek_char() == c(b'@'));

        // `while (TextWindow.PeekChar(index) == '@') index++;`: the `@`s are a byte each, so the character after them
        // is the one at their end (a scan forward, not `PeekChar(index)` from the start for every `@`)
        // (and a hole of `@`s asks at each of them: the run of the last one asked is kept)
        let position = self.text_window.position();
        let end = if self.at_run.start <= position && position < self.at_run.end {
            self.at_run.end
        } else {
            let text = self.text_window.text();
            let mut end = position;
            while end < self.text_window.text_end() && text[end as usize] == b'@' {
                end += 1;
            }
            self.at_run = position..end;
            end
        };
        let after = self.text_window.char_at(end);

        if after == c(b'"') {
            // @"
            self.scan_verbatim_string_literal(info);
            return true;
        } else if after == c(b'$') {
            // @$"
            self.scan_interpolated_string_literal(info);
            return true;
        }

        false
    }

    /// `TryScanInterpolatedString(ref info)`.
    pub(crate) fn try_scan_interpolated_string(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(self.text_window.peek_char() == c(b'$'));

        if matches!(ascii(self.text_window.peek_char_at(1)), b'$' | b'@' | b'"') {
            // $$ - definitely starts a raw interpolated string.
            // $@ - definitely starts an interpolated string.
            //
            // This will also match for $@@.  This is an error case when the user thinks they can mix verbatim and raw
            // interpolations together.  This will be properly handled in ScanInterpolatedStringLiteral
            self.scan_interpolated_string_literal(info);
            return true;
        }

        false
    }

    /// `CheckFeatureAvailability(feature)`.
    fn check_feature_availability(&mut self, feature: MessageID) {
        if let Some((code, args)) = feature_availability_diagnostic_info(feature, self.options.version) {
            self.add_error_code(code, &args);
        }
    }

    /// `ScanInteger()`.
    fn scan_integer(&mut self) -> bool {
        let start = self.text_window.position();
        while matches!(self.text_window.peek_char(), 0x30..=0x39) {
            self.text_window.advance_char();
        }

        start < self.text_window.position()
    }

    /// `ScanNumericLiteralSingleInteger(...)`: allows underscores in integers, except at beginning for decimal and end.
    fn scan_numeric_literal_single_integer(
        &mut self,
        underscore_in_wrong_place: &mut bool,
        used_underscore: &mut bool,
        first_char_was_underscore: &mut bool,
        is_hex: bool,
        is_binary: bool,
    ) {
        if self.text_window.peek_char() == c(b'_') {
            if is_hex || is_binary {
                *first_char_was_underscore = true;
            } else {
                *underscore_in_wrong_place = true;
            }
        }

        let mut last_char_was_underscore = false;
        loop {
            let ch = self.text_window.peek_char();
            if ch == c(b'_') {
                *used_underscore = true;
                last_char_was_underscore = true;
            } else if !(if is_hex {
                facts::is_hex_digit(ch)
            } else if is_binary {
                facts::is_binary_digit(ch)
            } else {
                facts::is_dec_digit(ch)
            }) {
                break;
            } else {
                self.builder.push(ch);
                last_char_was_underscore = false;
            }
            self.text_window.advance_char();
        }

        if last_char_was_underscore {
            *underscore_in_wrong_place = true;
        }
    }

    /// `ScanNumericLiteral(ref info)`.
    fn scan_numeric_literal(&mut self, info: &mut TokenInfo) -> bool {
        let start = self.text_window.position();
        let mut is_hex = false;
        let mut is_binary = false;
        let mut has_decimal = false;
        let mut has_exponent = false;
        let mut value_kind = ValueKind::None;
        self.builder.clear();
        let mut has_u_suffix = false;
        let mut has_l_suffix = false;
        let mut underscore_in_wrong_place = false;
        let mut used_underscore = false;
        let mut first_char_was_underscore = false;

        let ch = self.text_window.peek_char();
        if ch == c(b'0') {
            let ch = self.text_window.peek_char_at(1);
            if ch == c(b'x') || ch == c(b'X') {
                self.text_window.advance_char_n(2);
                is_hex = true;
            } else if ch == c(b'b') || ch == c(b'B') {
                self.check_feature_availability(MessageID::IDS_FeatureBinaryLiteral);
                self.text_window.advance_char_n(2);
                is_binary = true;
            }
        }

        if is_hex || is_binary {
            // It's OK if it has no digits after the '0x' -- we'll catch it in ScanNumericLiteral and give a proper error
            // then.
            self.scan_numeric_literal_single_integer(
                &mut underscore_in_wrong_place,
                &mut used_underscore,
                &mut first_char_was_underscore,
                is_hex,
                is_binary,
            );

            if matches!(ascii(self.text_window.peek_char()), b'L' | b'l') {
                self.text_window.advance_char();
                has_l_suffix = true;
                if matches!(ascii(self.text_window.peek_char()), b'u' | b'U') {
                    self.text_window.advance_char();
                    has_u_suffix = true;
                }
            } else if matches!(ascii(self.text_window.peek_char()), b'u' | b'U') {
                self.text_window.advance_char();
                has_u_suffix = true;
                if matches!(ascii(self.text_window.peek_char()), b'L' | b'l') {
                    self.text_window.advance_char();
                    has_l_suffix = true;
                }
            }
        } else {
            self.scan_numeric_literal_single_integer(
                &mut underscore_in_wrong_place,
                &mut used_underscore,
                &mut first_char_was_underscore,
                false,
                false,
            );

            if self.mode_is(LexerMode::DEBUGGER_SYNTAX) && self.text_window.peek_char() == c(b'#') {
                // Previously, in DebuggerSyntax mode, "123#" was a valid identifier.
                self.text_window.advance_char();
                info.kind = SyntaxKind::IdentifierToken;
                info.contextual_kind = SyntaxKind::IdentifierToken;
                let error = self.make_error_code(ErrorCode::ERR_LegacyObjectIdSyntax, &[]);
                self.add_error(Some(error));
                return true;
            }

            let ch = self.text_window.peek_char();
            if ch == c(b'.') {
                let ch2 = self.text_window.peek_char_at(1);
                if (c(b'0')..=c(b'9')).contains(&ch2) {
                    has_decimal = true;
                    self.builder.push(ch);
                    self.text_window.advance_char();

                    self.scan_numeric_literal_single_integer(
                        &mut underscore_in_wrong_place,
                        &mut used_underscore,
                        &mut first_char_was_underscore,
                        false,
                        false,
                    );
                } else if self.builder.is_empty() {
                    // we only have the dot so far.. (no preceding number or following number)
                    self.text_window.reset(start);
                    return false;
                }
            }

            let ch = self.text_window.peek_char();
            if matches!(ascii(ch), b'E' | b'e') {
                self.builder.push(ch);
                self.text_window.advance_char();
                has_exponent = true;
                let ch = self.text_window.peek_char();
                if matches!(ascii(ch), b'-' | b'+') {
                    self.builder.push(ch);
                    self.text_window.advance_char();
                }

                let ch = self.text_window.peek_char();
                if !((c(b'0')..=c(b'9')).contains(&ch) || ch == c(b'_')) {
                    // use this for now (CS0595), cant use CS0594 as we dont know 'type'
                    let error = self.make_error_code(ErrorCode::ERR_InvalidReal, &[]);
                    self.add_error(Some(error));
                    // add dummy exponent, so parser does not blow up
                    self.builder.push(c(b'0'));
                } else {
                    self.scan_numeric_literal_single_integer(
                        &mut underscore_in_wrong_place,
                        &mut used_underscore,
                        &mut first_char_was_underscore,
                        false,
                        false,
                    );
                }
            }

            let ch = ascii(self.text_window.peek_char());
            if has_exponent || has_decimal {
                if matches!(ch, b'f' | b'F') {
                    self.text_window.advance_char();
                    value_kind = ValueKind::Single;
                } else if matches!(ch, b'D' | b'd') {
                    self.text_window.advance_char();
                    value_kind = ValueKind::Double;
                } else if matches!(ch, b'm' | b'M') {
                    self.text_window.advance_char();
                    value_kind = ValueKind::Decimal;
                } else {
                    value_kind = ValueKind::Double;
                }
            } else if matches!(ch, b'f' | b'F') {
                self.text_window.advance_char();
                value_kind = ValueKind::Single;
            } else if matches!(ch, b'D' | b'd') {
                self.text_window.advance_char();
                value_kind = ValueKind::Double;
            } else if matches!(ch, b'm' | b'M') {
                self.text_window.advance_char();
                value_kind = ValueKind::Decimal;
            } else if matches!(ch, b'L' | b'l') {
                self.text_window.advance_char();
                has_l_suffix = true;
                if matches!(ascii(self.text_window.peek_char()), b'u' | b'U') {
                    self.text_window.advance_char();
                    has_u_suffix = true;
                }
            } else if matches!(ch, b'u' | b'U') {
                has_u_suffix = true;
                self.text_window.advance_char();
                if matches!(ascii(self.text_window.peek_char()), b'L' | b'l') {
                    self.text_window.advance_char();
                    has_l_suffix = true;
                }
            }
        }

        if underscore_in_wrong_place {
            let position = self.text_window.position();
            let error = self.make_error_at(start, position - start, ErrorCode::ERR_InvalidNumber, &[]);
            self.add_error(Some(error));
        } else if first_char_was_underscore {
            self.check_feature_availability(MessageID::IDS_FeatureLeadingDigitSeparator);
        } else if used_underscore {
            self.check_feature_availability(MessageID::IDS_FeatureDigitSeparator);
        }

        info.kind = SyntaxKind::NumericLiteralToken;
        match value_kind {
            ValueKind::Single => self.get_value_single(),
            ValueKind::Double => self.get_value_double(),
            ValueKind::Decimal => {
                let position = self.text_window.position();
                self.get_value_decimal(start, position)
            }
            ValueKind::None => {
                if self.builder.is_empty() {
                    if !underscore_in_wrong_place {
                        let error = self.make_error_code(ErrorCode::ERR_InvalidNumber, &[]);
                        self.add_error(Some(error));
                    }
                } else {
                    self.get_value_u_int64(is_hex, is_binary);
                }
                // The type of an integer literal (int, uint, long, ulong by its suffixes and value) is the value's,
                // which the tree does not keep.
                let _ = (has_u_suffix, has_l_suffix);
            }
        }

        true
    }

    /// `GetValueUInt64(text, isHex, isBinary)` of `_builder`: the overflow error (the value itself is not kept).
    fn get_value_u_int64(&mut self, is_hex: bool, is_binary: bool) {
        let ok = if is_binary {
            // TryParseBinaryUInt64
            let mut value: u64 = 0;
            let mut ok = true;
            for &c in &self.builder {
                // if uppermost bit is set, then the next bitshift will overflow
                if value & 0x8000000000000000 != 0 {
                    ok = false;
                    break;
                }
                value = (value << 1) | facts::binary_value(c) as u64;
            }
            ok
        } else {
            let radix = if is_hex { 16 } else { 10 };
            let mut value: u64 = 0;
            self.builder.iter().all(|&c| {
                let digit = if is_hex { facts::hex_value(c) } else { facts::dec_value(c) } as u64;
                match value.checked_mul(radix).and_then(|v| v.checked_add(digit)) {
                    Some(v) => {
                        value = v;
                        true
                    }
                    None => false,
                }
            })
        };
        if !ok {
            //we've already lexed the literal, so the error must be from overflow
            let error = self.make_error_code(ErrorCode::ERR_IntOverflow, &[]);
            self.add_error(Some(error));
        }
    }

    /// `GetValueDouble(text)`: `RealParser.TryParseDouble` fails on overflow.
    fn get_value_double(&mut self) {
        if real_overflows(&self.builder, false) {
            //we've already lexed the literal, so the error must be from overflow
            let error = self.make_error_code(ErrorCode::ERR_FloatOverflow, &[DiagArg::Str("double")]);
            self.add_error(Some(error));
        }
    }

    /// `GetValueSingle(text)`.
    fn get_value_single(&mut self) {
        if real_overflows(&self.builder, true) {
            //we've already lexed the literal, so the error must be from overflow
            let error = self.make_error_code(ErrorCode::ERR_FloatOverflow, &[DiagArg::Str("float")]);
            self.add_error(Some(error));
        }
    }

    /// `GetValueDecimal(text, start, end)`: `decimal.TryParse` fails when the value, rounded to the decimal's 29
    /// significant digits, is past `decimal.MaxValue` (79228162514264337593543950335).
    fn get_value_decimal(&mut self, start: u32, end: u32) {
        if decimal_overflows(&self.builder) {
            //we've already lexed the literal, so the error must be from overflow
            let error =
                self.make_error_at(start, end - start, ErrorCode::ERR_FloatOverflow, &[DiagArg::Str("decimal")]);
            self.add_error(Some(error));
        }
    }

    /// `ResetIdentBuffer()`.
    fn reset_ident_buffer(&mut self) {
        self.ident_buffer.clear();
    }

    /// `AddIdentChar(ch)`.
    fn add_ident_char(&mut self, ch: u16) {
        self.ident_buffer.push(ch);
    }

    /// `ScanIdentifier(ref info)`.
    fn scan_identifier(&mut self, info: &mut TokenInfo) -> bool {
        self.scan_identifier_fast_path(info)
            || if self.in_xml_cref_or_name_attribute_value() {
                self.scan_identifier_cref_slow_path(info)
            } else {
                self.scan_identifier_slow_path(info)
            }
    }

    /// `ScanIdentifier_FastPath(ref info)`: a faster identifier lexer for the common case in the language where
    /// identifiers are not verbatim, don't contain unicode characters and don't contain unicode escapes: a run of
    /// `[_a-zA-Z0-9]` ended by one of a small set of known characters (Roslyn's also by the end of its character
    /// window, where the slow path then makes the same token; here the end of the text ends it).
    #[inline]
    fn scan_identifier_fast_path(&mut self, info: &mut TokenInfo) -> bool {
        if self.mode.and(LexerMode::MASK_LEX_MODE) == LexerMode::DEBUGGER_SYNTAX {
            // Debugger syntax is wonky.  Can't use the fast path for it.
            return false;
        }

        let text = self.text_window.text();
        let start = self.text_window.position() as usize;
        let end = self.text_window.text_end() as usize;
        let mut current = start;
        loop {
            if current == end {
                break;
            }
            match text[current] {
                b'&' => {
                    // If we're in a cref, this could be the start of an xml entity that belongs in the identifier.
                    if self.in_xml_cref_or_name_attribute_value() {
                        // Fall back on the slow path.
                        return false;
                    }
                    // Otherwise, end the identifier.
                    break;
                }
                // All of the following characters are not valid in an identifier.  If we see any of them, then we
                // know we're done.
                b'\0' | b' ' | b'\r' | b'\n' | b'\t' | b'!' | b'%' | b'(' | b')' | b'*' | b'+' | b',' | b'-' | b'.'
                | b'/' | b':' | b';' | b'<' | b'=' | b'>' | b'?' | b'[' | b']' | b'^' | b'{' | b'|' | b'}' | b'~'
                | b'"' | b'\'' => break,
                b'0'..=b'9' => {
                    if current == start {
                        return false;
                    }
                    current += 1;
                }
                // All of these characters are valid inside an identifier. consume it and keep processing.
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => current += 1,
                // case '@':  verbatim identifiers are handled in the slow path
                // case '\\': unicode escapes are handled in the slow path
                // Any other character is something we cannot handle.  i.e. unicode chars or an escape.  Just break out
                // and move to the slow path.
                _ => return false,
            }
        }
        if current == start {
            return false;
        }
        self.text_window.reset(current as u32);
        info.has_value = false;
        info.is_verbatim = false;
        true
    }

    /// `ScanIdentifier_SlowPath(ref info)`.
    fn scan_identifier_slow_path(&mut self, info: &mut TokenInfo) -> bool {
        let start = self.text_window.position();
        self.reset_ident_buffer();

        while self.text_window.peek_char() == c(b'@') {
            self.text_window.advance_char();
        }

        let at_count = self.text_window.position() - start;
        info.is_verbatim = at_count > 0;

        let mut is_object_address = false;
        // what the switch does with a character: take it into the identifier, skip it (a formatting character, `<>`)
        // or end the identifier (`goto LoopExit`)
        enum Step {
            Take,
            Skip,
            Exit,
        }
        loop {
            let mut surrogate_character = INVALID_CHARACTER;
            let mut is_escaped = false;
            let mut ch = self.text_window.peek_char();
            let step = loop {
                // top:
                let ident_len = self.ident_buffer.len();
                let default = |this: &mut Self, ch: u16, is_escaped: bool, surrogate: &mut u16| -> Step {
                    // This is the 'expensive' call
                    if ident_len == 0 && ch > 127 && facts::is_identifier_start_character(ch) {
                        Step::Take
                    } else if ident_len > 0 && ch > 127 && facts::is_identifier_part_character(ch) {
                        //// BUG 424819 : Handle identifier chars > 0xFFFF via surrogate pairs
                        if facts::is_formatting_char(ch) {
                            if is_escaped {
                                let mut error = None;
                                this.next_char_or_unicode_escape(surrogate, &mut error);
                                this.add_error(error);
                            } else {
                                this.text_window.advance_char();
                            }
                            return Step::Skip; // Ignore formatting characters
                        }
                        Step::Take
                    } else {
                        // Not a valid identifier character, so bail.
                        Step::Exit
                    }
                };
                break match ascii(ch) {
                    b'\\' => {
                        if !is_escaped && self.is_unicode_escape() {
                            // ^^^^^^^ otherwise \u1234 looks just like ሴ! (i.e. escape within escape)
                            info.has_identifier_escape_sequence = true;
                            is_escaped = true;
                            ch = self.peek_unicode_escape(&mut surrogate_character);
                            continue;
                        }
                        default(self, ch, is_escaped, &mut surrogate_character)
                    }
                    b'$' => {
                        if !self.mode_is(LexerMode::DEBUGGER_SYNTAX) || ident_len > 0 {
                            Step::Exit
                        } else {
                            Step::Take
                        }
                    }
                    // Again, these are the 'common' identifier characters...
                    b'_' | b'a'..=b'z' | b'A'..=b'Z' => Step::Take,
                    b'0' => {
                        if ident_len == 0 {
                            // Debugger syntax allows @0x[hexdigit]+ for object address identifiers.
                            if info.is_verbatim
                                && self.mode_is(LexerMode::DEBUGGER_SYNTAX)
                                && matches!(ascii(self.text_window.peek_char_at(1)), b'x' | b'X')
                            {
                                is_object_address = true;
                                Step::Take
                            } else {
                                Step::Exit
                            }
                        } else {
                            Step::Take
                        }
                    }
                    b'1'..=b'9' => {
                        if ident_len == 0 {
                            Step::Exit
                        } else {
                            Step::Take
                        }
                    }
                    // ...and these are the 'common' stop characters.
                    b' ' | b'\t' | b'.' | b';' | b'(' | b')' | b',' => Step::Exit,
                    b'<' => {
                        if ident_len == 0
                            && self.mode_is(LexerMode::DEBUGGER_SYNTAX)
                            && self.text_window.peek_char_at(1) == c(b'>')
                        {
                            // In DebuggerSyntax mode, identifiers are allowed to begin with <>.
                            self.text_window.advance_char_n(2);
                            self.add_ident_char(c(b'<'));
                            self.add_ident_char(c(b'>'));
                            Step::Skip
                        } else {
                            Step::Exit
                        }
                    }
                    _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => Step::Exit,
                    _ => default(self, ch, is_escaped, &mut surrogate_character),
                };
            };
            match step {
                Step::Exit => break,
                Step::Skip => continue,
                Step::Take => {}
            }

            if is_escaped {
                let mut error = None;
                self.next_char_or_unicode_escape(&mut surrogate_character, &mut error);
                self.add_error(error);
            } else {
                self.text_window.advance_char();
            }

            self.add_ident_char(ch);
            if surrogate_character != INVALID_CHARACTER {
                self.add_ident_char(surrogate_character);
            }
        }

        // LoopExit:
        let width = self.current_lexeme_width(); // exact size of input characters
        if !self.ident_buffer.is_empty() {
            // id buffer is identical to width in input
            if utf8_len(&self.ident_buffer) == width {
                info.has_value = false;
            } else {
                self.value.clear();
                self.value.extend_from_slice(&self.ident_buffer);
                info.has_value = true;
            }

            let mut fail = false;
            if is_object_address {
                // @0x[hexdigit]+
                const OBJECT_ADDRESS_OFFSET: usize = 2;
                let digits = &self.ident_buffer[OBJECT_ADDRESS_OFFSET.min(self.ident_buffer.len())..];
                // Verify valid hex value.
                if digits.is_empty() || !digits.iter().all(|&d| facts::is_hex_digit(d)) {
                    fail = true;
                } else {
                    // Parse hex value to check for overflow.
                    self.builder.clear();
                    let digits = digits.to_vec();
                    self.builder.extend_from_slice(&digits);
                    self.get_value_u_int64(true, false);
                }
            }

            if !fail {
                if at_count >= 2 {
                    self.add_error_at(start, at_count, ErrorCode::ERR_IllegalAtSequence, &[]);
                }
                return true;
            }
        }

        // Fail:
        info.has_value = false;
        self.text_window.reset(start);
        false
    }

    /// `ScanIdentifier_CrefSlowPath(ref info)`: `ScanIdentifier_SlowPath` that also takes XML entities.
    fn scan_identifier_cref_slow_path(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(self.in_xml_cref_or_name_attribute_value());

        let start = self.text_window.position();
        self.reset_ident_buffer();

        if self.advance_if_matches(c(b'@')) {
            // In xml name attribute values, the '@' is part of the value text of the identifier (to match dev11).
            if self.in_xml_name_attribute_value() {
                self.add_ident_char(c(b'@'));
            } else {
                info.is_verbatim = true;
            }
        }

        loop {
            let before_consumed = self.text_window.position();
            let mut consumed_char;
            let mut consumed_surrogate;

            if self.text_window.peek_char() == c(b'&') {
                consumed_char = 0;
                consumed_surrogate = 0;
                if !self.try_scan_xml_entity(&mut consumed_char, &mut consumed_surrogate) {
                    // If it's not a valid entity, then it's not part of the identifier.
                    self.text_window.reset(before_consumed);
                    break;
                }
            } else {
                consumed_char = self.text_window.next_char();
                consumed_surrogate = INVALID_CHARACTER;
            }

            // NOTE: If the surrogate is non-zero, then consumedChar won't match any of the cases below (UTF-16
            // guarantees that members of surrogate pairs aren't separately valid).

            let mut is_escaped = false;
            // what to do with the consumed character: take it, ignore it, or end the identifier before it
            let take = loop {
                // top:
                let ident_len = self.ident_buffer.len();
                break match ascii(consumed_char) {
                    b'\\' => {
                        // NOTE: For completeness, we should allow xml entities in unicode escape sequences (DevDiv
                        // #16321).  Since it is not currently a priority, we will try to make the interim behavior
                        // sensible: we will only attempt to scan a unicode escape if NONE of the characters are XML
                        // entities (including the backslash, which we have already consumed).
                        if !is_escaped
                            && self.text_window.position() == before_consumed + 1
                            && matches!(ascii(self.text_window.peek_char()), b'u' | b'U')
                        {
                            info.has_identifier_escape_sequence = true;

                            self.text_window.reset(before_consumed);
                            // ^^^^^^^ otherwise \u1234 looks just like ሴ! (i.e. escape within escape)
                            is_escaped = true;
                            let mut error = None;
                            consumed_char = self.next_unicode_escape(&mut consumed_surrogate, &mut error);
                            self.add_cref_error_info(error);
                            continue;
                        }
                        self.cref_default(consumed_char, ident_len, before_consumed)
                    }
                    // Again, these are the 'common' identifier characters...
                    b'_' | b'a'..=b'z' | b'A'..=b'Z' => Some(true),
                    b'0'..=b'9' => {
                        if ident_len == 0 {
                            self.text_window.reset(before_consumed);
                            None
                        } else {
                            Some(true)
                        }
                    }
                    // ...and these are the 'common' stop characters.
                    b' ' | b'$' | b'\t' | b'.' | b';' | b'(' | b')' | b',' | b'<' => {
                        self.text_window.reset(before_consumed);
                        None
                    }
                    _ if consumed_char == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                        self.text_window.reset(before_consumed);
                        None
                    }
                    _ => self.cref_default(consumed_char, ident_len, before_consumed),
                };
            };
            match take {
                None => break,
                Some(false) => continue, // Ignore formatting characters
                Some(true) => {}
            }

            self.add_ident_char(consumed_char);
            if consumed_surrogate != INVALID_CHARACTER {
                self.add_ident_char(consumed_surrogate);
            }
        }

        // LoopExit:
        if !self.ident_buffer.is_empty() {
            // NOTE: If we don't intern the string value, then we won't get a hit in the keyword dictionary!  (It
            // searches for a key using identity.) The text does not have to be interned (and probably shouldn't be if
            // it contains entities (else-case).
            let width = self.current_lexeme_width(); // exact size of input characters

            // id buffer is identical to width in input
            if utf8_len(&self.ident_buffer) == width {
                info.has_value = false;
            } else {
                self.value.clear();
                self.value.extend_from_slice(&self.ident_buffer);
                info.has_value = true;
            }

            true
        } else {
            info.has_value = false;
            self.text_window.reset(start);
            false
        }
    }

    /// The `default:` of `ScanIdentifier_CrefSlowPath`: `Some(true)` takes the character, `Some(false)` ignores it (a
    /// formatting character), `None` ends the identifier before it.
    fn cref_default(&mut self, consumed_char: u16, ident_len: usize, before_consumed: u32) -> Option<bool> {
        // This is the 'expensive' call
        if ident_len == 0 && consumed_char > 127 && facts::is_identifier_start_character(consumed_char) {
            Some(true)
        } else if ident_len > 0 && consumed_char > 127 && facts::is_identifier_part_character(consumed_char) {
            //// BUG 424819 : Handle identifier chars > 0xFFFF via surrogate pairs
            if facts::is_formatting_char(consumed_char) {
                return Some(false); // Ignore formatting characters
            }
            Some(true)
        } else {
            // Not a valid identifier character, so bail.
            self.text_window.reset(before_consumed);
            None
        }
    }

    /// `ScanIdentifierOrKeyword(ref info)`.
    fn scan_identifier_or_keyword(&mut self, info: &mut TokenInfo) -> bool {
        info.contextual_kind = SyntaxKind::None;

        let start = self.text_window.position();
        if self.scan_identifier(info) {
            // check to see if it is an actual keyword
            if !info.is_verbatim && !info.has_identifier_escape_sequence {
                let text = self.text_window.get_text(start);
                if self.mode_is(LexerMode::DIRECTIVE) {
                    let keyword_kind = crate::facts::get_preprocessor_keyword_kind(text);
                    if crate::facts::is_preprocessor_contextual_keyword(keyword_kind) {
                        // Let the parser decide which instances are actually keywords.
                        info.kind = SyntaxKind::IdentifierToken;
                        info.contextual_kind = keyword_kind;
                    } else {
                        info.kind = keyword_kind;
                    }
                } else {
                    info.kind = keyword_kind(text);
                    if info.kind == SyntaxKind::None {
                        info.contextual_kind = SyntaxKind::IdentifierToken;
                        info.kind = SyntaxKind::IdentifierToken;
                    } else if crate::facts::is_contextual_keyword(info.kind) {
                        info.contextual_kind = info.kind;
                        info.kind = SyntaxKind::IdentifierToken;
                    }
                }

                if info.kind == SyntaxKind::None {
                    info.kind = SyntaxKind::IdentifierToken;
                }
            } else {
                info.contextual_kind = SyntaxKind::IdentifierToken;
                info.kind = SyntaxKind::IdentifierToken;
            }

            true
        } else {
            info.kind = SyntaxKind::None;
            false
        }
    }

    /// `LexSyntaxTrivia(isFollowingToken, isTrailing, ref triviaList)`.
    fn lex_syntax_trivia(&mut self, is_following_token: bool, is_trailing: bool, trivia_list: &mut Vec<Trivia>) {
        let mut only_whitespace_on_line = !is_trailing;

        loop {
            self.start();
            let mut ch = self.text_window.peek_char();
            if ch == c(b' ') {
                let trivia = self.scan_whitespace();
                self.add_trivia(trivia, trivia_list);
                continue;
            } else if ch > 127 {
                if facts::is_whitespace(ch) {
                    ch = c(b' ');
                } else if facts::is_new_line(ch) {
                    ch = c(b'\n');
                }
            }

            match ascii(ch) {
                b' ' | b'\t' | 0x0B | 0x0C | 0x1A => {
                    let trivia = self.scan_whitespace();
                    self.add_trivia(trivia, trivia_list);
                }
                b'/' => {
                    let ch = self.text_window.peek_char_at(1);
                    if ch == c(b'/') {
                        if !self.suppress_documentation_comment_parse()
                            && self.text_window.peek_char_at(2) == c(b'/')
                            && self.text_window.peek_char_at(3) != c(b'/')
                        {
                            // Doc comments should never be in trailing trivia.
                            // Stop processing so that it will be leading trivia on the next token.
                            if is_trailing {
                                return;
                            }

                            let doc_comment = self.lex_xml_doc_comment(XmlDocCommentStyle::SingleLine);
                            let trivia = self.arena.structured_trivia(doc_comment);
                            self.add_trivia(trivia, trivia_list);
                            continue;
                        }

                        // normal single line comment
                        self.lex_single_line_comment(trivia_list);
                        only_whitespace_on_line = false;
                    } else if ch == c(b'*') {
                        if !self.suppress_documentation_comment_parse()
                            && self.text_window.peek_char_at(2) == c(b'*')
                            && self.text_window.peek_char_at(3) != c(b'*')
                            && self.text_window.peek_char_at(3) != c(b'/')
                        {
                            // Doc comments should never be in trailing trivia.
                            // Stop processing so that it will be leading trivia on the next token.
                            if is_trailing {
                                return;
                            }

                            let doc_comment = self.lex_xml_doc_comment(XmlDocCommentStyle::Delimited);
                            let trivia = self.arena.structured_trivia(doc_comment);
                            self.add_trivia(trivia, trivia_list);
                            continue;
                        }

                        self.lex_multi_line_comment(trivia_list, b'/');
                        only_whitespace_on_line = false;
                    } else {
                        // not trivia
                        return;
                    }
                }
                b'@' => {
                    if self.text_window.peek_char_at(1) == c(b'*') {
                        // Razor comment. We pretend that it's a multi-line comment for error recovery, but it's an
                        // error case.
                        let position = self.text_window.position();
                        self.add_error_at(position, 1, ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::Str("@")]);
                        self.lex_multi_line_comment(trivia_list, b'@');
                        only_whitespace_on_line = false;
                    } else {
                        return;
                    }
                }
                b'\r' | b'\n' => {
                    if let Some(end_of_line) = self.scan_end_of_line() {
                        self.add_trivia(end_of_line, trivia_list);
                    }
                    if is_trailing {
                        return;
                    }

                    only_whitespace_on_line = true;
                }
                b'#' => {
                    if self.allow_preprocessor_directives {
                        if is_trailing || !only_whitespace_on_line {
                            // Directives cannot be in trailing trivia.
                            // We parse the directive (ignoring its effects like disabled text or defined symbols).
                            // We extract the text corresponding to the parsed directive and add it as a BadToken in
                            // SkippedTokensTrivia.

                            let save_position = self.text_window.position();

                            // All the `false` arguments affect only error reporting and the resulting node, both of
                            // which we discard. However, passing `false` can skip some unnecessary checks.
                            self.parse_directive(false, false, false);

                            let width = self.text_window.position() - save_position;

                            let error = self.arena.make_diagnostic(0, 1, ErrorCode::ERR_BadDirectivePlacement, &[]);

                            let token = self.arena.create_token(
                                TriviaList::EMPTY,
                                SyntaxKind::BadToken,
                                SyntaxKind::BadToken,
                                TokenText::Source { start: save_position, width },
                                TriviaList::EMPTY,
                            );
                            let token = self.arena.with_diagnostics(token, &[error]);

                            let skipped = self.arena.skipped_tokens_trivia(token);
                            let trivia = self.arena.structured_trivia(skipped);
                            self.add_trivia(trivia, trivia_list);
                        } else {
                            self.lex_directive_and_excluded_trivia(is_following_token, trivia_list);
                        }

                        only_whitespace_on_line = true;
                    } else {
                        return;
                    }
                }
                // Note: we specifically do not look for the >>>>>>> pattern as the start of a conflict marker trivia.
                // That's because *technically* (albeit unlikely) >>>>>>> could be the end of a very generic construct.
                // So, instead, we only recognize >>>>>>> as we are scanning the trivia after a ======= marker (which
                // can never be part of legal code).
                // case '>':
                b'|' | b'=' | b'<' => {
                    if !is_trailing && self.is_conflict_marker_trivia() {
                        self.lex_conflict_marker_trivia(trivia_list);
                        continue;
                    }

                    return;
                }
                _ => return,
            }
        }
    }

    /// `lexSingleLineComment(ref triviaList)` (a local function of `LexSyntaxTrivia`).
    fn lex_single_line_comment(&mut self, trivia_list: &mut Vec<Trivia>) {
        self.scan_to_end_of_line();
        let trivia = self.lexeme_trivia(SyntaxKind::SingleLineCommentTrivia);
        self.add_trivia(trivia, trivia_list);
    }

    /// `lexMultiLineComment(ref triviaList, delimiter)` (a local function of `LexSyntaxTrivia`).
    fn lex_multi_line_comment(&mut self, trivia_list: &mut Vec<Trivia>, delimiter: u8) {
        let mut is_terminated = false;
        self.scan_multi_line_comment(&mut is_terminated, delimiter);
        if !is_terminated {
            // The comment didn't end.  Report an error at the start point.
            self.add_error_code(ErrorCode::ERR_OpenEndedComment, &[]);
        }

        let trivia = self.lexeme_trivia(SyntaxKind::MultiLineCommentTrivia);
        self.add_trivia(trivia, trivia_list);
    }

    /// An unstructured trivia of `kind` over the lexeme (`SyntaxFactory.Comment`, `Whitespace`, `DisabledText`, ...).
    #[inline]
    fn lexeme_trivia(&mut self, kind: SyntaxKind) -> Trivia {
        let width = self.current_lexeme_width();
        self.arena.trivia(kind, self.lexeme_start_position, width)
    }

    /// `IsConflictMarkerTrivia()`.
    fn is_conflict_marker_trivia(&self) -> bool {
        let position = self.text_window.position();
        let w = &self.text_window;
        if position == w.text_start() || facts::is_new_line(w.previous_char()) {
            let text = w.text();
            let first_ch = text[position as usize];
            debug_assert!(matches!(first_ch, b'<' | b'|' | b'=' | b'>'));

            if position + CONFLICT_MARKER_LENGTH <= w.text_end() {
                let p = position as usize;
                if text[p..p + CONFLICT_MARKER_LENGTH as usize].iter().any(|&b| b != first_ch) {
                    return false;
                }

                if matches!(first_ch, b'|' | b'=') {
                    return true;
                }

                return position + CONFLICT_MARKER_LENGTH < w.text_end()
                    && text[p + CONFLICT_MARKER_LENGTH as usize] == b' ';
            }
        }

        false
    }

    /// `LexConflictMarkerTrivia(ref triviaList)`.
    fn lex_conflict_marker_trivia(&mut self, trivia_list: &mut Vec<Trivia>) {
        self.start();

        let position = self.text_window.position();
        self.add_error_at(position, CONFLICT_MARKER_LENGTH, ErrorCode::ERR_Merge_conflict_marker_encountered, &[]);

        let start_ch = self.text_window.peek_char();

        // First create a trivia from the start of this merge conflict marker to the end of line/file (whichever comes
        // first).
        self.lex_conflict_marker_header(trivia_list);

        // Now add the newlines as the next trivia.
        self.lex_conflict_marker_end_of_line(trivia_list);

        // Now, if it was an ||||||| or ======= marker, then also created a DisabledText trivia for the contents of the
        // file after it, up until the next >>>>>>> marker we see.
        if start_ch == c(b'|') || start_ch == c(b'=') {
            self.lex_conflict_marker_disabled_text(start_ch == c(b'='), trivia_list);
        }
    }

    /// `LexConflictMarkerDisabledText(atSecondMiddleMarker, ref triviaList)`.
    fn lex_conflict_marker_disabled_text(&mut self, at_second_middle_marker: bool, trivia_list: &mut Vec<Trivia>) {
        // Consume everything from the end of the current mid-conflict marker to the start of the next end-conflict
        // marker
        self.start();

        let mut hit_next_marker = false;
        loop {
            let ch = self.text_window.peek_char();
            if ch == INVALID_CHARACTER {
                break;
            }

            if !at_second_middle_marker && ch == c(b'=') && self.is_conflict_marker_trivia() {
                hit_next_marker = true;
                break;
            }

            // If we hit the end-conflict marker, then lex it out at this point.
            if ch == c(b'>') && self.is_conflict_marker_trivia() {
                hit_next_marker = true;
                break;
            }

            self.text_window.advance_char();
        }

        if self.current_lexeme_width() > 0 {
            let trivia = self.lexeme_trivia(SyntaxKind::DisabledTextTrivia);
            self.add_trivia(trivia, trivia_list);
        }

        if hit_next_marker {
            self.lex_conflict_marker_trivia(trivia_list);
        }
    }

    /// `LexConflictMarkerEndOfLine(ref triviaList)`.
    fn lex_conflict_marker_end_of_line(&mut self, trivia_list: &mut Vec<Trivia>) {
        self.start();
        while facts::is_new_line(self.text_window.peek_char()) {
            self.text_window.advance_char();
        }

        if self.current_lexeme_width() > 0 {
            let trivia = self.lexeme_trivia(SyntaxKind::EndOfLineTrivia);
            self.add_trivia(trivia, trivia_list);
        }
    }

    /// `LexConflictMarkerHeader(ref triviaList)`.
    fn lex_conflict_marker_header(&mut self, trivia_list: &mut Vec<Trivia>) {
        loop {
            let ch = self.text_window.peek_char();
            if ch == INVALID_CHARACTER || facts::is_new_line(ch) {
                break;
            }

            self.text_window.advance_char();
        }

        let trivia = self.lexeme_trivia(SyntaxKind::ConflictMarkerTrivia);
        self.add_trivia(trivia, trivia_list);
    }

    /// `AddTrivia(trivia, ref list)`: with the errors so far.
    fn add_trivia(&mut self, trivia: Trivia, list: &mut Vec<Trivia>) {
        let trivia = if self.has_errors() { self.arena.trivia_with_diagnostics(trivia, &self.errors) } else { trivia };
        list.push(trivia);
    }

    /// `ScanMultiLineComment(out isTerminated, delimiter)`.
    pub(crate) fn scan_multi_line_comment(&mut self, is_terminated: &mut bool, delimiter: u8) {
        debug_assert!(self.text_window.peek_char() == c(delimiter) && self.text_window.peek_char_at(1) == c(b'*'));
        self.text_window.advance_char_n(2);

        loop {
            let ch = self.text_window.peek_char();
            if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() {
                *is_terminated = false;
                break;
            } else if ch == c(b'*') && self.text_window.peek_char_at(1) == c(delimiter) {
                self.text_window.advance_char_n(2);
                *is_terminated = true;
                break;
            } else {
                self.text_window.advance_char();
            }
        }
    }

    /// `ScanToEndOfLine()`.
    pub(crate) fn scan_to_end_of_line(&mut self) {
        loop {
            let ch = self.text_window.peek_char();
            if facts::is_new_line(ch) || (ch == INVALID_CHARACTER && self.text_window.is_really_at_end()) {
                break;
            }
            self.text_window.advance_char();
        }
    }

    /// `ScanEndOfLine()`: scans a new-line sequence (either a single new-line character or a CR-LF combo).
    fn scan_end_of_line(&mut self) -> Option<Trivia> {
        let start = self.text_window.position();
        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'\r' => {
                self.text_window.advance_char();
                self.text_window.try_advance(b'\n');
            }
            b'\n' => self.text_window.advance_char(),
            _ => {
                if !facts::is_new_line(ch) {
                    return None;
                }
                self.text_window.advance_char();
            }
        }
        let width = self.text_window.position() - start;
        Some(self.arena.trivia(SyntaxKind::EndOfLineTrivia, start, width))
    }

    /// `ScanWhitespace()`: scans all of the whitespace (not new-lines) into a trivia node until it runs out.
    fn scan_whitespace(&mut self) -> Trivia {
        debug_assert!(facts::is_whitespace(self.text_window.peek_char()));
        let start = self.text_window.position();
        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                b' ' | b'\t' | 0x0B | 0x0C | 0x1A => self.text_window.advance_char(),
                b'\r' | b'\n' => break,
                _ => {
                    if ch > 127 && facts::is_whitespace(ch) {
                        self.text_window.advance_char();
                    } else {
                        break;
                    }
                }
            }
        }
        let width = self.text_window.position() - start;
        self.arena.trivia(SyntaxKind::WhitespaceTrivia, start, width)
    }

    /// `LexDirectiveAndExcludedTrivia(isFollowingToken, ref triviaList)`.
    fn lex_directive_and_excluded_trivia(&mut self, is_following_token: bool, trivia_list: &mut Vec<Trivia>) {
        let directive = self.lex_single_directive(true, true, is_following_token, trivia_list);

        // also lex excluded stuff
        if is_branching(self.arena.kind(directive)) && !self.branch_taken(directive) {
            self.lex_excluded_directives_and_trivia(true, trivia_list);
        }
    }

    /// `BranchingDirectiveTriviaSyntax.BranchTaken`.
    fn branch_taken(&self, directive: Green) -> bool {
        self.arena.extra(directive) & crate::syntax::types::IfDirectiveTriviaSyntax::BRANCH_TAKEN != 0
    }

    /// `LexExcludedDirectivesAndTrivia(endIsActive, ref triviaList)`: Roslyn recurses for each nested `#if` of
    /// excluded text; here a stack of the levels' `endIsActive` (a level ends at its `#endif` or at a branch taken,
    /// every level at the end of the file).
    fn lex_excluded_directives_and_trivia(&mut self, end_is_active: bool, trivia_list: &mut Vec<Trivia>) {
        // the nested levels are all `endIsActive: false`: their count is enough
        let mut nested = 0u32;
        loop {
            let level_end_is_active = if nested == 0 { end_is_active } else { false };
            let mut has_following_directive = false;
            if let Some(text) = self.lex_disabled_text(&mut has_following_directive) {
                self.add_trivia(text, trivia_list);
            }

            if !has_following_directive {
                // the end of the file: every level breaks out of its loop
                break;
            }

            let directive = self.lex_single_directive(false, level_end_is_active, false, trivia_list);
            let kind = self.arena.kind(directive);
            if kind == SyntaxKind::EndIfDirectiveTrivia || (is_branching(kind) && self.branch_taken(directive)) {
                if nested == 0 {
                    break;
                }
                nested -= 1;
            } else if kind == SyntaxKind::IfDirectiveTrivia {
                nested += 1;
            }
        }
    }

    /// `LexSingleDirective(isActive, endIsActive, isFollowingToken, ref triviaList)`.
    fn lex_single_directive(
        &mut self,
        is_active: bool,
        end_is_active: bool,
        is_following_token: bool,
        trivia_list: &mut Vec<Trivia>,
    ) -> Green {
        if facts::is_whitespace(self.text_window.peek_char()) {
            self.start();
            let trivia = self.scan_whitespace();
            self.add_trivia(trivia, trivia_list);
        }

        let directive = self.parse_directive(is_active, end_is_active, is_following_token);

        let trivia = self.arena.structured_trivia(directive);
        self.add_trivia(trivia, trivia_list);
        // `directive.ApplyDirectives(_directives)`
        let d = self.buffers.directives.directive(&self.arena, self.text_window.text(), directive);
        self.directives = self.directives.add(&mut self.buffers.directives, d);
        directive
    }

    /// `ParseDirective(isActive, endIsActive, isFollowingToken)`.
    fn parse_directive(&mut self, is_active: bool, end_is_active: bool, is_following_token: bool) -> Green {
        let save_mode = self.mode;

        let buffers = std::mem::take(&mut self.buffers.directive_parser);
        let context = self.directives;
        let (directive, buffers) = {
            let mut parser = DirectiveParser::new(self, buffers, context);
            let directive = parser.parse_directive(is_active, end_is_active, is_following_token);
            (directive, parser.into_buffers())
        };
        self.buffers.directive_parser = buffers;

        self.mode = save_mode;
        directive
    }

    /// `LexDisabledText(out followedByDirective)`: consume text up to the next directive.
    fn lex_disabled_text(&mut self, followed_by_directive: &mut bool) -> Option<Trivia> {
        self.start();

        let mut last_line_start = self.text_window.position();
        let mut all_whitespace = true;

        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                    *followed_by_directive = false;
                    return (self.current_lexeme_width() > 0)
                        .then(|| self.lexeme_trivia(SyntaxKind::DisabledTextTrivia));
                }
                b'#' if self.allow_preprocessor_directives => {
                    *followed_by_directive = true;
                    if last_line_start < self.text_window.position() && !all_whitespace {
                        // goto default
                        all_whitespace = false;
                        self.text_window.advance_char();
                        continue;
                    }

                    self.text_window.reset(last_line_start); // reset so directive parser can consume the starting whitespace on this line
                    return (self.current_lexeme_width() > 0)
                        .then(|| self.lexeme_trivia(SyntaxKind::DisabledTextTrivia));
                }
                b'\r' | b'\n' => {
                    self.scan_end_of_line_position();
                    last_line_start = self.text_window.position();
                    all_whitespace = true;
                }
                _ => {
                    if facts::is_new_line(ch) {
                        self.scan_end_of_line_position();
                        last_line_start = self.text_window.position();
                        all_whitespace = true;
                        continue;
                    }

                    all_whitespace = all_whitespace && facts::is_whitespace(ch);
                    self.text_window.advance_char();
                }
            }
        }
    }

    /// `ScanEndOfLine()` whose trivia is not used: past the line break.
    fn scan_end_of_line_position(&mut self) {
        if self.text_window.peek_char() == c(b'\r') {
            self.text_window.advance_char();
            self.text_window.try_advance(b'\n');
        } else if facts::is_new_line(self.text_window.peek_char()) {
            self.text_window.advance_char();
        }
    }

    /// `LexDirectiveToken()`.
    fn lex_directive_token(&mut self) -> Green {
        self.start();
        let start = self.lexeme_start_position;
        let mut info = TokenInfo::default();
        self.scan_directive_token(&mut info);
        let end = self.text_window.position();
        let mut errors = std::mem::take(&mut self.buffers.token_errors);
        self.get_errors(&mut errors);

        let mut directive_trivia_cache = std::mem::take(&mut self.buffers.directive_trivia);
        directive_trivia_cache.clear();

        self.lex_directive_trailing_trivia(info.kind == SyntaxKind::EndOfDirectiveToken, &mut directive_trivia_cache);

        let trailing = self.arena.trivia_list(&directive_trivia_cache);
        let token = self.create(&info, start, end, TriviaList::EMPTY, trailing, &errors);
        self.buffers.directive_trivia = directive_trivia_cache;
        self.buffers.token_errors = errors;

        token
    }

    /// `LexOptionalPreprocessingMessage()`: skip the rest of the line until we hit a EOL or EOF (the PP_Message
    /// portion of the specification). The message's range, `None` when it is empty.
    fn lex_optional_preprocessing_message(&mut self) -> Option<(u32, u32)> {
        let start = self.text_window.position();
        loop {
            let ch = self.text_window.peek_char();
            if facts::is_new_line(ch) {
                // don't consume EOL characters here
                break;
            } else if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() {
                // don't consume EOF characters here
                break;
            }

            self.text_window.advance_char();
        }
        let end = self.text_window.position();
        (end > start).then_some((start, end - start))
    }

    /// `LexEndOfDirectiveAfterOptionalPreprocessingMessage(leading)`.
    fn lex_end_of_directive_after_optional_preprocessing_message(&mut self, leading: TriviaList) -> Green {
        // now try to consume the EOL if there.
        let mut directive_trivia_cache = std::mem::take(&mut self.buffers.directive_trivia);
        directive_trivia_cache.clear();

        let position = self.text_window.position();
        self.lex_directive_trailing_trivia(true, &mut directive_trivia_cache);
        let trailing = self.arena.trivia_list(&directive_trivia_cache);
        self.buffers.directive_trivia = directive_trivia_cache;

        self.arena.create_token(
            leading,
            SyntaxKind::EndOfDirectiveToken,
            SyntaxKind::EndOfDirectiveToken,
            TokenText::Source { start: position, width: 0 },
            trailing,
        )
    }

    /// `LexEndOfDirectiveWithOptionalPreprocessingMessage()`.
    pub(crate) fn lex_end_of_directive_with_optional_preprocessing_message(&mut self) -> Green {
        let leading = match self.lex_optional_preprocessing_message() {
            Some((start, width)) => {
                let message = self.arena.trivia(SyntaxKind::PreprocessingMessageTrivia, start, width);
                self.arena.trivia_list(&[message])
            }
            None => TriviaList::EMPTY,
        };

        self.lex_end_of_directive_after_optional_preprocessing_message(leading)
    }

    /// `LexEndOfDirectiveWithOptionalContent(out content)`.
    pub(crate) fn lex_end_of_directive_with_optional_content(&mut self, content: &mut Green) -> Green {
        *content = match self.lex_optional_preprocessing_message() {
            // `SyntaxToken.StringLiteral(message)`
            Some((start, width)) => self.arena.create_token(
                TriviaList::EMPTY,
                SyntaxKind::StringLiteralToken,
                SyntaxKind::StringLiteralToken,
                TokenText::Source { start, width },
                TriviaList::EMPTY,
            ),
            None => Green::NULL,
        };
        self.lex_end_of_directive_after_optional_preprocessing_message(TriviaList::EMPTY)
    }

    /// `ScanDirectiveToken(ref info)`.
    fn scan_directive_token(&mut self, info: &mut TokenInfo) -> bool {
        let mut is_escaped = false;
        let mut character = self.text_window.peek_char();
        let eod = |info: &mut TokenInfo| {
            // don't consume end characters here
            info.kind = SyntaxKind::EndOfDirectiveToken;
        };
        match ascii(character) {
            _ if character == INVALID_CHARACTER && self.text_window.is_really_at_end() => eod(info),
            b'\r' | b'\n' => eod(info),
            b'#' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::HashToken;
            }
            b'(' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::OpenParenToken;
            }
            b')' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::CloseParenToken;
            }
            b',' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::CommaToken;
            }
            b'-' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::MinusToken;
            }
            b':' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::ColonToken;
            }
            b'!' => {
                self.text_window.advance_char();
                if self.text_window.try_advance(b'=') {
                    info.kind = SyntaxKind::ExclamationEqualsToken;
                } else {
                    info.kind = SyntaxKind::ExclamationToken;
                }
            }
            b'=' => {
                self.text_window.advance_char();
                if self.text_window.try_advance(b'=') {
                    info.kind = SyntaxKind::EqualsEqualsToken;
                } else {
                    info.kind = SyntaxKind::EqualsToken;
                }
            }
            b'&' if self.text_window.peek_char_at(1) == c(b'&') => {
                self.text_window.advance_char_n(2);
                info.kind = SyntaxKind::AmpersandAmpersandToken;
            }
            b'|' if self.text_window.peek_char_at(1) == c(b'|') => {
                self.text_window.advance_char_n(2);
                info.kind = SyntaxKind::BarBarToken;
            }
            b'0'..=b'9' => {
                self.scan_integer();
                info.kind = SyntaxKind::NumericLiteralToken;
                self.get_value_int32();
            }
            b'"' => self.scan_string_literal(info, true),
            _ => {
                if character == c(b'\\') {
                    // Could be unicode escape. Try that.
                    let mut surrogate_character = INVALID_CHARACTER;
                    character = self.peek_char_or_unicode_escape(&mut surrogate_character);
                    is_escaped = true;
                    if facts::is_identifier_start_character(character) {
                        self.scan_identifier_or_keyword(info);
                        return info.kind != SyntaxKind::None;
                    }
                }

                if !is_escaped && facts::is_new_line(character) {
                    eod(info);
                } else if facts::is_identifier_start_character(character) {
                    self.scan_identifier_or_keyword(info);
                } else {
                    // unknown single character
                    if is_escaped {
                        let mut surrogate_character = INVALID_CHARACTER;
                        let mut error = None;
                        self.next_char_or_unicode_escape(&mut surrogate_character, &mut error);
                        self.add_error(error);
                    } else {
                        self.text_window.advance_char();
                    }

                    info.kind = SyntaxKind::None;
                }
            }
        }

        info.kind != SyntaxKind::None
    }

    /// `GetValueInt32(text, isHex: false)` of the lexeme (used in directives): the overflow error.
    fn get_value_int32(&mut self) {
        if parse_int32(self.get_lexeme_text()).is_none() {
            //we've already lexed the literal, so the error must be from overflow
            let error = self.make_error_code(ErrorCode::ERR_IntOverflow, &[]);
            self.add_error(Some(error));
        }
    }

    /// `LexDirectiveTrailingTrivia(includeEndOfLine, ref trivia)`.
    fn lex_directive_trailing_trivia(&mut self, include_end_of_line: bool, trivia: &mut Vec<Trivia>) {
        loop {
            let pos = self.text_window.position();
            let Some(tr) = self.lex_directive_trivia() else { break };
            if self.arena.trivia_kind(tr) == SyntaxKind::EndOfLineTrivia {
                if include_end_of_line {
                    self.add_trivia(tr, trivia);
                } else {
                    // don't consume end of line...
                    self.text_window.reset(pos);
                }

                break;
            } else {
                self.add_trivia(tr, trivia);
            }
        }
    }

    /// `LexDirectiveTrivia()`.
    fn lex_directive_trivia(&mut self) -> Option<Trivia> {
        self.start();
        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'/' => {
                if self.text_window.peek_char_at(1) == c(b'/') {
                    // normal single line comment
                    self.scan_to_end_of_line();
                    return Some(self.lexeme_trivia(SyntaxKind::SingleLineCommentTrivia));
                }
                None
            }
            b'\r' | b'\n' => self.scan_end_of_line(),
            b' ' | b'\t' | 0x0B | 0x0C => Some(self.scan_whitespace()),
            _ => {
                if facts::is_whitespace(ch) {
                    return Some(self.scan_whitespace());
                }
                if facts::is_new_line(ch) {
                    return self.scan_end_of_line();
                }
                None
            }
        }
    }

    /// `LexXmlDocComment(style)`.
    fn lex_xml_doc_comment(&mut self, style: XmlDocCommentStyle) -> Green {
        let save_mode = self.mode;

        let buffers = std::mem::take(&mut self.buffers.xml_parser);
        let mut is_terminated = true;
        let (doc_comment, buffers) = {
            let mut parser = DocumentationCommentParser::new(self, buffers);
            parser.re_initialize(if style == XmlDocCommentStyle::SingleLine {
                LexerMode::XML_DOC_COMMENT_STYLE_SINGLE_LINE
            } else {
                LexerMode::XML_DOC_COMMENT_STYLE_DELIMITED
            });
            let doc_comment = parser.parse_documentation_comment(&mut is_terminated);
            (doc_comment, parser.into_buffers())
        };
        self.buffers.xml_parser = buffers;

        // We better have finished with the whole comment. There should be error
        // code in the implementation of ParseXmlDocComment that ensures this.
        debug_assert!(
            self.location_is(XmlDocCommentLocation::End) || self.text_window.peek_char() == INVALID_CHARACTER
        );

        self.mode = save_mode;

        if !is_terminated {
            // The comment didn't end.  Report an error at the start point.
            // NOTE: report this error even if the DocumentationMode is less than diagnose - the comment would be
            // malformed as a non-doc comment as well.
            let (position, width) = (self.lexeme_start_position, self.current_lexeme_width());
            self.add_error_at(position, width, ErrorCode::ERR_OpenEndedComment, &[]);
        }

        doc_comment
    }

    /// `LexXmlToken()`: lexer entry point for LexMode.XmlDocComment.
    fn lex_xml_token(&mut self) -> Green {
        let mut xml_token_info = TokenInfo::default();

        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_token(&mut xml_token_info);
        self.finish_xml_token(&xml_token_info, start, leading)
    }

    /// The end of an XML token's `Lex*` method: `Create(in info, leading, null, errors)`.
    fn finish_xml_token(&mut self, info: &TokenInfo, start: u32, leading: Vec<Trivia>) -> Green {
        let end = self.text_window.position();
        let mut errors = std::mem::take(&mut self.buffers.token_errors);
        self.get_errors(&mut errors);
        let leading_list = self.arena.trivia_list(&leading);
        let token = self.create(info, start, end, leading_list, TriviaList::EMPTY, &errors);
        self.buffers.xml_trivia = leading;
        self.buffers.token_errors = errors;
        token
    }

    /// `ScanXmlToken(ref info)`.
    fn scan_xml_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'&' => {
                self.scan_xml_entity(info);
                info.kind = SyntaxKind::XmlEntityLiteralToken;
            }
            b'<' => self.scan_xml_tag_start(info),
            b'\r' | b'\n' => self.scan_xml_text_literal_new_line_token(info),
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            }
            _ => {
                if facts::is_new_line(ch) {
                    self.scan_xml_text_literal_new_line_token(info);
                } else {
                    self.scan_xml_text(info);
                    info.kind = SyntaxKind::XmlTextLiteralToken;
                }
            }
        }

        info.kind != SyntaxKind::None
    }

    /// `ScanXmlTextLiteralNewLineToken(ref info)`.
    fn scan_xml_text_literal_new_line_token(&mut self, info: &mut TokenInfo) {
        self.scan_end_of_line_position();
        info.kind = SyntaxKind::XmlTextLiteralNewLineToken;
        self.mutate_location(XmlDocCommentLocation::Exterior);
    }

    /// `ScanXmlTagStart(ref info)`.
    fn scan_xml_tag_start(&mut self, info: &mut TokenInfo) {
        debug_assert!(self.text_window.peek_char() == c(b'<'));

        if self.text_window.peek_char_at(1) == c(b'!') {
            if self.text_window.peek_char_at(2) == c(b'-') && self.text_window.peek_char_at(3) == c(b'-') {
                self.text_window.advance_char_n(4);
                info.kind = SyntaxKind::XmlCommentStartToken;
            } else if self.text_window.peek_char_at(2) == c(b'[')
                && self.text_window.peek_char_at(3) == c(b'C')
                && self.text_window.peek_char_at(4) == c(b'D')
                && self.text_window.peek_char_at(5) == c(b'A')
                && self.text_window.peek_char_at(6) == c(b'T')
                && self.text_window.peek_char_at(7) == c(b'A')
                && self.text_window.peek_char_at(8) == c(b'[')
            {
                self.text_window.advance_char_n(9);
                info.kind = SyntaxKind::XmlCDataStartToken;
            } else {
                // TODO: Take the < by itself, I guess?
                self.text_window.advance_char();
                info.kind = SyntaxKind::LessThanToken;
            }
        } else if self.text_window.peek_char_at(1) == c(b'/') {
            self.text_window.advance_char_n(2);
            info.kind = SyntaxKind::LessThanSlashToken;
        } else if self.text_window.peek_char_at(1) == c(b'?') {
            self.text_window.advance_char_n(2);
            info.kind = SyntaxKind::XmlProcessingInstructionStartToken;
        } else {
            self.text_window.advance_char();
            info.kind = SyntaxKind::LessThanToken;
        }
    }

    /// `ScanXmlEntity(ref info)`.
    fn scan_xml_entity(&mut self, info: &mut TokenInfo) {
        info.has_value = false;

        // `Debug.Assert(TextWindow.PeekChar() == '&')`: a cref's `@` not followed by an identifier comes here too
        self.text_window.advance_char();
        self.builder.clear();
        let mut error: Option<XmlParseErrorCode> = None;
        let mut error_arg: Option<DiagArg> = None;
        // `info.StringValue`, when the entity has a value
        let mut value: Option<[u16; 2]> = None;

        let mut ch = self.text_window.peek_char();
        if is_xml_name_start_char(ch) {
            loop {
                ch = self.text_window.peek_char();
                if !is_xml_name_char(ch) {
                    break;
                }
                // Important bit of information here: none of \0, \r, \n, and crucially for delimited comments, * are
                // considered Xml name characters. Also, since entities appear in xml text and attribute text, it's
                // relevant here that none of <, /, >, ', ", =, are Xml name characters. Note that - and ] are
                // irrelevant--entities do not appear in comments or cdata.

                self.text_window.advance_char();
                self.builder.push(ch);
            }

            let name: &[u16] = &self.builder;
            let known = |s: &[u8]| name.len() == s.len() && name.iter().zip(s).all(|(&a, &b)| a == b as u16);
            if known(b"lt") {
                value = Some([c(b'<'), INVALID_CHARACTER]);
            } else if known(b"gt") {
                value = Some([c(b'>'), INVALID_CHARACTER]);
            } else if known(b"amp") {
                value = Some([c(b'&'), INVALID_CHARACTER]);
            } else if known(b"apos") {
                value = Some([c(b'\''), INVALID_CHARACTER]);
            } else if known(b"quot") {
                value = Some([c(b'"'), INVALID_CHARACTER]);
            } else {
                error = Some(XmlParseErrorCode::XML_RefUndefinedEntity_1);
                let text = String::from_utf16_lossy(&self.builder);
                let (start, len) = self.arena.intern(&text);
                error_arg = Some(DiagArg::Owned { start, len });
            }
        } else if ch == c(b'#') {
            self.text_window.advance_char();
            let is_hex = self.text_window.peek_char() == c(b'x');
            let mut char_value: u32 = 0;

            if is_hex {
                self.text_window.advance_char(); // x
                loop {
                    ch = self.text_window.peek_char();
                    if !facts::is_hex_digit(ch) {
                        break;
                    }
                    self.text_window.advance_char();

                    // disallow overflow
                    if char_value <= 0x7FFFFFF {
                        char_value = (char_value << 4) + facts::hex_value(ch);
                    }
                }
            } else {
                loop {
                    ch = self.text_window.peek_char();
                    if !facts::is_dec_digit(ch) {
                        break;
                    }
                    self.text_window.advance_char();

                    // disallow overflow
                    if char_value <= 0x7FFFFFF {
                        char_value = (char_value << 3) + (char_value << 1) + facts::dec_value(ch);
                    }
                }
            }

            if self.text_window.peek_char() != c(b';') {
                error = Some(XmlParseErrorCode::XML_InvalidCharEntity);
            }

            if matches_production_for_xml_char(char_value) {
                let mut low_surrogate = INVALID_CHARACTER;
                let high_surrogate = get_chars_from_utf32(char_value, &mut low_surrogate);
                value = Some([high_surrogate, low_surrogate]);
            } else if error.is_none() {
                error = Some(XmlParseErrorCode::XML_InvalidUnicodeChar);
            }
        } else if facts::is_whitespace(ch) || facts::is_new_line(ch) {
            if error.is_none() {
                error = Some(XmlParseErrorCode::XML_InvalidWhitespace);
            }
        } else if error.is_none() {
            error = Some(XmlParseErrorCode::XML_InvalidToken);
            error_arg = Some(DiagArg::Char(ch));
        }

        ch = self.text_window.peek_char();
        if ch == c(b';') {
            self.text_window.advance_char();
        } else if error.is_none() {
            error = Some(XmlParseErrorCode::XML_InvalidToken);
            error_arg = Some(DiagArg::Char(ch));
        }

        // If we don't have a value computed from above, then we don't recognize the entity, in which case we will
        // simply use the text.
        match value {
            Some([high, low]) => {
                self.value.clear();
                self.value.push(high);
                if low != INVALID_CHARACTER {
                    self.value.push(low);
                }
                info.has_value = true;
            }
            None => info.has_value = false,
        }

        if let Some(error) = error {
            match error_arg {
                Some(arg) => self.add_xml_error(error, &[arg]),
                None => self.add_xml_error(error, &[]),
            }
        }
    }

    /// `ScanXmlText(ref info)`.
    fn scan_xml_text(&mut self, info: &mut TokenInfo) {
        info.has_value = false;
        // Collect "]]>" strings into their own XmlText.
        if self.text_window.peek_char() == c(b']')
            && self.text_window.peek_char_at(1) == c(b']')
            && self.text_window.peek_char_at(2) == c(b'>')
        {
            self.text_window.advance_char_n(3);
            self.add_xml_error(XmlParseErrorCode::XML_CDataEndTagNotAllowed, &[]);
            return;
        }

        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => return,
                b'&' | b'<' | b'\r' | b'\n' => return,
                b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                    // we're at the end of the comment, but don't lex it yet.
                    return;
                }
                b']' if self.text_window.peek_char_at(1) == c(b']') && self.text_window.peek_char_at(2) == c(b'>') => {
                    return;
                }
                _ => {
                    if facts::is_new_line(ch) {
                        return;
                    }

                    self.text_window.advance_char();
                }
            }
        }
    }

    /// `LexXmlElementTagToken()`: lexer entry point for LexMode.XmlElementTag (the `DocumentationCommentXmlTokens`
    /// lookup only shares equal tokens).
    fn lex_xml_element_tag_token(&mut self) -> Green {
        let mut tag_info = TokenInfo::default();

        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia_with_whitespace(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_element_tag_token(&mut tag_info);
        self.finish_xml_token(&tag_info, start, leading)
    }

    /// `ScanXmlElementTagToken(ref info)`.
    fn scan_xml_element_tag_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        let mut default = false;
        match ascii(ch) {
            b'<' => self.scan_xml_tag_start(info),
            b'>' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::GreaterThanToken;
            }
            b'/' => {
                if self.text_window.peek_char_at(1) == c(b'>') {
                    self.text_window.advance_char_n(2);
                    info.kind = SyntaxKind::SlashGreaterThanToken;
                } else {
                    default = true;
                }
            }
            b'"' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::DoubleQuoteToken;
            }
            b'\'' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::SingleQuoteToken;
            }
            b'=' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::EqualsToken;
            }
            b':' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::ColonToken;
            }
            b'\r' | b'\n' => {
                // Assert?
            }
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            }
            b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                // Assert? We should have gotten this in the leading trivia.
            }
            _ => default = true,
        }
        if default {
            if is_xml_name_start_char(ch) {
                self.scan_xml_name(info);
                info.has_value = false;
                info.kind = SyntaxKind::IdentifierToken;
                // `SyntaxFactory.Identifier(SyntaxKind.None, ...)`, as `DocumentationCommentXmlTokens` makes its too
                info.contextual_kind = SyntaxKind::None;
            } else if facts::is_whitespace(ch) || facts::is_new_line(ch) {
                // whitespace! needed to do a better job with trivia
            } else {
                self.text_window.advance_char();
                info.kind = SyntaxKind::None;
                info.has_value = false;
            }
        }

        info.kind != SyntaxKind::None
    }

    /// `ScanXmlName(ref info)`.
    fn scan_xml_name(&mut self, info: &mut TokenInfo) {
        let _ = info;
        loop {
            let ch = self.text_window.peek_char();

            // Important bit of information here: none of \0, \r, \n, and crucially for delimited comments, * are
            // considered Xml name characters.
            if ch != c(b':') && is_xml_name_char(ch) {
                // Although ':' is a name char, we don't include it in ScanXmlName since it is its own token. This
                // enables the parser to add structure to colon-separated names.
                self.text_window.advance_char();
            } else {
                break;
            }
        }
    }

    /// `LexXmlAttributeTextToken()`: lexer entry point for LexMode.XmlAttributeText.
    fn lex_xml_attribute_text_token(&mut self) -> Green {
        let mut info = TokenInfo::default();

        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_attribute_text_token(&mut info);
        self.finish_xml_token(&info, start, leading)
    }

    /// `ScanXmlAttributeTextToken(ref info)`.
    fn scan_xml_attribute_text_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'"' if self.mode_is(LexerMode::XML_ATTRIBUTE_TEXT_DOUBLE_QUOTE) => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::DoubleQuoteToken;
            }
            b'\'' if self.mode_is(LexerMode::XML_ATTRIBUTE_TEXT_QUOTE) => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::SingleQuoteToken;
            }
            b'&' => {
                self.scan_xml_entity(info);
                info.kind = SyntaxKind::XmlEntityLiteralToken;
            }
            b'<' => {
                self.text_window.advance_char();
                info.kind = SyntaxKind::LessThanToken;
            }
            b'\r' | b'\n' => self.scan_xml_text_literal_new_line_token(info),
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            }
            _ => {
                if facts::is_new_line(ch) {
                    self.scan_xml_text_literal_new_line_token(info);
                } else {
                    self.scan_xml_attribute_text(info);
                    info.kind = SyntaxKind::XmlTextLiteralToken;
                }
            }
        }

        info.kind != SyntaxKind::None
    }

    /// `ScanXmlAttributeText(ref info)`.
    fn scan_xml_attribute_text(&mut self, info: &mut TokenInfo) {
        info.has_value = false;
        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                b'"' if self.mode_is(LexerMode::XML_ATTRIBUTE_TEXT_DOUBLE_QUOTE) => return,
                b'\'' if self.mode_is(LexerMode::XML_ATTRIBUTE_TEXT_QUOTE) => return,
                b'&' | b'<' | b'\r' | b'\n' => return,
                _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => return,
                b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                    // we're at the end of the comment, but don't lex it yet.
                    return;
                }
                _ => {
                    if facts::is_new_line(ch) {
                        return;
                    }

                    self.text_window.advance_char();
                }
            }
        }
    }

    /// `LexXmlCharacter()`: lexer entry point for LexerMode.XmlCharacter.
    fn lex_xml_character(&mut self) -> Green {
        let mut info = TokenInfo::default();

        //TODO: Dev11 allows C# comments and newlines in cref trivia (DevDiv #530523).
        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia_with_whitespace(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_character(&mut info);
        self.finish_xml_token(&info, start, leading)
    }

    /// `ScanXmlCharacter(ref info)`: scan a single XML character (or entity). Assumes that leading trivia has already
    /// been consumed.
    fn scan_xml_character(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'&' => {
                self.scan_xml_entity(info);
                info.kind = SyntaxKind::XmlEntityLiteralToken;
            }
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfFileToken;
            }
            _ => {
                info.kind = SyntaxKind::XmlTextLiteralToken;
                info.has_value = false;
                self.text_window.next_char();
            }
        }

        true
    }

    /// `LexXmlCrefOrNameToken()`: lexer entry point for LexerMode.XmlCrefQuote, LexerMode.XmlCrefDoubleQuote,
    /// LexerMode.XmlNameQuote, and LexerMode.XmlNameDoubleQuote.
    fn lex_xml_cref_or_name_token(&mut self) -> Green {
        let mut info = TokenInfo::default();

        //TODO: Dev11 allows C# comments and newlines in cref trivia (DevDiv #530523).
        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia_with_whitespace(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_cref_token(&mut info);
        self.finish_xml_token(&info, start, leading)
    }

    /// `ScanXmlCrefToken(ref info)`: scan a single cref attribute token. Assumes that leading trivia has already been
    /// consumed. Within this method, characters that are not XML meta-characters can be seamlessly replaced with the
    /// corresponding XML entities.
    fn scan_xml_cref_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let before_consumed = self.text_window.position();
        let mut consumed_char = self.text_window.next_char();
        let mut consumed_surrogate = INVALID_CHARACTER;

        // This first switch is for special characters.  If we see the corresponding XML entities, we DO NOT want to
        // take these actions.
        match ascii(consumed_char) {
            b'"' => {
                if self.mode_is(LexerMode::XML_CREF_DOUBLE_QUOTE) || self.mode_is(LexerMode::XML_NAME_DOUBLE_QUOTE) {
                    info.kind = SyntaxKind::DoubleQuoteToken;
                    return true;
                }
            }
            b'\'' => {
                if self.mode_is(LexerMode::XML_CREF_QUOTE) || self.mode_is(LexerMode::XML_NAME_QUOTE) {
                    info.kind = SyntaxKind::SingleQuoteToken;
                    return true;
                }
            }
            b'<' => {
                let text = String::from_utf8_lossy(self.get_lexeme_text()).into_owned();
                let (start, len) = self.arena.intern(&text);
                //ErrorCode.WRN_XMLParseError
                self.add_xml_error(XmlParseErrorCode::XML_LessThanInAttributeValue, &[DiagArg::Owned { start, len }]);
                return true;
            }
            _ if consumed_char == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
                return true;
            }
            b'\r' | b'\n' => {
                self.text_window.reset(before_consumed);
                self.scan_xml_text_literal_new_line_token(info);
            }
            b'&' => {
                self.text_window.reset(before_consumed);
                if !self.try_scan_xml_entity(&mut consumed_char, &mut consumed_surrogate) {
                    self.text_window.reset(before_consumed);
                    self.scan_xml_entity(info);
                    info.kind = SyntaxKind::XmlEntityLiteralToken;
                    return true;
                }

                // TryScanXmlEntity advances even when it returns false.
            }
            b'{' => consumed_char = c(b'<'),
            b'}' => consumed_char = c(b'>'),
            _ => {
                if facts::is_new_line(consumed_char) {
                    self.text_window.reset(before_consumed);
                    self.scan_xml_text_literal_new_line_token(info);
                }
            }
        }

        debug_assert!(self.text_window.position() > before_consumed, "First character or entity has been consumed.");

        // NOTE: None of these cases will be matched if the surrogate is non-zero (UTF-16 rules) so we don't need to
        // check for that explicitly.

        // NOTE: there's a lot of overlap between this switch and the one in ScanSyntaxToken, but we probably don't want
        // to share code because ScanSyntaxToken is really hot code and this switch does some extra work.
        match ascii(consumed_char) {
            //// Single-Character Punctuation/Operators ////
            b'(' => info.kind = SyntaxKind::OpenParenToken,
            b')' => info.kind = SyntaxKind::CloseParenToken,
            b'[' => info.kind = SyntaxKind::OpenBracketToken,
            b']' => info.kind = SyntaxKind::CloseBracketToken,
            b',' => info.kind = SyntaxKind::CommaToken,
            b'.' => {
                if self.advance_if_matches(c(b'.')) {
                    if self.text_window.peek_char() == c(b'.') {
                        // See documentation in ScanSyntaxToken
                        self.add_cref_error(ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::Str(".")]);
                    }

                    info.kind = SyntaxKind::DotDotToken;
                } else {
                    info.kind = SyntaxKind::DotToken;
                }
            }
            b'?' => info.kind = SyntaxKind::QuestionToken,
            b'&' => info.kind = SyntaxKind::AmpersandToken,
            b'*' => info.kind = SyntaxKind::AsteriskToken,
            b'|' => info.kind = SyntaxKind::BarToken,
            b'^' => info.kind = SyntaxKind::CaretToken,
            b'%' => info.kind = SyntaxKind::PercentToken,
            b'/' => info.kind = SyntaxKind::SlashToken,
            b'~' => info.kind = SyntaxKind::TildeToken,

            // NOTE: Special case - convert curly brackets into angle brackets.
            b'{' => info.kind = SyntaxKind::LessThanToken,
            b'}' => info.kind = SyntaxKind::GreaterThanToken,

            //// Multi-Character Punctuation/Operators ////
            b':' => {
                info.kind =
                    if self.advance_if_matches(c(b':')) { SyntaxKind::ColonColonToken } else { SyntaxKind::ColonToken }
            }
            b'=' => {
                info.kind = if self.advance_if_matches(c(b'=')) {
                    SyntaxKind::EqualsEqualsToken
                } else {
                    SyntaxKind::EqualsToken
                }
            }
            b'!' => {
                info.kind = if self.advance_if_matches(c(b'=')) {
                    SyntaxKind::ExclamationEqualsToken
                } else {
                    SyntaxKind::ExclamationToken
                }
            }
            b'>' => {
                // GreaterThanGreaterThanToken/GreaterThanGreaterThanGreaterThanToken is synthesized in the parser
                // since it is ambiguous (with closing nested type parameter lists)
                info.kind = if self.advance_if_matches(c(b'=')) {
                    SyntaxKind::GreaterThanEqualsToken
                } else {
                    SyntaxKind::GreaterThanToken
                }
            }
            b'<' => {
                info.kind = if self.advance_if_matches(c(b'=')) {
                    SyntaxKind::LessThanEqualsToken
                } else if self.advance_if_matches(c(b'<')) {
                    SyntaxKind::LessThanLessThanToken
                } else {
                    SyntaxKind::LessThanToken
                }
            }
            b'+' => {
                info.kind =
                    if self.advance_if_matches(c(b'+')) { SyntaxKind::PlusPlusToken } else { SyntaxKind::PlusToken }
            }
            b'-' => {
                info.kind =
                    if self.advance_if_matches(c(b'-')) { SyntaxKind::MinusMinusToken } else { SyntaxKind::MinusToken }
            }
            _ => {}
        }

        if info.kind != SyntaxKind::None {
            let value_text = crate::facts::get_text(info.kind).as_bytes();
            let actual_text = self.get_lexeme_text();
            if !value_text.is_empty() && actual_text != value_text {
                info.requires_text_for_xml_entity = true;
                let units: Vec<u16> = value_text.iter().map(|&b| b as u16).collect();
                self.set_string_value(info, &units);
            }
        } else {
            // If we didn't match any of the above cases, then we either have an identifier or an unexpected character.

            self.text_window.reset(before_consumed);

            if self.scan_identifier(info) && self.current_lexeme_width() > 0 {
                // ACASEY:  All valid identifier characters should be valid in XML attribute values, but I don't want
                // to add an assert because XML character classification is expensive. check to see if it is an actual
                // keyword NOTE: name attribute values don't respect keywords - everything is an identifier.
                let keyword =
                    if !self.in_xml_name_attribute_value() && !info.is_verbatim && !info.has_identifier_escape_sequence
                    {
                        let value = self.identifier_value_utf8(info);
                        keyword_kind(&value)
                    } else {
                        SyntaxKind::None
                    };
                if keyword != SyntaxKind::None {
                    if crate::facts::is_contextual_keyword(keyword) {
                        info.kind = SyntaxKind::IdentifierToken;
                        info.contextual_kind = keyword;
                        // Don't need to set any special flags to store the original text of an identifier.
                    } else {
                        info.kind = keyword;
                        info.requires_text_for_xml_entity = info.has_value;
                    }
                } else {
                    info.contextual_kind = SyntaxKind::IdentifierToken;
                    info.kind = SyntaxKind::IdentifierToken;
                }
            } else if consumed_char == c(b'@') {
                // Saw '@', but it wasn't followed by an identifier (otherwise ScanIdentifier would have succeeded).
                if self.text_window.peek_char() == c(b'@') {
                    self.text_window.next_char();
                    self.set_string_value(info, &[]); // Can't be null for an identifier.
                } else {
                    self.scan_xml_entity(info);
                }
                info.kind = SyntaxKind::IdentifierToken;
                self.add_error_code(ErrorCode::ERR_ExpectedVerbatimLiteral, &[]);
            } else if self.text_window.peek_char() == c(b'&') {
                self.scan_xml_entity(info);
                info.kind = SyntaxKind::XmlEntityLiteralToken;
                let text = String::from_utf8_lossy(self.get_lexeme_text()).into_owned();
                let (start, len) = self.arena.intern(&text);
                self.add_cref_error(ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::Owned { start, len }]);
            } else {
                let bad = self.text_window.next_char();
                let text = String::from_utf8_lossy(self.get_lexeme_text()).into_owned();

                // If it's valid in XML, then it was unexpected in cref mode. Otherwise, it's just bad XML.
                if matches_production_for_xml_char(bad as u32) {
                    let (start, len) = self.arena.intern(&text);
                    self.add_cref_error(ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::Owned { start, len }]);
                } else {
                    self.add_xml_error(XmlParseErrorCode::XML_InvalidUnicodeChar, &[]);
                }
            }
        }

        info.kind != SyntaxKind::None
    }

    /// An identifier's `StringValue` as UTF-8 (its value when it has one, else its text).
    fn identifier_value_utf8(&self, info: &TokenInfo) -> Vec<u8> {
        if info.has_value {
            String::from_utf16_lossy(&self.value).into_bytes()
        } else {
            self.get_lexeme_text().to_vec()
        }
    }

    /// `AdvanceIfMatches(ch)`: given a character, advance the input if either the character or the corresponding XML
    /// entity appears next in the text window.
    fn advance_if_matches(&mut self, ch: u16) -> bool {
        let peek_ch = self.text_window.peek_char();
        if peek_ch == ch || (peek_ch == c(b'{') && ch == c(b'<')) || (peek_ch == c(b'}') && ch == c(b'>')) {
            self.text_window.advance_char();
            return true;
        }

        if peek_ch == c(b'&') {
            let pos = self.text_window.position();

            let mut next_char = 0;
            let mut next_surrogate = 0;
            if self.try_scan_xml_entity(&mut next_char, &mut next_surrogate)
                && next_char == ch
                && next_surrogate == INVALID_CHARACTER
            {
                return true;
            }

            self.text_window.reset(pos);
        }

        false
    }

    /// `InXmlCrefOrNameAttributeValue`.
    #[inline]
    fn in_xml_cref_or_name_attribute_value(&self) -> bool {
        matches!(
            self.mode.and(LexerMode::MASK_LEX_MODE),
            LexerMode::XML_CREF_QUOTE
                | LexerMode::XML_CREF_DOUBLE_QUOTE
                | LexerMode::XML_NAME_QUOTE
                | LexerMode::XML_NAME_DOUBLE_QUOTE
        )
    }

    /// `InXmlNameAttributeValue`.
    fn in_xml_name_attribute_value(&self) -> bool {
        matches!(self.mode.and(LexerMode::MASK_LEX_MODE), LexerMode::XML_NAME_QUOTE | LexerMode::XML_NAME_DOUBLE_QUOTE)
    }

    /// `AddCrefError(code, args)`: diagnostics that occur within cref attributes need to be wrapped with
    /// ErrorCode.WRN_ErrorOverride.
    fn add_cref_error(&mut self, code: ErrorCode, args: &[DiagArg]) {
        let info = self.make_error_code(code, args);
        self.add_cref_error_info(Some(info));
    }

    /// `AddCrefError(DiagnosticInfo? info)`.
    fn add_cref_error_info(&mut self, info: Option<DiagnosticId>) {
        if let Some(info) = info {
            let code = self.arena.diagnostic(info).code;
            self.add_error_code(
                ErrorCode::WRN_ErrorOverride,
                &[DiagArg::Info(info), DiagArg::Int(code.number() as i64)],
            );
        }
    }

    /// `LexXmlCDataSectionTextToken()`: lexer entry point for LexMode.XmlCDataSectionText.
    fn lex_xml_c_data_section_text_token(&mut self) -> Green {
        let mut info = TokenInfo::default();

        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_c_data_section_text_token(&mut info);
        self.finish_xml_token(&info, start, leading)
    }

    /// `ScanXmlCDataSectionTextToken(ref info)`.
    fn scan_xml_c_data_section_text_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b']' if self.text_window.peek_char_at(1) == c(b']') && self.text_window.peek_char_at(2) == c(b'>') => {
                self.text_window.advance_char_n(3);
                info.kind = SyntaxKind::XmlCDataEndToken;
            }
            b'\r' | b'\n' => self.scan_xml_text_literal_new_line_token(info),
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            }
            _ => {
                if facts::is_new_line(ch) {
                    self.scan_xml_text_literal_new_line_token(info);
                } else {
                    self.scan_xml_c_data_section_text(info);
                    info.kind = SyntaxKind::XmlTextLiteralToken;
                }
            }
        }

        true
    }

    /// `ScanXmlCDataSectionText(ref info)`.
    fn scan_xml_c_data_section_text(&mut self, info: &mut TokenInfo) {
        info.has_value = false;
        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                b']' if self.text_window.peek_char_at(1) == c(b']') && self.text_window.peek_char_at(2) == c(b'>') => {
                    return;
                }
                b'\r' | b'\n' => return,
                _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => return,
                b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                    // we're at the end of the comment, but don't lex it yet.
                    return;
                }
                _ => {
                    if facts::is_new_line(ch) {
                        return;
                    }

                    self.text_window.advance_char();
                }
            }
        }
    }

    /// `LexXmlCommentTextToken()`: lexer entry point for LexMode.XmlCommentText.
    fn lex_xml_comment_text_token(&mut self) -> Green {
        let mut info = TokenInfo::default();

        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_comment_text_token(&mut info);
        self.finish_xml_token(&info, start, leading)
    }

    /// `ScanXmlCommentTextToken(ref info)`.
    fn scan_xml_comment_text_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'-' if self.text_window.peek_char_at(1) == c(b'-') => {
                if self.text_window.peek_char_at(2) == c(b'>') {
                    self.text_window.advance_char_n(3);
                    info.kind = SyntaxKind::XmlCommentEndToken;
                } else {
                    self.text_window.advance_char_n(2);
                    info.kind = SyntaxKind::MinusMinusToken;
                }
            }
            b'\r' | b'\n' => self.scan_xml_text_literal_new_line_token(info),
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            }
            _ => {
                if facts::is_new_line(ch) {
                    self.scan_xml_text_literal_new_line_token(info);
                } else {
                    self.scan_xml_comment_text(info);
                    info.kind = SyntaxKind::XmlTextLiteralToken;
                }
            }
        }

        true
    }

    /// `ScanXmlCommentText(ref info)`.
    fn scan_xml_comment_text(&mut self, info: &mut TokenInfo) {
        info.has_value = false;
        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                b'-' if self.text_window.peek_char_at(1) == c(b'-') => return,
                b'\r' | b'\n' => return,
                _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => return,
                b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                    // we're at the end of the comment, but don't lex it yet.
                    return;
                }
                _ => {
                    if facts::is_new_line(ch) {
                        return;
                    }

                    self.text_window.advance_char();
                }
            }
        }
    }

    /// `LexXmlProcessingInstructionTextToken()`: lexer entry point for LexMode.XmlProcessingInstructionText.
    fn lex_xml_processing_instruction_text_token(&mut self) -> Green {
        let mut info = TokenInfo::default();

        let mut leading = std::mem::take(&mut self.buffers.xml_trivia);
        leading.clear();
        self.lex_xml_doc_comment_leading_trivia(&mut leading);

        self.start();
        let start = self.lexeme_start_position;
        self.scan_xml_processing_instruction_text_token(&mut info);
        self.finish_xml_token(&info, start, leading)
    }

    /// `ScanXmlProcessingInstructionTextToken(ref info)`.
    fn scan_xml_processing_instruction_text_token(&mut self, info: &mut TokenInfo) -> bool {
        debug_assert!(!self.location_is(XmlDocCommentLocation::Start));
        debug_assert!(!self.location_is(XmlDocCommentLocation::Exterior));

        if self.location_is(XmlDocCommentLocation::End) {
            info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            return true;
        }

        let ch = self.text_window.peek_char();
        match ascii(ch) {
            b'?' if self.text_window.peek_char_at(1) == c(b'>') => {
                self.text_window.advance_char_n(2);
                info.kind = SyntaxKind::XmlProcessingInstructionEndToken;
            }
            b'\r' | b'\n' => self.scan_xml_text_literal_new_line_token(info),
            _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => {
                info.kind = SyntaxKind::EndOfDocumentationCommentToken;
            }
            _ => {
                if facts::is_new_line(ch) {
                    self.scan_xml_text_literal_new_line_token(info);
                } else {
                    self.scan_xml_processing_instruction_text(info);
                    info.kind = SyntaxKind::XmlTextLiteralToken;
                }
            }
        }

        true
    }

    /// `ScanXmlProcessingInstructionText(ref info)`.
    fn scan_xml_processing_instruction_text(&mut self, info: &mut TokenInfo) {
        info.has_value = false;
        loop {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                b'?' if self.text_window.peek_char_at(1) == c(b'>') => return,
                b'\r' | b'\n' => return,
                _ if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() => return,
                b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                    // we're at the end of the comment, but don't lex it yet.
                    return;
                }
                _ => {
                    if facts::is_new_line(ch) {
                        return;
                    }

                    self.text_window.advance_char();
                }
            }
        }
    }

    /// `LexXmlDocCommentLeadingTrivia(ref trivia)`: collects XML doc comment exterior trivia, and therefore is a no op
    /// unless we are in the Start or Exterior of an XML doc comment.
    fn lex_xml_doc_comment_leading_trivia(&mut self, trivia: &mut Vec<Trivia>) {
        let start = self.text_window.position();
        self.start();

        if self.location_is(XmlDocCommentLocation::Start) && self.style_is(XmlDocCommentStyle::Delimited) {
            // Read the /** that begins an XML doc comment. Since these are recognized only when the trailing character
            // is not a '*', we wind up in the interior of the doc comment at the end.

            if self.text_window.peek_char() == c(b'/')
                && self.text_window.peek_char_at(1) == c(b'*')
                && self.text_window.peek_char_at(2) == c(b'*')
                && self.text_window.peek_char_at(3) != c(b'*')
            {
                self.text_window.advance_char_n(3);
                let t = self.lexeme_trivia(SyntaxKind::DocumentationCommentExteriorTrivia);
                self.add_trivia(t, trivia);
                self.mutate_location(XmlDocCommentLocation::Interior);
            }
        } else if self.location_is(XmlDocCommentLocation::Start) || self.location_is(XmlDocCommentLocation::Exterior) {
            // We're in the exterior of an XML doc comment and need to eat the beginnings of lines, for single line and
            // delimited comments. We chew up white space until a non-whitespace character, and then make the right
            // decision depending on what kind of comment we're in.

            loop {
                let ch = self.text_window.peek_char();
                match ascii(ch) {
                    b' ' | b'\t' | 0x0B | 0x0C => self.text_window.advance_char(),
                    b'/' if self.style_is(XmlDocCommentStyle::SingleLine)
                        && self.text_window.peek_char_at(1) == c(b'/')
                        && self.text_window.peek_char_at(2) == c(b'/')
                        && self.text_window.peek_char_at(3) != c(b'/') =>
                    {
                        self.text_window.advance_char_n(3);
                        let t = self.lexeme_trivia(SyntaxKind::DocumentationCommentExteriorTrivia);
                        self.add_trivia(t, trivia);
                        self.mutate_location(XmlDocCommentLocation::Interior);
                        return;
                    }
                    b'*' if self.style_is(XmlDocCommentStyle::Delimited) => {
                        while self.text_window.peek_char() == c(b'*') && self.text_window.peek_char_at(1) != c(b'/') {
                            self.text_window.advance_char();
                        }

                        if self.current_lexeme_width() > 0 {
                            let t = self.lexeme_trivia(SyntaxKind::DocumentationCommentExteriorTrivia);
                            self.add_trivia(t, trivia);
                        }

                        // This setup ensures that on the final line of a comment, if we have the string "  */", the
                        // "*/" part is separated from the whitespace and therefore recognizable as the end of the
                        // comment.

                        if self.text_window.peek_char() == c(b'*') && self.text_window.peek_char_at(1) == c(b'/') {
                            let position = self.text_window.position();
                            self.text_window.advance_char_n(2);
                            let t = self.arena.trivia(SyntaxKind::DocumentationCommentExteriorTrivia, position, 2);
                            self.add_trivia(t, trivia);
                            self.mutate_location(XmlDocCommentLocation::End);
                        } else {
                            self.mutate_location(XmlDocCommentLocation::Interior);
                        }

                        return;
                    }
                    _ => {
                        if facts::is_whitespace(ch) {
                            self.text_window.advance_char();
                            continue;
                        }

                        // so here we have something else. if this is a single-line xml doc comment, that means we're
                        // on a line that's no longer a doc comment, so we need to rewind. if we're in a delimited doc
                        // comment, then that means we hit pay dirt and we're back into xml text.

                        if self.style_is(XmlDocCommentStyle::SingleLine) {
                            self.text_window.reset(start);
                            self.mutate_location(XmlDocCommentLocation::End);
                        } else {
                            // XmlDocCommentStyle.Delimited
                            debug_assert!(self.style_is(XmlDocCommentStyle::Delimited));

                            if self.current_lexeme_width() > 0 {
                                let t = self.lexeme_trivia(SyntaxKind::DocumentationCommentExteriorTrivia);
                                self.add_trivia(t, trivia);
                            }
                            self.mutate_location(XmlDocCommentLocation::Interior);
                        }

                        return;
                    }
                }
            }
        } else if !self.location_is(XmlDocCommentLocation::End)
            && self.style_is(XmlDocCommentStyle::Delimited)
            && self.text_window.peek_char() == c(b'*')
            && self.text_window.peek_char_at(1) == c(b'/')
        {
            self.text_window.advance_char_n(2);
            let t = self.lexeme_trivia(SyntaxKind::DocumentationCommentExteriorTrivia);
            self.add_trivia(t, trivia);
            self.mutate_location(XmlDocCommentLocation::End);
        }
    }

    /// `LexXmlDocCommentLeadingTriviaWithWhitespace(ref trivia)`.
    fn lex_xml_doc_comment_leading_trivia_with_whitespace(&mut self, trivia: &mut Vec<Trivia>) {
        loop {
            self.lex_xml_doc_comment_leading_trivia(trivia);

            let ch = self.text_window.peek_char();
            if self.location_is(XmlDocCommentLocation::Interior) && (facts::is_whitespace(ch) || facts::is_new_line(ch))
            {
                self.lex_xml_whitespace_and_new_line_trivia(trivia);
            } else {
                break;
            }
        }
    }

    /// `LexXmlWhitespaceAndNewLineTrivia(ref trivia)`: collects whitespace and new line trivia for XML doc comments.
    /// Does not see XML doc comment exterior trivia, and is a no op unless we are in the interior.
    fn lex_xml_whitespace_and_new_line_trivia(&mut self, trivia: &mut Vec<Trivia>) {
        self.start();
        if self.location_is(XmlDocCommentLocation::Interior) {
            let ch = self.text_window.peek_char();
            match ascii(ch) {
                b' ' | b'\t' | 0x0B | 0x0C => {
                    let t = self.scan_whitespace();
                    self.add_trivia(t, trivia);
                }
                b'\r' | b'\n' => {
                    if let Some(end_of_line) = self.scan_end_of_line() {
                        self.add_trivia(end_of_line, trivia);
                    }
                    self.mutate_location(XmlDocCommentLocation::Exterior);
                }
                b'*' if self.style_is(XmlDocCommentStyle::Delimited) && self.text_window.peek_char_at(1) == c(b'/') => {
                    // we're at the end of the comment, but don't add as trivia here.
                }
                _ => {
                    if facts::is_whitespace(ch) {
                        let t = self.scan_whitespace();
                        self.add_trivia(t, trivia);
                    } else if facts::is_new_line(ch) {
                        if let Some(end_of_line) = self.scan_end_of_line() {
                            self.add_trivia(end_of_line, trivia);
                        }
                        self.mutate_location(XmlDocCommentLocation::Exterior);
                    }
                }
            }
        }
    }

    /// `IsUnicodeEscape()`.
    pub(crate) fn is_unicode_escape(&self) -> bool {
        if self.text_window.peek_char() == c(b'\\') {
            let ch2 = self.text_window.peek_char_at(1);
            if ch2 == c(b'U') || ch2 == c(b'u') {
                return true;
            }
        }

        false
    }

    /// `PeekCharOrUnicodeEscape(out surrogateCharacter)`.
    fn peek_char_or_unicode_escape(&mut self, surrogate_character: &mut u16) -> u16 {
        if self.is_unicode_escape() {
            self.peek_unicode_escape(surrogate_character)
        } else {
            *surrogate_character = INVALID_CHARACTER;
            self.text_window.peek_char()
        }
    }

    /// `PeekUnicodeEscape(out surrogateCharacter)`.
    fn peek_unicode_escape(&mut self, surrogate_character: &mut u16) -> u16 {
        let position = self.text_window.position();

        // if we're peeking, then we don't want to change the position
        let mut info = None;
        let ch = self.scan_unicode_escape(true, surrogate_character, &mut info);
        debug_assert!(info.is_none(), "Never produce a diagnostic while peeking.");
        self.text_window.reset(position);
        ch
    }

    /// `NextCharOrUnicodeEscape(out surrogateCharacter, out info)`.
    fn next_char_or_unicode_escape(&mut self, surrogate_character: &mut u16, info: &mut Option<DiagnosticId>) -> u16 {
        let ch = self.text_window.peek_char();
        debug_assert!(
            ch != INVALID_CHARACTER || !self.text_window.is_really_at_end(),
            "Precondition established by all callers; required for correctness of AdvanceChar() call."
        );
        if ch == c(b'\\') {
            let ch2 = self.text_window.peek_char_at(1);
            if ch2 == c(b'U') || ch2 == c(b'u') {
                return self.scan_unicode_escape(false, surrogate_character, info);
            }
        }

        *surrogate_character = INVALID_CHARACTER;
        *info = None;
        self.text_window.advance_char();
        ch
    }

    /// `NextUnicodeEscape(out surrogateCharacter, out info)`.
    pub(crate) fn next_unicode_escape(
        &mut self,
        surrogate_character: &mut u16,
        info: &mut Option<DiagnosticId>,
    ) -> u16 {
        self.scan_unicode_escape(false, surrogate_character, info)
    }

    /// `ScanUnicodeEscape(peek, out surrogateCharacter, out info)`.
    fn scan_unicode_escape(
        &mut self,
        peek: bool,
        surrogate_character: &mut u16,
        info: &mut Option<DiagnosticId>,
    ) -> u16 {
        *surrogate_character = INVALID_CHARACTER;
        *info = None;

        let start = self.text_window.position();
        let character = self.text_window.peek_char();
        debug_assert!(character == c(b'\\'));
        self.text_window.advance_char();

        let mut character = self.text_window.peek_char();
        if character == c(b'U') {
            let mut uint_char: u32 = 0;

            self.text_window.advance_char();
            if !facts::is_hex_digit(self.text_window.peek_char()) {
                if !peek {
                    *info = Some(self.create_illegal_escape_diagnostic(start));
                }
            } else {
                for _ in 0..8 {
                    character = self.text_window.peek_char();
                    if !facts::is_hex_digit(character) {
                        if !peek {
                            *info = Some(self.create_illegal_escape_diagnostic(start));
                        }

                        break;
                    }

                    uint_char = (uint_char << 4).wrapping_add(facts::hex_value(character));
                    self.text_window.advance_char();
                }

                if uint_char > 0x0010FFFF {
                    if !peek {
                        *info = Some(self.create_illegal_escape_diagnostic(start));
                    }
                } else {
                    character = get_chars_from_utf32(uint_char, surrogate_character);
                }
            }
        } else {
            debug_assert!(character == c(b'u') || character == c(b'x'));

            let mut int_char: u32 = 0;
            self.text_window.advance_char();
            if !facts::is_hex_digit(self.text_window.peek_char()) {
                if !peek {
                    *info = Some(self.create_illegal_escape_diagnostic(start));
                }
            } else {
                for _ in 0..4 {
                    let ch2 = self.text_window.peek_char();
                    if !facts::is_hex_digit(ch2) {
                        if character == c(b'u') && !peek {
                            *info = Some(self.create_illegal_escape_diagnostic(start));
                        }

                        break;
                    }

                    int_char = (int_char << 4) + facts::hex_value(ch2);
                    self.text_window.advance_char();
                }

                character = int_char as u16;
            }
        }

        character
    }

    /// `TryScanXmlEntity(out ch, out surrogate)`: given that the next character is an ampersand ('&'), attempt to
    /// interpret the following characters as an XML entity.  On success, populate the out parameters with the low and
    /// high UTF-16 surrogates for the character represented by the entity. NOTE: Always advances, even on failure.
    pub(crate) fn try_scan_xml_entity(&mut self, ch: &mut u16, surrogate: &mut u16) -> bool {
        debug_assert!(self.text_window.peek_char() == c(b'&'));

        *ch = c(b'&');
        self.text_window.advance_char();

        *surrogate = INVALID_CHARACTER;

        match ascii(self.text_window.peek_char()) {
            b'l' => {
                if self.text_window.advance_if_matches(b"lt;") {
                    *ch = c(b'<');
                    return true;
                }
            }
            b'g' => {
                if self.text_window.advance_if_matches(b"gt;") {
                    *ch = c(b'>');
                    return true;
                }
            }
            b'a' => {
                if self.text_window.advance_if_matches(b"amp;") {
                    *ch = c(b'&');
                    return true;
                } else if self.text_window.advance_if_matches(b"apos;") {
                    *ch = c(b'\'');
                    return true;
                }
            }
            b'q' => {
                if self.text_window.advance_if_matches(b"quot;") {
                    *ch = c(b'"');
                    return true;
                }
            }
            b'#' => {
                self.text_window.advance_char(); //#

                let mut uint_char: u32 = 0;

                if self.text_window.advance_if_matches(b"x") {
                    loop {
                        let digit = self.text_window.peek_char();
                        if !facts::is_hex_digit(digit) {
                            break;
                        }
                        self.text_window.advance_char();

                        // disallow overflow
                        if uint_char <= 0x7FFFFFF {
                            uint_char = (uint_char << 4) + facts::hex_value(digit);
                        } else {
                            return false;
                        }
                    }
                } else {
                    loop {
                        let digit = self.text_window.peek_char();
                        if !facts::is_dec_digit(digit) {
                            break;
                        }
                        self.text_window.advance_char();

                        // disallow overflow
                        if uint_char <= 0x7FFFFFF {
                            uint_char = (uint_char << 3) + (uint_char << 1) + facts::dec_value(digit);
                        } else {
                            return false;
                        }
                    }
                }

                if self.text_window.advance_if_matches(b";") {
                    *ch = get_chars_from_utf32(uint_char, surrogate);
                    return true;
                }
            }
            _ => {}
        }

        false
    }

    /// `CreateIllegalEscapeDiagnostic(start)`.
    pub(crate) fn create_illegal_escape_diagnostic(&mut self, start: u32) -> DiagnosticId {
        let offset = start as i32 - self.lexeme_start_position as i32;
        let width = (self.text_window.position() - start) as i32;
        self.arena.make_diagnostic(offset, width, ErrorCode::ERR_IllegalEscape, &[])
    }
}

/// `MatchesProductionForXmlChar(charValue)`: Char ::= #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] |
/// [#x10000-#x10FFFF] (any Unicode character, excluding the surrogate blocks, FFFE, and FFFF).
fn matches_production_for_xml_char(char_value: u32) -> bool {
    char_value == 0x9
        || char_value == 0xA
        || char_value == 0xD
        || (0x20..=0xD7FF).contains(&char_value)
        || (0xE000..=0xFFFD).contains(&char_value)
        || (0x10000..=0x10FFFF).contains(&char_value)
}

/// `IsXmlNameStartChar(ch)`: `XmlCharType.IsStartNCNameCharXml4e`.
fn is_xml_name_start_char(ch: u16) -> bool {
    xml_char_type::is_start_nc_name_char_xml4e(ch)
}

/// `IsXmlNameChar(ch)`: `XmlCharType.IsNCNameCharXml4e`.
fn is_xml_name_char(ch: u16) -> bool {
    xml_char_type::is_nc_name_char_xml4e(ch)
}

/// `GetCharsFromUtf32(codepoint, out lowSurrogate)`.
pub(crate) fn get_chars_from_utf32(codepoint: u32, low_surrogate: &mut u16) -> u16 {
    if codepoint < 0x00010000 {
        *low_surrogate = INVALID_CHARACTER;
        codepoint as u16
    } else {
        debug_assert!(codepoint > 0x0000FFFF && codepoint <= 0x0010FFFF);
        *low_surrogate = ((codepoint - 0x00010000) % 0x0400 + 0xDC00) as u16;
        ((codepoint - 0x00010000) / 0x0400 + 0xD800) as u16
    }
}

/// `char.IsHighSurrogate`.
#[inline]
pub(crate) fn is_high_surrogate(ch: u16) -> bool {
    (0xD800..=0xDBFF).contains(&ch)
}

/// `char.IsLowSurrogate`.
#[inline]
pub(crate) fn is_low_surrogate(ch: u16) -> bool {
    (0xDC00..=0xDFFF).contains(&ch)
}

/// The UTF-8 length of UTF-16 code units (a lone surrogate, which no identifier holds, counted as 3).
fn utf8_len(units: &[u16]) -> u32 {
    let mut len = 0;
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        len += if u < 0x80 {
            1
        } else if u < 0x800 {
            2
        } else if is_high_surrogate(u) && units.get(i + 1).is_some_and(|&l| is_low_surrogate(l)) {
            i += 1;
            4
        } else {
            3
        };
        i += 1;
    }
    len
}

/// `Int32.TryParse(text, NumberStyles.None, CultureInfo.InvariantCulture)` of ASCII digits: `None` on overflow (or no
/// digit).
pub(crate) fn parse_int32(text: &[u8]) -> Option<i32> {
    if text.is_empty() {
        return None;
    }
    let mut value: i32 = 0;
    for &b in text {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((b - b'0') as i32)?;
    }
    Some(value)
}

/// `GetFeatureAvailabilityDiagnosticInfo(feature, options)`: the "not available" diagnostic of `feature` at `version`
/// (`ERR_FeatureInPreview` with the feature's name, or the version's error with the feature and its required version),
/// `None` when it is available.
pub(crate) fn feature_availability_diagnostic_info(
    feature: MessageID,
    version: LanguageVersion,
) -> Option<(ErrorCode, Vec<DiagArg>)> {
    if version.is_feature_enabled(feature) {
        return None;
    }
    let required = feature.required_version().unwrap_or(LanguageVersion::Preview);
    if required == LanguageVersion::Preview {
        return Some((ErrorCode::ERR_FeatureInPreview, vec![DiagArg::Feature(feature)]));
    }
    let code = version.get_error_code().unwrap_or(ErrorCode::ERR_FeatureInPreview);
    Some((code, vec![DiagArg::Feature(feature), DiagArg::Version(required)]))
}

/// Whether `RealParser.TryParseDouble` (`is_float`: `TryParseFloat`) of a real literal's digits fails: the value,
/// correctly rounded, is infinite. Read as `DecimalFloatingPointString.FromSource` reads it: the mantissa's digits and
/// a decimal exponent, which is +-2^30 when its digits are missing, do not fit an `int` or are above 2^30.
fn real_overflows(builder: &[u16], is_float: bool) -> bool {
    const MAX_EXP: i64 = 1 << 30; // even playing ground
    let digit = |i: usize| builder.get(i).filter(|&&u| (0x30..=0x39).contains(&u)).map(|&u| (u - 0x30) as u8);
    let mut mantissa = String::new();
    let mut exponent: i64 = 0;
    let mut i = 0;
    while digit(i) == Some(0) {
        i += 1;
    }
    let mut skipped_decimals = 0;
    while let Some(d) = digit(i) {
        if d == 0 {
            skipped_decimals += 1;
        } else {
            mantissa.extend(std::iter::repeat_n('0', skipped_decimals));
            skipped_decimals = 0;
            mantissa.push((b'0' + d) as char);
        }
        exponent += 1;
        i += 1;
    }
    if builder.get(i) == Some(&c(b'.')) {
        i += 1;
        while let Some(d) = digit(i) {
            if d == 0 {
                skipped_decimals += 1;
            } else {
                mantissa.extend(std::iter::repeat_n('0', skipped_decimals));
                skipped_decimals = 0;
                mantissa.push((b'0' + d) as char);
            }
            i += 1;
        }
    }
    if matches!(builder.get(i).map(|&u| ascii(u)), Some(b'e' | b'E')) {
        i += 1;
        let mut negative = false;
        if let Some(&sign) = builder.get(i).filter(|&&u| u == c(b'-') || u == c(b'+')) {
            negative = sign == c(b'-');
            i += 1;
        }
        let first_exponent = i;
        let mut magnitude: i64 = 0;
        while let Some(d) = digit(i) {
            magnitude = (magnitude * 10 + d as i64).min(i64::MAX / 20);
            i += 1;
        }
        // `int.TryParse` of the digits and at most MAX_EXP, else +-MAX_EXP
        if i > first_exponent && magnitude <= MAX_EXP {
            exponent += if negative { -magnitude } else { magnitude };
        } else {
            exponent = if negative { -MAX_EXP } else { MAX_EXP };
        }
    }
    if mantissa.is_empty() {
        return false;
    }
    let text = format!("0.{mantissa}e{exponent}");
    if is_float {
        text.parse::<f32>().is_ok_and(f32::is_infinite)
    } else {
        text.parse::<f64>().is_ok_and(f64::is_infinite)
    }
}

/// Whether `decimal.TryParse` of a real literal's digits (`_builder`: digits, `.`, an exponent) fails: its value
/// rounded to 29 significant digits is above `decimal.MaxValue`.
fn decimal_overflows(builder: &[u16]) -> bool {
    const MAX: &[u8; 29] = b"79228162514264337593543950335";
    let mut digits: Vec<u8> = Vec::new();
    // the decimal exponent of the digit after the last integer digit, as digits are read
    let mut integer_digits: i64 = 0;
    let mut seen_point = false;
    let mut i = 0;
    while i < builder.len() {
        let ch = builder[i];
        match ascii(ch) {
            b'0'..=b'9' => {
                let d = ch as u8;
                if digits.is_empty() && d == b'0' {
                    if seen_point {
                        integer_digits -= 1;
                    }
                } else {
                    digits.push(d);
                    if !seen_point {
                        integer_digits += 1;
                    }
                }
            }
            b'.' => seen_point = true,
            b'e' | b'E' => break,
            _ => {}
        }
        i += 1;
    }
    if digits.is_empty() {
        return false;
    }
    // the exponent (saturating: only its size matters)
    let mut exponent: i64 = 0;
    let mut negative = false;
    if i < builder.len() {
        i += 1;
        if i < builder.len() && (builder[i] == c(b'-') || builder[i] == c(b'+')) {
            negative = builder[i] == c(b'-');
            i += 1;
        }
        while i < builder.len() {
            if let 0x30..=0x39 = builder[i] {
                exponent = (exponent * 10 + (builder[i] - c(b'0')) as i64).min(1 << 40);
            }
            i += 1;
        }
    }
    let scale = integer_digits + if negative { -exponent } else { exponent };
    if scale != 29 {
        return scale > 29;
    }
    // 29 integer digits: compare the value rounded to them with the maximum
    let mut head = [b'0'; 29];
    for (j, d) in digits.iter().take(29).enumerate() {
        head[j] = *d;
    }
    match head.cmp(MAX) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        // equal: a rounding up goes past it (the maximum is odd, so a tie rounds up)
        std::cmp::Ordering::Equal => digits.get(29).is_some_and(|&d| d >= b'5'),
    }
}

/// `IsBranchingDirective`: `#if`, `#elif`, `#else` (`BranchingDirectiveTriviaSyntax`).
fn is_branching(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::IfDirectiveTrivia | SyntaxKind::ElifDirectiveTrivia | SyntaxKind::ElseDirectiveTrivia)
}

/// `LexerCache.TryGetKeywordKind(text)`: `SyntaxFacts.GetKeywordKind`, else `GetContextualKeywordKind`, of a text of at
/// most `MaxKeywordLength` characters; `SyntaxKind.None` for an identifier. A perfect hash of the first eight bytes
/// and the length, one candidate compared.
#[inline]
pub(crate) fn keyword_kind(text: &[u8]) -> SyntaxKind {
    let len = text.len();
    if len == 0 || len > MAX_KEYWORD_LENGTH {
        return SyntaxKind::None;
    }
    let slot = KEYWORD_SLOTS[keyword_hash(keyword_key(text))];
    if slot == 0 {
        return SyntaxKind::None;
    }
    let (word, kind) = KEYWORDS[slot as usize - 1];
    if word == text { kind } else { SyntaxKind::None }
}

/// The hash key of a keyword candidate: its first eight bytes and its length.
#[inline(always)]
const fn keyword_key(text: &[u8]) -> u64 {
    let mut head = 0u64;
    let mut i = 0;
    while i < 8 && i < text.len() {
        head |= (text[i] as u64) << (8 * i);
        i += 1;
    }
    head ^ ((text.len() as u64) << 56)
}

const KEYWORD_HASH_BITS: u32 = 10;

#[inline(always)]
const fn keyword_hash(key: u64) -> usize {
    (key.wrapping_mul(0xd31af195367a267f) >> (64 - KEYWORD_HASH_BITS)) as usize
}

/// The reserved keywords (`GetKeywordKind`) and the contextual ones (`GetContextualKeywordKind`); `keyword_tests`
/// checks them against `facts`.
const KEYWORDS: [(&[u8], SyntaxKind); 132] = {
    use SyntaxKind::*;
    [
        (b"bool", BoolKeyword),
        (b"byte", ByteKeyword),
        (b"sbyte", SByteKeyword),
        (b"short", ShortKeyword),
        (b"ushort", UShortKeyword),
        (b"int", IntKeyword),
        (b"uint", UIntKeyword),
        (b"long", LongKeyword),
        (b"ulong", ULongKeyword),
        (b"double", DoubleKeyword),
        (b"float", FloatKeyword),
        (b"decimal", DecimalKeyword),
        (b"string", StringKeyword),
        (b"char", CharKeyword),
        (b"void", VoidKeyword),
        (b"object", ObjectKeyword),
        (b"typeof", TypeOfKeyword),
        (b"sizeof", SizeOfKeyword),
        (b"null", NullKeyword),
        (b"true", TrueKeyword),
        (b"false", FalseKeyword),
        (b"if", IfKeyword),
        (b"else", ElseKeyword),
        (b"while", WhileKeyword),
        (b"for", ForKeyword),
        (b"foreach", ForEachKeyword),
        (b"do", DoKeyword),
        (b"switch", SwitchKeyword),
        (b"case", CaseKeyword),
        (b"default", DefaultKeyword),
        (b"lock", LockKeyword),
        (b"try", TryKeyword),
        (b"throw", ThrowKeyword),
        (b"catch", CatchKeyword),
        (b"finally", FinallyKeyword),
        (b"goto", GotoKeyword),
        (b"break", BreakKeyword),
        (b"continue", ContinueKeyword),
        (b"return", ReturnKeyword),
        (b"public", PublicKeyword),
        (b"private", PrivateKeyword),
        (b"internal", InternalKeyword),
        (b"protected", ProtectedKeyword),
        (b"static", StaticKeyword),
        (b"readonly", ReadOnlyKeyword),
        (b"sealed", SealedKeyword),
        (b"const", ConstKeyword),
        (b"fixed", FixedKeyword),
        (b"stackalloc", StackAllocKeyword),
        (b"volatile", VolatileKeyword),
        (b"new", NewKeyword),
        (b"override", OverrideKeyword),
        (b"abstract", AbstractKeyword),
        (b"virtual", VirtualKeyword),
        (b"event", EventKeyword),
        (b"extern", ExternKeyword),
        (b"ref", RefKeyword),
        (b"out", OutKeyword),
        (b"in", InKeyword),
        (b"is", IsKeyword),
        (b"as", AsKeyword),
        (b"params", ParamsKeyword),
        (b"__arglist", ArgListKeyword),
        (b"__makeref", MakeRefKeyword),
        (b"__reftype", RefTypeKeyword),
        (b"__refvalue", RefValueKeyword),
        (b"this", ThisKeyword),
        (b"base", BaseKeyword),
        (b"namespace", NamespaceKeyword),
        (b"using", UsingKeyword),
        (b"class", ClassKeyword),
        (b"struct", StructKeyword),
        (b"interface", InterfaceKeyword),
        (b"enum", EnumKeyword),
        (b"delegate", DelegateKeyword),
        (b"checked", CheckedKeyword),
        (b"unchecked", UncheckedKeyword),
        (b"unsafe", UnsafeKeyword),
        (b"operator", OperatorKeyword),
        (b"implicit", ImplicitKeyword),
        (b"explicit", ExplicitKeyword),
        (b"yield", YieldKeyword),
        (b"partial", PartialKeyword),
        (b"from", FromKeyword),
        (b"group", GroupKeyword),
        (b"join", JoinKeyword),
        (b"into", IntoKeyword),
        (b"let", LetKeyword),
        (b"by", ByKeyword),
        (b"where", WhereKeyword),
        (b"select", SelectKeyword),
        (b"get", GetKeyword),
        (b"set", SetKeyword),
        (b"add", AddKeyword),
        (b"remove", RemoveKeyword),
        (b"orderby", OrderByKeyword),
        (b"alias", AliasKeyword),
        (b"on", OnKeyword),
        (b"equals", EqualsKeyword),
        (b"ascending", AscendingKeyword),
        (b"descending", DescendingKeyword),
        (b"assembly", AssemblyKeyword),
        (b"module", ModuleKeyword),
        (b"type", TypeKeyword),
        (b"field", FieldKeyword),
        (b"method", MethodKeyword),
        (b"param", ParamKeyword),
        (b"property", PropertyKeyword),
        (b"typevar", TypeVarKeyword),
        (b"global", GlobalKeyword),
        (b"async", AsyncKeyword),
        (b"await", AwaitKeyword),
        (b"when", WhenKeyword),
        (b"nameof", NameOfKeyword),
        (b"_", UnderscoreToken),
        (b"var", VarKeyword),
        (b"and", AndKeyword),
        (b"or", OrKeyword),
        (b"not", NotKeyword),
        (b"with", WithKeyword),
        (b"init", InitKeyword),
        (b"record", RecordKeyword),
        (b"managed", ManagedKeyword),
        (b"unmanaged", UnmanagedKeyword),
        (b"required", RequiredKeyword),
        (b"scoped", ScopedKeyword),
        (b"file", FileKeyword),
        (b"allows", AllowsKeyword),
        (b"extension", ExtensionKeyword),
        (b"union", UnionKeyword),
        (b"closed", ClosedKeyword),
        (b"safe", SafeKeyword),
    ]
};

/// `KEYWORDS` by `keyword_hash` (the index + 1; 0 is no keyword). Building it fails to compile on a collision.
const KEYWORD_SLOTS: [u8; 1 << KEYWORD_HASH_BITS] = {
    let mut table = [0u8; 1 << KEYWORD_HASH_BITS];
    let mut i = 0;
    while i < KEYWORDS.len() {
        let slot = keyword_hash(keyword_key(KEYWORDS[i].0));
        assert!(table[slot] == 0, "keyword_hash collides");
        table[slot] = (i + 1) as u8;
        i += 1;
    }
    table
};

#[cfg(test)]
mod keyword_tests {
    use super::*;

    #[test]
    fn keywords_are_roslyn_s() {
        for (text, kind) in KEYWORDS {
            let expected = match crate::facts::get_keyword_kind(text) {
                SyntaxKind::None => crate::facts::get_contextual_keyword_kind(text),
                kind => kind,
            };
            assert_eq!(kind, expected, "{}", String::from_utf8_lossy(text));
            assert_eq!(keyword_kind(text), kind);
        }
        // every keyword kind of `SyntaxFacts` is found by its text
        for kind in crate::facts::get_keyword_kinds() {
            let text = crate::facts::get_text(kind).as_bytes();
            if !text.is_empty() {
                assert_eq!(keyword_kind(text), kind, "{kind:?}");
            }
        }
        assert_eq!(keyword_kind(b"classes"), SyntaxKind::None);
        assert_eq!(keyword_kind(b"Class"), SyntaxKind::None);
    }

    #[test]
    fn decimal_range() {
        let units = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert!(!decimal_overflows(&units("79228162514264337593543950335")));
        assert!(decimal_overflows(&units("79228162514264337593543950336")));
        assert!(decimal_overflows(&units("1e29")));
        assert!(!decimal_overflows(&units("1e28")));
        assert!(!decimal_overflows(&units("0.000001e-100")));
        assert!(!decimal_overflows(&units("0e1000")));
        assert!(real_overflows(&units("1E"), false) && !real_overflows(&units("1E-"), false));
        assert!(!real_overflows(&units("0E"), false) && real_overflows(&units("1e39"), true));
        assert!(!real_overflows(&units("1e308"), false) && real_overflows(&units("1.8e308"), false));
    }
}
