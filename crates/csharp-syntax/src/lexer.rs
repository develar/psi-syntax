//! The lexer the parsers run on: the interface of Roslyn's `Lexer` (`Parser/Lexer.cs`) that `SyntaxParser` and its
//! subclasses call, and, until the lexer port lands, a placeholder behind it that never panics and tiles the text
//! (whitespace and line breaks as trivia, runs of identifier bytes as identifiers, any other byte a bad token), so the
//! pipeline runs end to end.
//!
//! The contract the port keeps (`Lexer.cs`, `AbstractLexer.cs`, `SlidingTextWindow.cs`):
//!
//! - `Lexer::new` / `Lexer::with_window` make a lexer over the file's bytes, or over the window `start..end` of them
//!   (the interpolation lexer of `LanguageParser_InterpolatedString.cs`, `new Lexer(SourceText.From(text), ...)`),
//!   positions always being offsets into the whole file, so tokens of a nested lexer land where they are;
//! - `lex(mode)` is `Lex(LexerMode)`: the next token in `mode` with its leading and trailing trivia, made in the arena
//!   (`GreenArena::create_token`, `trivia`, `structured_trivia`, `trivia_list`; diagnostics with `make_diagnostic` and
//!   `with_diagnostics`); `lex_ref(&mut mode)` is `Lex(ref LexerMode)`, which hands back the lexer-driven bits of the
//!   mode (`MASK_XML_DOC_COMMENT_LOCATION`, `MASK_XML_DOC_COMMENT_STYLE`);
//! - `position()` is `TextWindow.Position`; `reset(position, directives)` is `Reset(position, directives)`;
//!   `directives()` is `Directives`, a `DirectiveStack` the parser keeps per token and hands back on a reset (Roslyn's
//!   immutable `DirectiveStack`; `Copy` here, its storage the lexer's);
//! - `text()` is `TextWindow.Text`, `options()` the parse options; the arena is the lexer's while it lives
//!   (`arena()`), and `into_parts` gives it and the lexer's reusable buffers back to the `Parser`.

use crate::green::{Green, GreenArena, TokenText, Trivia, TriviaList};
use crate::language_version::LanguageVersion;
use crate::syntax_kind::SyntaxKind;
use crate::tree::DocumentationMode;

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

/// Roslyn's `DirectiveStack`: the `#if`/`#region` state after a token, which a parser that re-lexes (a mode change,
/// a reset) hands back to `Lexer::reset`. Opaque to the parsers; its meaning and storage are the lexer's.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct DirectiveStack {
    /// The lexer's handle of the stack's top (`ConsList<Directive>`); 0 is `DirectiveStack.Empty`.
    pub(crate) head: u32,
    /// `_seenAnyIfDirectives`.
    pub(crate) seen_any_if_directives: bool,
}

/// `Lexer.InterpolatedStringKind` (Lexer_StringLiteral.cs): what kind of interpolated string the lexer scanned.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum InterpolatedStringKind {
    /// `$"`.
    #[default]
    Normal,
    /// `$@"` or `@$"`.
    Verbatim,
    /// At least one `$`, then at least three `"`, on one line.
    SingleLineRaw,
    /// At least one `$`, then at least three `"`, over lines.
    MultiLineRaw,
}

/// `Lexer.Interpolation` (Lexer.Interpolation.cs): where an interpolation's braces and format colon are, as offsets
/// into the file (Roslyn's are into the token's text: the parser adds the token's start).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Interpolation {
    pub open_brace_range: std::ops::Range<u32>,
    /// Empty when there is no format.
    pub colon_range: std::ops::Range<u32>,
    pub close_brace_range: std::ops::Range<u32>,
}

impl Interpolation {
    /// `HasColon`.
    pub fn has_colon(&self) -> bool {
        self.colon_range.start != self.colon_range.end
    }
}

/// What a lexer is configured with (`CSharpParseOptions`: no preprocessor symbols, `SourceCodeKind.Regular`).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct LexerOptions {
    pub version: LanguageVersion,
    pub doc_mode: DocumentationMode,
}

/// The lexer's buffers kept by a `Parser` from one file to the next.
#[derive(Default)]
pub struct LexerBuffers {
    trivia: Vec<Trivia>,
}

/// Roslyn's `Lexer` over a window of a file's bytes.
pub struct Lexer<'t> {
    text: &'t [u8],
    /// `TextWindow.Position`: an offset into `text`.
    position: u32,
    /// The window's start and end: the lexer sees `text[start..end]`.
    start: u32,
    end: u32,
    options: LexerOptions,
    directives: DirectiveStack,
    /// `_allowPreprocessorDirectives`.
    allow_preprocessor_directives: bool,
    /// `_interpolationFollowedByColon`.
    interpolation_followed_by_colon: bool,
    arena: GreenArena,
    buffers: LexerBuffers,
}

