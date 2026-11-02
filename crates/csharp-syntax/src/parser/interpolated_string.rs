//! `LanguageParser_InterpolatedString.cs`: interpolated and raw string literals, each interpolation parsed
//! by a nested lexer and parser over its text (lane D). Every method in C# order; see `parser/mod.rs` for the conventions.
//!
//! Roslyn's temporary lexers run over strings cut from the token's text; here they are windows of the file
//! (`Lexer::with_window`), so every range below (`Lexer.Interpolation`'s, the quotes', a text's) is an offset into the
//! file, the tokens they make have the source's text, and their positions in the tree are Roslyn's. The temporary
//! lexers and parsers take the green arena of the parser's lexer for their run and give it back, their buffers come
//! from `InterpolatedStringBuffers` (kept by the lexer, one level of nesting each). A text is indexed in UTF-8 bytes
//! where Roslyn indexes UTF-16 code units; the dedentation walks characters (`char_at`), so its errors are at the same
//! characters.
//!
//! The `ValueText` of the string texts is not kept, as the lexer keeps no string literal's value (no tree shows it):
//! `ParseRawStringToken`'s `getTokenValue` and the content `makeContent` builds are computed only as far as the
//! diagnostics need them. `MakeInterpolatedStringTextToken` lexes a text as a string literal only when that literal
//! could have a diagnostic (an escape of a regular text, a quote of a verbatim one); the others lex without one.
//!
//! Nesting: a hole's parser starts at the hole's parser's `_recursionDepth` plus `INTERPOLATION_RECURSION_DEPTH`, and
//! when it runs out (`ERR_InsufficientStack`) the outer parse is abandoned too, as Roslyn's exception unwinds through
//! every parser of the file.

use std::ops::Range;

use super::*;
use crate::green::{GreenArena, TokenText, TriviaList};
use crate::lexer::character_info as chars;
use crate::lexer::{Lexer, LexerBuffers, TokenInfo};
use crate::syntax_parser::SyntaxParserBuffers;

/// What a hole's parser adds to `_recursionDepth`: a temporary lexer and parser (1.4 KB), their buffers and the
/// frames from one hole to the next are on the stack once per nested interpolation, about 7 KB measured with one
/// expression level between two holes (`$"{$"{...}"}"` 100 000 deep on a 1 MB stack), some ten levels of an
/// expression.
const INTERPOLATION_RECURSION_DEPTH: i32 = 16;

/// The buffers the parser of an interpolated or raw string token reuses, kept by its lexer (`LexerBuffers`).
#[derive(Default)]
pub(crate) struct InterpolatedStringBuffers {
    /// `interpolations` of `ParseInterpolatedOrRawStringToken`.
    interpolations: Vec<Interpolation>,
    /// The buffers of the temporary lexers and parsers of this level (each holds the next level's).
    nested: Vec<NestedBuffers>,
    /// `fakeString` of `MakeInterpolatedStringTextToken`.
    fake_string: Vec<u8>,
    /// `content` of `getContent`.
    content: Vec<u8>,
}

/// A temporary lexer's and parser's buffers.
#[derive(Default)]
struct NestedBuffers {
    lexer: LexerBuffers,
    parser: SyntaxParserBuffers,
    pool: SyntaxListPool,
}

/// The buffers of a temporary lexer or parser over `lexer`'s level.
fn take_nested(lexer: &mut Lexer<'_>) -> NestedBuffers {
    lexer.interpolated_string_buffers().nested.pop().unwrap_or_default()
}