impl<'t> Lexer<'t> {
    /// `new Lexer(text, options)` over a whole file.
    pub fn new(text: &'t [u8], options: LexerOptions, arena: GreenArena, buffers: LexerBuffers) -> Lexer<'t> {
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
        options: LexerOptions,
        allow_preprocessor_directives: bool,
        interpolation_followed_by_colon: bool,
        arena: GreenArena,
        buffers: LexerBuffers,
    ) -> Lexer<'t> {
        let end = end.min(text.len() as u32);
        Lexer {
            text,
            position: start.min(end),
            start: start.min(end),
            end,
            options,
            directives: DirectiveStack::default(),
            allow_preprocessor_directives,
            interpolation_followed_by_colon,
            arena,
            buffers,
        }
    }

    /// The arena and the reusable buffers, back to the `Parser`.
    pub fn into_parts(self) -> (GreenArena, LexerBuffers) {
        (self.arena, self.buffers)
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
        self.text
    }

    /// The start of the window (0 for a file's lexer).
    #[inline]
    pub fn window_start(&self) -> u32 {
        self.start
    }

    /// The end of the window (`TextWindow.Text.Length` of the window's own text, as an offset into `text()`).
    #[inline]
    pub fn end(&self) -> u32 {
        self.end
    }

    /// `Options`.
    #[inline]
    pub fn options(&self) -> LexerOptions {
        self.options
    }

    /// `Directives`.
    #[inline]
    pub fn directives(&self) -> DirectiveStack {
        self.directives
    }

    /// `TextWindow.Position`.
    #[inline]
    pub fn position(&self) -> u32 {
        self.position
    }

    /// `InterpolationFollowedByColon`.
    pub fn interpolation_followed_by_colon(&self) -> bool {
        self.interpolation_followed_by_colon
    }

    /// `AllowPreprocessorDirectives`.
    pub fn allow_preprocessor_directives(&self) -> bool {
        self.allow_preprocessor_directives
    }

    /// `Reset(position, directives)`.
    pub fn reset(&mut self, position: u32, directives: DirectiveStack) {
        self.position = position.min(self.end);
        self.directives = directives;
    }

    /// `Lex(ref LexerMode)`: the next token; `mode` gets the lexer-driven bits back.
    pub fn lex_ref(&mut self, mode: &mut LexerMode) -> Green {
        self.lex(*mode)
    }

    /// `Lex(LexerMode)`: the next token in `mode`, with its trivia. TODO(port): Lexer.cs:247. The placeholder below
    /// ignores the mode: leading trivia are whitespace and line breaks, a token is a run of ASCII letters, digits,
    /// `_` and bytes above 0x7F (an identifier) or one other byte (a bad token), trailing trivia whitespace up to and
    /// with one line break; at the end an `EndOfFileToken`.
    pub fn lex(&mut self, mode: LexerMode) -> Green {
        let _ = mode;
        let leading = self.placeholder_trivia(false);
        let start = self.position;
        let text = self.text;
        let end = self.end as usize;
        let mut i = start as usize;
        let kind = if i >= end {
            SyntaxKind::EndOfFileToken
        } else if is_placeholder_identifier_byte(text[i]) {
            while i < end && is_placeholder_identifier_byte(text[i]) {
                i += 1;
            }
            SyntaxKind::IdentifierToken
        } else {
            i += 1;
            SyntaxKind::BadToken
        };
        self.position = i as u32;
        let trailing =
            if kind == SyntaxKind::EndOfFileToken { TriviaList::EMPTY } else { self.placeholder_trivia(true) };
        let width = i as u32 - start;
        self.arena.create_token(leading, kind, kind, TokenText::Source { start, width }, trailing)
    }

    fn placeholder_trivia(&mut self, trailing: bool) -> TriviaList {
        let mut items = std::mem::take(&mut self.buffers.trivia);
        items.clear();
        let text = self.text;
        let end = self.end as usize;
        let mut i = self.position as usize;
        while i < end {
            let start = i;
            match text[i] {
                b' ' | b'\t' | 0x0B | 0x0C => {
                    while i < end && matches!(text[i], b' ' | b'\t' | 0x0B | 0x0C) {
                        i += 1;
                    }
                    items.push(self.arena.trivia(SyntaxKind::WhitespaceTrivia, start as u32, (i - start) as u32));
                }
                b'\r' | b'\n' => {
                    i += if text[i] == b'\r' && text.get(i + 1) == Some(&b'\n') && i + 1 < end { 2 } else { 1 };
                    items.push(self.arena.trivia(SyntaxKind::EndOfLineTrivia, start as u32, (i - start) as u32));
                    if trailing {
                        break;
                    }
                }
                _ => break,
            }
        }
        self.position = i as u32;
        let list = self.arena.trivia_list(&items);
        self.buffers.trivia = items;
        list
    }
}

fn is_placeholder_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}