/// `take_nested`'s buffers back.
fn give_nested(lexer: &mut Lexer<'_>, buffers: NestedBuffers) {
    lexer.interpolated_string_buffers().nested.push(buffers);
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private LiteralExpressionSyntax ParseRawStringToken()` (LanguageParser_InterpolatedString.cs:16).
    pub(crate) fn parse_raw_string_token(&mut self) -> Green {
        let original_token = self.eat_token();

        let expression_kind = facts::get_literal_expression(self.arena_ref().kind(original_token));
        debug_assert!(expression_kind != SyntaxKind::None);

        // We want to share as much code as possible with raw-interpolated-strings.  Especially the code for dealing
        // with indentation removal and determining the 'value' of the string.  As such, we will reinterpret this
        // raw string as an interpolated string with no $'s and no holes, and then extract out the content token
        // from that.

        let interpolated_string = self.parse_interpolated_or_raw_string_token(original_token, false);

        // Because there are no actual interpolations, we expect to only see a single text content node containing
        // the interpreted value of the raw string.
        let a = self.base.arena();
        let string_start_token = a.slot(interpolated_string, 0);
        let contents = a.slot(interpolated_string, 1);
        let interpolated_text = if contents.is_list() { a.slot(contents, 0) } else { contents };
        debug_assert!(a.kind(interpolated_text) == SyntaxKind::InterpolatedStringText);

        // `getDiagnostics()`: any diagnostics from the interpolated string as a whole.
        let mut diagnostics: Vec<DiagnosticId> = a.diagnostics(interpolated_string).collect();

        // We may have diagnostics on the InterpolatedStringText node itself, but not on the text token inside it
        // (since we create it, and immediately add it to the InterpolatedStringText node).  If so, move those over.
        // However, move them as they are relative to the text token, and now need to be relative to the start of
        // the token as a whole.
        let text_diagnostics: Vec<DiagnosticId> = a.diagnostics(interpolated_text).collect();
        let start_width = a.width(string_start_token) as i32;
        diagnostics.extend(Self::move_diagnostics(a, &text_diagnostics, start_width));

        // We preserve everything from the original raw token.  Except we use the computed value text from the
        // interpolated text token instead as long as we got no diagnostics for this raw string. (The value is not
        // kept, see the module's documentation: `getTokenValue()`.)
        let final_token = a.with_diagnostics(original_token, &diagnostics);

        self.base.arena().literal_expression(expression_kind, final_token)
    }

    /// `private InterpolatedStringExpressionSyntax ParseInterpolatedStringToken()` (LanguageParser_InterpolatedString.cs:90).
    pub(crate) fn parse_interpolated_string_token(&mut self) -> Green {
        // We don't want to make the scanner stateful (between tokens) if we can possibly avoid it.
        // The approach implemented here is
        //
        // (1) Scan the whole interpolated string literal as a single token. Now the statefulness of
        // the scanner (to match { }'s) is limited to its behavior while scanning a single token.
        //
        // (2) When the parser gets such a token, here, it spins up another scanner / parser on each of
        // the holes and builds a tree for the whole thing (resulting in an InterpolatedStringExpressionSyntax).
        //
        // (3) The parser discards the original token and replaces it with this tree. (In other words,
        // it replaces one token with a different set of tokens that have already been parsed)
        //
        // (4) On an incremental change, we widen the invalidated region to include any enclosing interpolated
        // string nonterminal so that we never reuse tokens inside a changed interpolated string.
        //
        // This has the secondary advantage that it can reasonably be specified.
        //
        // The substitution will end up being invisible to external APIs and clients such as the IDE, as
        // they have no way to ask for the stream of tokens before parsing.

        debug_assert!(self.current_kind() == SyntaxKind::InterpolatedStringToken);
        let original_token = self.eat_token();

        self.parse_interpolated_or_raw_string_token(original_token, true)
    }

    /// `private InterpolatedStringExpressionSyntax ParseInterpolatedOrRawStringToken( SyntaxToken originalToken, bool isInterpolatedString)` (LanguageParser_InterpolatedString.cs:126):
    /// takes the token produced by the lexer for an (raw or regular) interpolated string or non-interpolated raw
    /// string literal and creates an actual parsed `InterpolatedStringExpressionSyntax` for the syntax tree. For an
    /// interpolated string, this will now contain all the holes parsed out as well. For a raw string this will
    /// contain a single `InterpolatedStringTextSyntax` for the contents of the raw string. Its local functions follow
    /// it, `rescan_interpolation` to `get_interpolation_indentation_error`.
    pub(crate) fn parse_interpolated_or_raw_string_token(
        &mut self,
        original_token: Green,
        is_interpolated_string: bool,
    ) -> Green {
        // A lexed token's text is the source's.
        let Some((text_start, text_width)) = self.arena_ref().token_source_range(original_token) else {
            let a = self.base.arena();
            let close = a.missing_token(SyntaxKind::InterpolatedStringEndToken);
            return a.interpolated_string_expression(original_token, Green::NULL, close);
        };
        let original_text = text_start..text_start + text_width;

        // compute the positions of the interpolations in the original string literal, if there was an error or not,
        // and where the open and close quotes can be found.
        let mut interpolations = std::mem::take(&mut self.base.lexer.interpolated_string_buffers().interpolations);
        interpolations.clear();

        let mut kind = InterpolatedStringKind::Normal;
        let mut error = None;
        let mut open_quote_range = 0..0;
        let mut close_quote_range = 0..0;
        self.rescan_interpolation(
            original_text.clone(),
            is_interpolated_string,
            &mut kind,
            &mut error,
            &mut open_quote_range,
            &mut interpolations,
            &mut close_quote_range,
        );

        // Only bother trying to do dedentation if we have a multiline literal without errors.  There's no point
        // trying in the presence of errors as we may not even be able to determine what the dedentation should be.
        let needs_dedentation = kind == InterpolatedStringKind::MultiLineRaw && error.is_none();

        let open_quote = self.get_open_quote(original_token, kind, open_quote_range.clone());
        let content = self.get_content(
            &original_text,
            &interpolations,
            kind,
            needs_dedentation,
            is_interpolated_string,
            &open_quote_range,
            &close_quote_range,
        );
        let close_quote = self.get_close_quote(original_token, kind, close_quote_range);
        let mut result = self.base.arena().interpolated_string_expression(open_quote, content, close_quote);

        if let Some(error) = error {
            result = self.base.arena().with_diagnostics(result, &[error]);
        }

        self.base.lexer.interpolated_string_buffers().interpolations = interpolations;
        result
    }

    /// `void rescanInterpolation(out kind, out error, out openQuoteRange, interpolations, out closeQuoteRange)`: a
    /// temporary lexer over the token's text, no directives.
    #[allow(clippy::too_many_arguments)]
    fn rescan_interpolation(
        &mut self,
        original_text: Range<u32>,
        is_interpolated_string: bool,
        kind: &mut InterpolatedStringKind,
        error: &mut Option<DiagnosticId>,
        open_quote_range: &mut Range<u32>,
        interpolations: &mut Vec<Interpolation>,
        close_quote_range: &mut Range<u32>,
    ) {
        let source = self.source();
        let options = self.options();
        let mut nested = take_nested(self.base.lexer);
        let arena = std::mem::take(&mut self.base.lexer.arena);
        let mut temp_lexer = Lexer::with_window(
            source,
            original_text.start,
            original_text.end,
            options,
            false,
            false,
            arena,
            std::mem::take(&mut nested.lexer),
        );
        let mut info = TokenInfo::default();
        temp_lexer.scan_interpolated_or_raw_string_literal_top(
            &mut info,
            is_interpolated_string,
            error,
            kind,
            open_quote_range,
            Some(interpolations),
            close_quote_range,
        );
        let (arena, lexer_buffers) = temp_lexer.into_parts();
        self.base.lexer.arena = arena;
        nested.lexer = lexer_buffers;
        give_nested(self.base.lexer, nested);

        debug_assert!(
            is_interpolated_string || interpolations.is_empty(),
            "Non-interpolated parsing should never produce interpolations"
        );
    }

    /// `SyntaxToken getOpenQuote()`.
    fn get_open_quote(
        &mut self,
        original_token: Green,
        kind: InterpolatedStringKind,
        open_quote_range: Range<u32>,
    ) -> Green {
        let token_kind = match kind {
            InterpolatedStringKind::Normal => SyntaxKind::InterpolatedStringStartToken,
            InterpolatedStringKind::Verbatim => SyntaxKind::InterpolatedVerbatimStringStartToken,
            InterpolatedStringKind::SingleLineRaw => SyntaxKind::InterpolatedSingleLineRawStringStartToken,
            InterpolatedStringKind::MultiLineRaw => SyntaxKind::InterpolatedMultiLineRawStringStartToken,
        };
        let a = self.base.arena();
        let leading = a.leading_trivia(original_token);
        a.create_token(
            leading,
            token_kind,
            token_kind,
            TokenText::Source { start: open_quote_range.start, width: open_quote_range.end - open_quote_range.start },
            TriviaList::EMPTY,
        )
    }

    /// `SyntaxList<InterpolatedStringContentSyntax> getContent(originalTextSpan)`.
    #[allow(clippy::too_many_arguments)]
    fn get_content(
        &mut self,
        original_text: &Range<u32>,
        interpolations: &[Interpolation],
        kind: InterpolatedStringKind,
        needs_dedentation: bool,
        is_interpolated_string: bool,
        open_quote_range: &Range<u32>,
        close_quote_range: &Range<u32>,
    ) -> Green {
        let mut content = std::mem::take(&mut self.base.lexer.interpolated_string_buffers().content);
        let mut builder = self.pool.allocate();

        let indentation_whitespace =
            if needs_dedentation { self.get_indentation_whitespace(close_quote_range) } else { 0..0 };

        let mut current_content_start = open_quote_range.end;
        for (i, interpolation) in interpolations.iter().enumerate() {
            // Add a token for text preceding the interpolation
            let text = current_content_start..interpolation.open_brace_range.start.max(current_content_start);
            let node = self.make_content(
                &indentation_whitespace,
                &mut content,
                i == 0,
                false,
                text,
                kind,
                needs_dedentation,
                is_interpolated_string,
            );
            if !node.is_null() {
                builder.push(node);
            }

            // Now parse the interpolation itself.
            let (is_in_async, is_in_field_keyword_context) = (self.is_in_async, self.is_in_field_keyword_context);
            let mut interpolation_node =
                self.parse_interpolation(interpolation, kind, is_in_async, is_in_field_keyword_context);

            // Make sure the interpolation starts at the right location.
            if let Some(indentation_error) = self.get_interpolation_indentation_error(
                original_text,
                &indentation_whitespace,
                interpolation,
                needs_dedentation,
            ) {
                interpolation_node = self.base.arena().with_diagnostics(interpolation_node, &[indentation_error]);
            }

            builder.push(interpolation_node);
            current_content_start = interpolation.close_brace_range.end;
        }

        // Add a token for text following the last interpolation
        let text = current_content_start..close_quote_range.start.max(current_content_start);
        let node = self.make_content(
            &indentation_whitespace,
            &mut content,
            interpolations.is_empty(),
            true,
            text,
            kind,
            needs_dedentation,
            is_interpolated_string,
        );
        if !node.is_null() {
            builder.push(node);
        }

        let result = self.base.arena().list(&builder);
        self.pool.free(builder);
        self.base.lexer.interpolated_string_buffers().content = content;
        result
    }

    /// `ReadOnlySpan<char> getIndentationWhitespace(originalTextSpan)`: gets the indentation whitespace from the last
    /// line of a multi-line raw literal.
    fn get_indentation_whitespace(&self, close_quote_range: &Range<u32>) -> Range<u32> {
        // The content we want to create text token out of.  Effectively, what is in the text sections
        // minus leading whitespace.
        let close_quote_text =
            self.source().get(close_quote_range.start as usize..close_quote_range.end as usize).unwrap_or(b"");

        // A multi-line raw interpolation without errors always ends with a new-line, some number of spaces, and
        // the quotes. So it's safe to just pull off the first two characters here to find where the
        // newline-ends.
        let after_new_line = new_line_width(close_quote_text, 0);
        let after_whitespace = Self::skip_whitespace(close_quote_text, after_new_line as i32) as u32;

        debug_assert!(close_quote_text.get(after_whitespace as usize) == Some(&b'"'));
        close_quote_range.start + after_new_line..close_quote_range.start + after_whitespace
    }

    /// `InterpolatedStringContentSyntax? makeContent(indentationWhitespace, content, isFirst, isLast, text)`.
    #[allow(clippy::too_many_arguments)]
    fn make_content(
        &mut self,
        indentation_whitespace: &Range<u32>,
        content: &mut Vec<u8>,
        is_first: bool,
        is_last: bool,
        text_range: Range<u32>,
        kind: InterpolatedStringKind,
        needs_dedentation: bool,
        is_interpolated_string: bool,
    ) -> Green {
        if text_range.is_empty() {
            // For the raw string case, always include an InterpolatedStringText token, even if empty. This
            // allows the caller to uniformly assume there is always at least one text token that it can
            // extract data from.
            if is_interpolated_string {
                return Green::NULL;
            }
            let a = self.base.arena();
            let token = a.create_token(
                TriviaList::EMPTY,
                SyntaxKind::InterpolatedStringTextToken,
                SyntaxKind::InterpolatedStringTextToken,
                TokenText::Source { start: text_range.start, width: 0 },
                TriviaList::EMPTY,
            );
            return a.interpolated_string_text(token);
        }

        // If we're not dedenting then just make a standard interpolated text token.  Also, we can short-circuit
        // if the indentation whitespace is empty (nothing to dedent in that case).
        if !needs_dedentation || indentation_whitespace.is_empty() {
            let token = self.make_interpolated_string_text_token(kind, text_range);
            return self.base.arena().interpolated_string_text(token);
        }

        let source = self.source();
        let text = source.get(text_range.start as usize..text_range.end as usize).unwrap_or(b"");
        let indentation =
            source.get(indentation_whitespace.start as usize..indentation_whitespace.end as usize).unwrap_or(b"");

        content.clear();
        let mut current_index = 0i32;
        let len = text.len() as i32;

        // If we're not processing the first content chunk, then we must be processing a chunk that came after
        // an interpolation.  In that case, we need to consume up through the next newline of that chunk as
        // content that is not subject to dedentation.
        if !is_first {
            current_index = Self::consume_remaining_content_through_new_line(content, text, current_index);
        }

        // We're either the first item, or we consumed up through a newline from the previous line. We're
        // definitely at the start of a new line (or at the end).  Regardless, we want to consume each
        // successive line, making sure its indentation is correct.

        // Consume one line at a time.
        let mut indentation_error: Option<DiagnosticId> = None;
        while current_index < len {
            let line_start_position = current_index;

            // Only bother reporting a single indentation error on a text chunk.
            if indentation_error.is_none() {
                current_index = Self::skip_whitespace(text, current_index);
                let current_line_whitespace = &text[line_start_position as usize..current_index as usize];

                if !current_line_whitespace.starts_with(indentation) {
                    // We have a line where the indentation of that line isn't a prefix of indentation
                    // whitespace.
                    //
                    // If we're not on a blank line then this is bad.  That's a content line that doesn't start
                    // with the indentation whitespace.  If we are on a blank line then it's ok if the whitespace
                    // we do have is a prefix of the indentation whitespace.
                    let is_blank_line = (current_index == len && is_last)
                        || (current_index < len && chars::is_new_line(char_at(text, current_index as usize).0));
                    let is_legal_blank_line = is_blank_line && indentation.starts_with(current_line_whitespace);
                    if !is_legal_blank_line {
                        // Specialized error message if this is a spacing difference.
                        let mut current_line_whitespace_char = String::new();
                        let mut indentation_whitespace_char = String::new();
                        let width = current_index - line_start_position;
                        if Self::check_for_space_difference(
                            current_line_whitespace,
                            indentation,
                            &mut current_line_whitespace_char,
                            &mut indentation_whitespace_char,
                        ) {
                            let a = self.base.arena();
                            let (s1, l1) = a.intern(&current_line_whitespace_char);
                            let (s2, l2) = a.intern(&indentation_whitespace_char);
                            indentation_error = Some(self.make_error(
                                line_start_position,
                                width,
                                ErrorCode::ERR_LineContainsDifferentWhitespace,
                                &[DiagArg::Owned { start: s1, len: l1 }, DiagArg::Owned { start: s2, len: l2 }],
                            ));
                        } else {
                            indentation_error = Some(self.make_error(
                                line_start_position,
                                width,
                                ErrorCode::ERR_LineDoesNotStartWithSameWhitespace,
                                &[],
                            ));
                        }
                    }
                }
            }

            // Skip the leading whitespace that matches the terminator line and add any text after that to our content.
            current_index = current_index.min(line_start_position + indentation.len() as i32);
            current_index = Self::consume_remaining_content_through_new_line(content, text, current_index);
        }

        // if we ran into any errors, don't give this item any special value.  It just has the value of our actual
        // text. (The value, `content` without errors, is not kept: see the module's documentation.)

        // Note: we place errors on the InterpolatedStringText node itself, not on the token.  This is an
        // invariant that higher up callers can depend on.
        let a = self.base.arena();
        let token = a.create_token(
            TriviaList::EMPTY,
            SyntaxKind::InterpolatedStringTextToken,
            SyntaxKind::InterpolatedStringTextToken,
            TokenText::Source { start: text_range.start, width: text_range.end - text_range.start },
            TriviaList::EMPTY,
        );
        let node = a.interpolated_string_text(token);

        match indentation_error {
            Some(error) => a.with_diagnostics(node, &[error]),
            None => node,
        }
    }

    /// `SyntaxToken getCloseQuote()`: a token for the close quote " (even if it was missing).
    fn get_close_quote(
        &mut self,
        original_token: Green,
        kind: InterpolatedStringKind,
        close_quote_range: Range<u32>,
    ) -> Green {
        let token_kind = match kind {
            InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim => SyntaxKind::InterpolatedStringEndToken,
            InterpolatedStringKind::SingleLineRaw | InterpolatedStringKind::MultiLineRaw => {
                SyntaxKind::InterpolatedRawStringEndToken
            }
        };
        let a = self.base.arena();
        let trailing = a.trailing_trivia(original_token);
        Self::token_or_missing_token(a, TriviaList::EMPTY, token_kind, close_quote_range, trailing)
    }

    /// `SyntaxDiagnosticInfo? getInterpolationIndentationError(indentationWhitespace, interpolation)`: if the
    /// interpolation starts on its own line, then it has to have correct indentation whitespace before it, e.g.:
    ///
    /// ```text
    ///      var x = """
    ///          {1 + 1}
    ///          """
    /// ```
    ///
    /// Not:
    ///
    /// ```text
    ///      var x = """
    /// {1 + 1}
    ///          """
    /// ```
    ///
    /// Note: We don't need to check `<space>{1 + 1}` as initial whitespace in text will already be checked in
    /// makeContent. This is only for the case where the interpolation is at the start of a line.
    fn get_interpolation_indentation_error(
        &mut self,
        original_text: &Range<u32>,
        indentation_whitespace: &Range<u32>,
        interpolation: &Interpolation,
        needs_dedentation: bool,
    ) -> Option<DiagnosticId> {
        if needs_dedentation && !indentation_whitespace.is_empty() {
            let open_brace_position = interpolation.open_brace_range.start;
            let source = self.source();
            let text = source.get(original_text.start as usize..original_text.end as usize).unwrap_or(b"");
            let at = open_brace_position.saturating_sub(original_text.start) as usize;
            if at > 0 && chars::is_new_line(char_before(text, at)) {
                // Pass 0 as the offset to give the error on the interpolation brace.
                return Some(self.make_error(0, 1, ErrorCode::ERR_LineDoesNotStartWithSameWhitespace, &[]));
            }
        }

        None
    }

    /// `private static string CharToString(char ch)` (LanguageParser_InterpolatedString.cs:404): converts a whitespace
    /// character to its string representation for error messages.
    pub(crate) fn char_to_string(ch: u16) -> String {
        match ch {
            0x09 => r"\t".to_string(),
            0x0B => r"\v".to_string(),
            0x0C => r"\f".to_string(),
            _ => format!(r"\u{:04x}", ch),
        }
    }

    /// `private static bool CheckForSpaceDifference( ReadOnlySpan<char> currentLineWhitespace, ReadOnlySpan<char> indentationLineWhitespace, [NotNullWhen(true)] out string? currentLineMessage, [NotNullWhen(true)] out string? indentationLineMessage)` (LanguageParser_InterpolatedString.cs:419):
    /// checks if two whitespace sequences differ at a specific character position where both characters are
    /// whitespace but different types (e.g., tab vs space). The texts are UTF-8, compared character by character.
    pub(crate) fn check_for_space_difference(
        current_line_whitespace: &[u8],
        indentation_line_whitespace: &[u8],
        current_line_message: &mut String,
        indentation_line_message: &mut String,
    ) -> bool {
        let (mut i, mut j) = (0usize, 0usize);
        while i < current_line_whitespace.len() && j < indentation_line_whitespace.len() {
            let (current_line_char, n) = char_at(current_line_whitespace, i);
            let (indentation_line_char, m) = char_at(indentation_line_whitespace, j);

            if current_line_char != indentation_line_char
                && chars::is_whitespace(current_line_char)
                && chars::is_whitespace(indentation_line_char)
            {
                *current_line_message = Self::char_to_string(current_line_char);
                *indentation_line_message = Self::char_to_string(indentation_line_char);
                return true;
            }
            i += n;
            j += m;
        }

        current_line_message.clear();
        indentation_line_message.clear();
        false
    }

    /// `private static SyntaxToken TokenOrMissingToken(GreenNode? leading, SyntaxKind kind, string text, GreenNode? trailing)` (LanguageParser_InterpolatedString.cs:445):
    /// the text is a range of the source.
    pub(crate) fn token_or_missing_token(
        a: &mut GreenArena,
        leading: TriviaList,
        kind: SyntaxKind,
        text: Range<u32>,
        trailing: TriviaList,
    ) -> Green {
        if text.is_empty() {
            a.missing_token_with_trivia(leading, kind, trailing)
        } else {
            a.create_token(
                leading,
                kind,
                kind,
                TokenText::Source { start: text.start, width: text.end - text.start },
                trailing,
            )
        }
    }

    /// `private static int SkipWhitespace(ReadOnlySpan<char> text, int currentIndex)` (LanguageParser_InterpolatedString.cs:450).
    pub(crate) fn skip_whitespace(text: &[u8], current_index: i32) -> i32 {
        let mut current_index = current_index.max(0) as usize;
        while current_index < text.len() {
            let (ch, n) = char_at(text, current_index);
            if !chars::is_whitespace(ch) {
                break;
            }
            current_index += n;
        }
        current_index as i32
    }

    /// `private static int ConsumeRemainingContentThroughNewLine(StringBuilder content, ReadOnlySpan<char> text, int currentIndex)` (LanguageParser_InterpolatedString.cs:457).
    pub(crate) fn consume_remaining_content_through_new_line(
        content: &mut Vec<u8>,
        text: &[u8],
        current_index: i32,
    ) -> i32 {
        let start = current_index.max(0) as usize;
        let mut current_index = start;
        while current_index < text.len() {
            let (ch, n) = char_at(text, current_index);
            if !chars::is_new_line(ch) {
                current_index += n;
                continue;
            }

            current_index += new_line_width(text, current_index) as usize;
            break;
        }

        content.extend_from_slice(text.get(start..current_index).unwrap_or(b""));
        current_index as i32
    }

    /// `private static InterpolationSyntax ParseInterpolation( CSharpParseOptions options, string text, Lexer.Interpolation interpolation, Lexer.InterpolatedStringKind kind, bool isInAsyncContext, bool isInFieldKeywordContext)` (LanguageParser_InterpolatedString.cs:486):
    /// a method, as the temporary lexer takes this parser's arena (and its options); the text is the source.
    pub(crate) fn parse_interpolation(
        &mut self,
        interpolation: &Interpolation,
        kind: InterpolatedStringKind,
        is_in_async_context: bool,
        is_in_field_keyword_context: bool,
    ) -> Green {
        // Grab the text from after the { all the way to the start of the } (or the start of the : if present). This
        // will be used to parse out the expression of the interpolation.
        //
        // The parsing of the open brace, close brace and colon is specially handled in ParseInterpolation below.
        let following_range =
            if interpolation.has_colon() { &interpolation.colon_range } else { &interpolation.close_brace_range };
        let expression_text =
            interpolation.open_brace_range.end..following_range.start.max(interpolation.open_brace_range.end);

        let source = self.source();
        let options = self.options();
        let mut nested = take_nested(self.base.lexer);
        let arena = std::mem::take(&mut self.base.lexer.arena);
        let mut temp_lexer = Lexer::with_window(
            source,
            expression_text.start,
            expression_text.end,
            options,
            false,
            interpolation.has_colon(),
            arena,
            std::mem::take(&mut nested.lexer),
        );

        // First grab any trivia right after the {, it will be trailing trivia for the { token.
        let open_token_trailing_trivia = temp_lexer.lex_syntax_trailing_trivia();

        // Now create a parser to actually handle the expression portion of the interpolation
        let (result, exhausted, parser_buffers, pool) = {
            let mut temp_parser = LanguageParser::new(
                &mut temp_lexer,
                LexerMode::SYNTAX,
                std::mem::take(&mut nested.parser),
                std::mem::take(&mut nested.pool),
            );
            // `ParserSyntaxContextResetter(tempParser, isInAsyncContext, isInFieldKeywordContext)` on a parser that is
            // dropped after
            temp_parser.is_in_async = is_in_async_context;
            temp_parser.is_in_field_keyword_context = is_in_field_keyword_context;
            temp_parser.recursion_depth = self.recursion_depth + INTERPOLATION_RECURSION_DEPTH;

            let open_range = &interpolation.open_brace_range;
            let open_brace_token = temp_parser.arena().create_token(
                TriviaList::EMPTY,
                SyntaxKind::OpenBraceToken,
                SyntaxKind::OpenBraceToken,
                TokenText::Source { start: open_range.start, width: open_range.end - open_range.start },
                open_token_trailing_trivia,
            );
            let result = temp_parser.parse_interpolation_with_open_brace_token(interpolation, kind, open_brace_token);
            let exhausted = temp_parser.stack_exhausted_at.is_some();
            let (parser_buffers, pool) = temp_parser.into_buffers();
            (result, exhausted, parser_buffers, pool)
        };
        let (arena, lexer_buffers) = temp_lexer.into_parts();
        self.base.lexer.arena = arena;
        give_nested(self.base.lexer, NestedBuffers { lexer: lexer_buffers, parser: parser_buffers, pool });

        // The temporary parser ran out of depth: Roslyn's `InsufficientExecutionStackException` unwinds this parser too.
        if exhausted && self.stack_exhausted_at.is_none() {
            self.stack_exhausted_at = Some(self.lexer.position());
            self.base.abandon();
        }
        result
    }

    /// `private InterpolationSyntax ParseInterpolation( string text, Lexer.Interpolation interpolation, Lexer.InterpolatedStringKind kind, SyntaxToken openBraceToken)` (LanguageParser_InterpolatedString.cs:518):
    /// the text is the source. Its local functions are inline.
    pub(crate) fn parse_interpolation_with_open_brace_token(
        &mut self,
        interpolation: &Interpolation,
        kind: InterpolatedStringKind,
        open_brace_token: Green,
    ) -> Green {
        // `getExpressionAndAlignment()`
        let expression = self.parse_expression_core();

        let (expression, alignment) = if self.current_kind() != SyntaxKind::CommaToken {
            (self.consume_unexpected_tokens(expression), Green::NULL)
        } else {
            let comma = self.eat_token_kind(SyntaxKind::CommaToken);
            let value = self.parse_expression_core();
            let value = self.consume_unexpected_tokens(value);
            (expression, self.base.arena().interpolation_alignment_clause(comma, value))
        };

        // `getFormatAndCloseBrace()`
        let current = self.current_token();
        let leading = self.arena_ref().leading_trivia(current);
        let (format, close_brace_token) = if interpolation.has_colon() {
            let colon = &interpolation.colon_range;
            let colon_token = self.base.arena().create_token(
                leading,
                SyntaxKind::ColonToken,
                SyntaxKind::ColonToken,
                TokenText::Source { start: colon.start, width: colon.end - colon.start },
                TriviaList::EMPTY,
            );
            let format_text = colon.end..interpolation.close_brace_range.start.max(colon.end);
            let format_string_token = self.make_interpolated_string_text_token(kind, format_text);
            let format = self.base.arena().interpolation_format_clause(colon_token, format_string_token);
            (format, self.get_interpolation_close_token(interpolation, TriviaList::EMPTY))
        } else {
            (Green::NULL, self.get_interpolation_close_token(interpolation, leading))
        };

        self.base.arena().interpolation(open_brace_token, expression, alignment, format, close_brace_token)
    }

    /// `SyntaxToken getInterpolationCloseToken(GreenNode? leading)`.
    fn get_interpolation_close_token(&mut self, interpolation: &Interpolation, leading: TriviaList) -> Green {
        Self::token_or_missing_token(
            self.base.arena(),
            leading,
            SyntaxKind::CloseBraceToken,
            interpolation.close_brace_range.clone(),
            TriviaList::EMPTY,
        )
    }

    /// `private SyntaxToken MakeInterpolatedStringTextToken(Lexer.InterpolatedStringKind kind, string text)` (LanguageParser_InterpolatedString.cs:579):
    /// interpret the given raw text from source (a range of it) as an InterpolatedStringTextToken.
    pub(crate) fn make_interpolated_string_text_token(
        &mut self,
        kind: InterpolatedStringKind,
        text: Range<u32>,
    ) -> Green {
        let mut token = self.base.arena().create_token(
            TriviaList::EMPTY,
            SyntaxKind::InterpolatedStringTextToken,
            SyntaxKind::InterpolatedStringTextToken,
            TokenText::Source { start: text.start, width: text.end - text.start },
            TriviaList::EMPTY,
        );

        // with a raw string, we don't do any interpretation of the content.  Note: removal of indentation is
        // handled already in splitContent
        if matches!(kind, InterpolatedStringKind::SingleLineRaw | InterpolatedStringKind::MultiLineRaw) {
            return token;
        }

        debug_assert!(matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim));

        // For a normal/verbatim piece of content, process the inner content as if it was in a corresponding
        // *non*-interpolated string to get the correct meaning of all the escapes/diagnostics within. (Only a text
        // that could give a diagnostic is lexed, see the module's documentation.)
        let source = self.source();
        let content = source.get(text.start as usize..text.end as usize).unwrap_or(b"");
        let prefix: &[u8] = if kind == InterpolatedStringKind::Verbatim { b"@\"" } else { b"\"" };
        let may_have_diagnostics =
            if kind == InterpolatedStringKind::Verbatim { content.contains(&b'"') } else { content.contains(&b'\\') };
        if !may_have_diagnostics {
            return token;
        }

        let mut fake_string = std::mem::take(&mut self.base.lexer.interpolated_string_buffers().fake_string);
        fake_string.clear();
        fake_string.extend_from_slice(prefix);
        fake_string.extend_from_slice(content);
        fake_string.push(b'"');
        let options = self.options();
        let mut nested = take_nested(self.base.lexer);
        let arena = std::mem::take(&mut self.base.lexer.arena);
        let diagnostics: Vec<DiagnosticId> = {
            let end = fake_string.len() as u32;
            let mut temp_lexer = Lexer::with_window(
                &fake_string,
                0,
                end,
                options,
                false,
                false,
                arena,
                std::mem::take(&mut nested.lexer),
            );
            let fake_token = temp_lexer.lex(LexerMode::SYNTAX);
            debug_assert!(temp_lexer.arena_ref().kind(fake_token) == SyntaxKind::StringLiteralToken);
            let diagnostics = temp_lexer.arena_ref().diagnostics(fake_token).collect();
            let (arena, lexer_buffers) = temp_lexer.into_parts();
            self.base.lexer.arena = arena;
            nested.lexer = lexer_buffers;
            diagnostics
        };
        give_nested(self.base.lexer, nested);
        self.base.lexer.interpolated_string_buffers().fake_string = fake_string;

        if !diagnostics.is_empty() {
            let a = self.base.arena();
            let moved = Self::move_diagnostics(a, &diagnostics, -(prefix.len() as i32));
            token = a.with_diagnostics(token, &moved);
        }

        token
    }

    /// `private static DiagnosticInfo[]? MoveDiagnostics(DiagnosticInfo[]? infos, int offset)` (LanguageParser_InterpolatedString.cs:603):
    /// the copies are made in the arena (empty for none, Roslyn's null).
    pub(crate) fn move_diagnostics(a: &mut GreenArena, infos: &[DiagnosticId], offset: i32) -> Vec<DiagnosticId> {
        infos
            .iter()
            .map(|&info| {
                // This cast should always be safe.  We are only moving diagnostics produced on syntax nodes and tokens.
                let moved = a.diagnostic(info).offset + offset;
                a.diagnostic_with_offset(info, moved)
            })
            .collect()
    }
}

/// The UTF-16 code unit of the character at byte `i` of UTF-8 `text` (`text[currentIndex]`) and the character's length
/// in bytes: a supplementary character (two code units, neither whitespace nor a new line) reads as its high surrogate,
/// a byte that starts no character as U+FFFD of one byte.
fn char_at(text: &[u8], i: usize) -> (u16, usize) {
    let Some(&b) = text.get(i) else { return (0, 1) };
    let n = match b {
        0..=0x7F => return (b as u16, 1),
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return (0xFFFD, 1),
    };
    match text.get(i..i + n).and_then(|bytes| std::str::from_utf8(bytes).ok()).and_then(|s| s.chars().next()) {
        Some(_) if n == 4 => (0xD800, 4),
        Some(ch) => (ch as u32 as u16, n),
        None => (0xFFFD, 1),
    }
}

/// The code unit of the character that ends at byte `end` of `text` (`text[end - 1]`), U+FFFD when no character ends
/// there.
fn char_before(text: &[u8], end: usize) -> u16 {
    let mut start = end.saturating_sub(1);
    while start > 0 && end - start < 4 && text.get(start).is_some_and(|&b| b & 0xC0 == 0x80) {
        start -= 1;
    }
    match char_at(text, start) {
        (ch, n) if start + n == end => ch,
        _ => 0xFFFD,
    }
}

/// `SlidingTextWindow.GetNewLineWidth(text[i], text[i + 1])` in bytes, for the new line character at byte `i`: `\r\n`
/// is two, any other new line its length.
fn new_line_width(text: &[u8], i: usize) -> u32 {
    if text.get(i) == Some(&b'\r') && text.get(i + 1) == Some(&b'\n') {
        return 2;
    }
    char_at(text, i).1 as u32
}
