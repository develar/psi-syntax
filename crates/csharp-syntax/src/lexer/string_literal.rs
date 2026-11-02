//! `Lexer_StringLiteral.cs`: string and character literals, verbatim strings, and the scanner of interpolated and raw
//! string literals (`InterpolatedOrRawStringScanner`), which finds where such a literal ends by matching the braces,
//! brackets, quotes and comments of its holes, and reports the interpolations' braces and colons to the parser.

use std::ops::Range;

use super::sliding_text_window::INVALID_CHARACTER;
use super::{Interpolation, Lexer, MAX_NESTED_SCAN_DEPTH, TokenInfo, ascii, c, character_info as facts};
use crate::error_code::{ErrorCode, MessageID};
use crate::green::{DiagArg, DiagnosticId, Green, GreenArena, TokenText};
use crate::syntax_kind::SyntaxKind;

/// `Lexer.InterpolatedStringKind`: the type of string we are producing (this should be named
/// `InterpolatedOrRawStringKind`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum InterpolatedStringKind {
    /// Normal interpolated string that just starts with `$"`. Not ever produced in the raw-string case.
    #[default]
    Normal,
    /// Verbatim interpolated string that starts with `$@"` or `@$"`. Not ever produced in the raw-string case.
    Verbatim,
    /// Single-line raw or raw-interpolated string that can start with at least one `$`, and then at least three `"`s.
    SingleLineRaw,
    /// Multi-line raw or raw-interpolated string that can start with at least one `$`, and then at least three `"`s.
    MultiLineRaw,
}

impl<'t> Lexer<'t> {
    /// `ScanStringLiteral(ref info, inDirective)`.
    pub(crate) fn scan_string_literal(&mut self, info: &mut TokenInfo, in_directive: bool) {
        let quote_character = self.text_window.peek_char();
        debug_assert!(quote_character == c(b'\'') || quote_character == c(b'"'));

        if self.text_window.peek_char() == c(b'"')
            && self.text_window.peek_char_at(1) == c(b'"')
            && self.text_window.peek_char_at(2) == c(b'"')
        {
            self.scan_raw_string_literal(info, in_directive);
            if in_directive {
                // Reinterpret this as just a string literal so that the directive parser can consume this.
                // But report this is illegal so that the user knows to fix this up to be a normal string.
                info.kind = SyntaxKind::StringLiteralToken;
                self.set_string_value(info, &[]);
                self.add_error_code(ErrorCode::ERR_RawStringNotInDirectives, &[]);
            }
            return;
        }

        self.text_window.advance_char();
        self.builder.clear();

        loop {
            let ch = self.text_window.peek_char();

            // Normal string & char constants can have escapes. Strings in directives cannot.
            if ch == c(b'\\') && !in_directive {
                let mut c2 = INVALID_CHARACTER;
                let ch = self.scan_escape_sequence(&mut c2);
                self.builder.push(ch);
                if c2 != INVALID_CHARACTER {
                    self.builder.push(c2);
                }
            } else if ch == quote_character {
                self.text_window.advance_char();
                break;
            } else if facts::is_new_line(ch) || (ch == INVALID_CHARACTER && self.text_window.is_really_at_end()) {
                //String and character literals can contain any Unicode character. They are not limited to valid UTF-16
                //characters. So if we get the SlidingTextWindow's sentinel value, double check that it was not real
                //user-code contents. This will be rare.
                debug_assert!(self.current_lexeme_width() > 0);
                self.add_error_code(ErrorCode::ERR_NewlineInConst, &[]);
                break;
            } else {
                self.text_window.advance_char();
                self.builder.push(ch);
            }
        }

        if quote_character == c(b'\'') {
            info.kind = SyntaxKind::CharacterLiteralToken;
            if self.builder.len() != 1 {
                let code = if !self.builder.is_empty() {
                    ErrorCode::ERR_TooManyCharsInConst
                } else {
                    ErrorCode::ERR_EmptyCharConst
                };
                self.add_error_code(code, &[]);
            }
            // the value (`CharValue`, `StringValue`) is not kept
        } else {
            if !in_directive && self.scan_utf8_suffix() {
                info.kind = SyntaxKind::Utf8StringLiteralToken;
            } else {
                info.kind = SyntaxKind::StringLiteralToken;
            }

            // the value is kept in a directive only, where the parser reads it (`#pragma checksum`, `#line`)
            if in_directive {
                let builder = std::mem::take(&mut self.builder);
                self.set_string_value(info, &builder);
                self.builder = builder;
            }
        }
    }

    /// `ScanUtf8Suffix()`.
    pub(crate) fn scan_utf8_suffix(&mut self) -> bool {
        if matches!(ascii(self.text_window.peek_char()), b'u' | b'U') && self.text_window.peek_char_at(1) == c(b'8') {
            self.text_window.advance_char_n(2);
            return true;
        }

        false
    }

    /// `ScanEscapeSequence(out surrogateCharacter)`.
    pub(crate) fn scan_escape_sequence(&mut self, surrogate_character: &mut u16) -> u16 {
        let start = self.text_window.position();
        *surrogate_character = INVALID_CHARACTER;
        let ch = self.text_window.next_char();
        debug_assert!(ch == c(b'\\'));

        let ch = self.text_window.next_char();
        match ascii(ch) {
            // escaped characters that translate to themselves
            b'\'' | b'"' | b'\\' => ch,
            // translate escapes as per C# spec 2.4.4.4
            b'0' => 0x0000,
            b'a' => 0x0007,
            b'b' => 0x0008,
            b'e' => {
                if let Some((code, args)) = super::feature_availability_diagnostic_info(
                    MessageID::IDS_FeatureStringEscapeCharacter,
                    self.options().version,
                ) {
                    let position = self.text_window.position();
                    self.add_error_at(start, position - start, code, &args);
                }

                0x001b
            }
            b'f' => 0x000c,
            b'n' => 0x000a,
            b'r' => 0x000d,
            b't' => 0x0009,
            b'v' => 0x000b,
            b'x' | b'u' | b'U' => {
                self.text_window.reset(start);
                let mut error = None;
                let ch = self.next_unicode_escape(surrogate_character, &mut error);
                self.add_error(error);
                ch
            }
            _ => {
                let position = self.text_window.position();
                self.add_error_at(start, position - start, ErrorCode::ERR_IllegalEscape, &[]);
                ch
            }
        }
    }

    /// `ScanVerbatimStringLiteral(ref info)`.
    pub(crate) fn scan_verbatim_string_literal(&mut self, info: &mut TokenInfo) {
        debug_assert!(self.text_window.peek_char() == c(b'@'));
        self.builder.clear();

        let start = self.text_window.position();
        while self.text_window.peek_char() == c(b'@') {
            self.text_window.advance_char();
        }

        if self.text_window.position() - start >= 2 {
            let width = self.text_window.position() - start;
            self.add_error_at(start, width, ErrorCode::ERR_IllegalAtSequence, &[]);
        }

        debug_assert!(self.text_window.peek_char() == c(b'"'));
        self.text_window.advance_char();

        loop {
            let ch = self.text_window.peek_char();
            if ch == c(b'"') {
                self.text_window.advance_char();
                if self.text_window.peek_char() == c(b'"') {
                    // Doubled quote -- skip & put the single quote in the string and keep going.
                    self.text_window.advance_char();
                    continue;
                }

                // otherwise, the string is finished.
                break;
            }

            if ch == INVALID_CHARACTER && self.text_window.is_really_at_end() {
                // Reached the end of the source without finding the end-quote.  Give an error back at the starting
                // point. And finish lexing this string.
                self.add_error_code(ErrorCode::ERR_UnterminatedStringLit, &[]);
                break;
            }

            self.text_window.advance_char();
        }

        if self.scan_utf8_suffix() {
            info.kind = SyntaxKind::Utf8StringLiteralToken;
        } else {
            info.kind = SyntaxKind::StringLiteralToken;
        }
    }

    /// `ScanInterpolatedStringLiteral(ref info)`: a string of one of the forms `$" ... "`, `$@" ... "`, `@$" ... "`,
    /// where the contents contains zero or more sequences `{ STUFF }` where these curly braces delimit STUFF in
    /// expression "holes". In order to properly find the closing quote of the whole string, we need to locate the
    /// closing brace of each hole, as strings may appear in expressions in the holes. So we need to match up any braces
    /// that appear between them. But in order to do that, we also need to match up any /**/ comments, ' characters
    /// quotes, () parens [] brackets, and "" strings, including interpolated holes in the latter.
    pub(crate) fn scan_interpolated_string_literal(&mut self, info: &mut TokenInfo) {
        let mut error = None;
        let mut kind = InterpolatedStringKind::Normal;
        let mut open_quote_range = 0..0;
        let mut close_quote_range = 0..0;
        self.scan_interpolated_or_raw_string_literal_top(
            info,
            true,
            &mut error,
            &mut kind,
            &mut open_quote_range,
            None,
            &mut close_quote_range,
        );
        self.add_error(error);
    }

    /// `ScanInterpolatedOrRawStringLiteralTop(ref info, isInterpolatedString, out error, out kind, out openQuoteRange,
    /// interpolations, out closeQuoteRange)`. The ranges are offsets into the file.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn scan_interpolated_or_raw_string_literal_top(
        &mut self,
        info: &mut TokenInfo,
        is_interpolated_string: bool,
        error: &mut Option<DiagnosticId>,
        kind: &mut InterpolatedStringKind,
        open_quote_range: &mut Range<u32>,
        interpolations: Option<&mut Vec<Interpolation>>,
        close_quote_range: &mut Range<u32>,
    ) {
        let mut sub_scanner = InterpolatedOrRawStringScanner::new(self, is_interpolated_string);
        sub_scanner.scan_string_literal_top(kind, open_quote_range, interpolations, close_quote_range);
        *error = sub_scanner.error;
        info.kind = SyntaxKind::InterpolatedStringToken;
    }

    /// `RescanInterpolatedString(interpolatedString)`: turn a (parsed) interpolated string nonterminal into an
    /// interpolated string token, its text the node's (`start` is where the node's text starts in the file).
    #[allow(dead_code, reason = "LanguageParser_InterpolatedString.cs calls it (lane D)")]
    pub(crate) fn rescan_interpolated_string(arena: &mut GreenArena, interpolated_string: Green, start: u32) -> Green {
        let width = arena.width(interpolated_string);
        // TODO: scan the contents (perhaps using ScanInterpolatedStringLiteralContents) to reconstruct any lexical
        // errors such as // inside an expression hole
        let first = arena.first_token(interpolated_string);
        let last = arena.last_token(interpolated_string);
        let leading = arena.leading_trivia(first);
        let trailing = arena.trailing_trivia(last);
        arena.create_token(
            leading,
            SyntaxKind::InterpolatedStringToken,
            SyntaxKind::InterpolatedStringToken,
            TokenText::Source { start, width },
            trailing,
        )
    }
}

/// `InterpolatedOrRawStringScanner`: lives on the stack for the lifetime of the lexer/parser recursing to process
/// interpolated strings.
pub(crate) struct InterpolatedOrRawStringScanner<'a, 't> {
    lexer: &'a mut Lexer<'t>,
    is_interpolated_string: bool,
    /// Error encountered while scanning.  If we run into an error, then we'll attempt to stop parsing at the next
    /// potential ending location to prevent compounding the issue.
    pub(crate) error: Option<DiagnosticId>,
}

impl<'a, 't> InterpolatedOrRawStringScanner<'a, 't> {
    pub(crate) fn new(lexer: &'a mut Lexer<'t>, is_interpolated_string: bool) -> Self {
        InterpolatedOrRawStringScanner { lexer, is_interpolated_string, error: None }
    }

    /// `IsAtEnd(kind)`.
    fn is_at_end_kind(&self, kind: InterpolatedStringKind) -> bool {
        self.is_at_end(matches!(kind, InterpolatedStringKind::Verbatim | InterpolatedStringKind::MultiLineRaw))
    }

    /// `IsAtEnd(allowNewline)`.
    fn is_at_end(&self, allow_newline: bool) -> bool {
        let ch = self.lexer.text_window.peek_char();
        (!allow_newline && facts::is_new_line(ch))
            || (ch == INVALID_CHARACTER && self.lexer.text_window.is_really_at_end())
    }

    /// `TrySetError(_lexer.MakeError(position, width, code, args))`: only need to record the first error we hit.
    fn try_set_error_at(&mut self, position: u32, width: u32, code: ErrorCode, args: &[DiagArg]) {
        if self.error.is_none() {
            let error = self.lexer.make_error_at(position, width, code, args);
            self.error = Some(error);
        }
    }

    /// `ScanStringLiteralTop(out kind, out openQuoteRange, interpolations, out closeQuoteRange)`.
    pub(crate) fn scan_string_literal_top(
        &mut self,
        kind: &mut InterpolatedStringKind,
        open_quote_range: &mut Range<u32>,
        interpolations: Option<&mut Vec<Interpolation>>,
        close_quote_range: &mut Range<u32>,
    ) {
        // Scan through the open-quote portion of this literal, determining important information the rest of the
        // scanning needs.
        let start = self.lexer.text_window.position();
        let mut starting_dollar_sign_count = 0;
        let mut starting_quote_count = 0;
        let succeeded = self.scan_open_quote(kind, &mut starting_dollar_sign_count, &mut starting_quote_count);
        debug_assert!(self.lexer.text_window.position() != start);

        *open_quote_range = start..self.lexer.text_window.position();

        if !succeeded {
            // Processing the start of this literal didn't give us enough information to proceed.  Stop now,
            // terminating the string to the furthest point we reached.
            let position = self.lexer.text_window.position();
            *close_quote_range = position..position;
            return;
        }

        self.scan_interpolated_string_literal_contents(
            *kind,
            starting_dollar_sign_count,
            starting_quote_count,
            interpolations,
        );
        self.scan_interpolated_string_literal_end(*kind, starting_quote_count, close_quote_range);
    }

    /// `ScanOpenQuote(out kind, out startingDollarSignCount, out startingQuoteCount)`: `true` if we successfully
    /// processed the open quote range and can proceed to the rest of the literal, `false` if we were not successful
    /// and should stop processing.
    fn scan_open_quote(
        &mut self,
        kind: &mut InterpolatedStringKind,
        starting_dollar_sign_count: &mut u32,
        starting_quote_count: &mut u32,
    ) -> bool {
        // Handles reading the start of the interpolated string literal (up to where the content begins)
        let window = &mut self.lexer.text_window;
        let start = window.position();

        let (c0, c1, c2, c3) =
            (window.peek_char(), window.peek_char_at(1), window.peek_char_at(2), window.peek_char_at(3));
        if (c0 == c(b'$') && c1 == c(b'@') && c2 == c(b'"')) || (c0 == c(b'@') && c1 == c(b'$') && c2 == c(b'"')) {
            // $@" or @$"
            //
            // Note: we do not consider $@""" as the start of raw-string (in error conditions) as that's a legal
            // verbatim string beginning already.

            *kind = InterpolatedStringKind::Verbatim;
            *starting_dollar_sign_count = 1;
            *starting_quote_count = 1;
            window.advance_char_n(3);
            return true;
        }

        if (c0 == c(b'$') && c1 == c(b'"') && c2 != c(b'"'))
            || (c0 == c(b'$') && c1 == c(b'"') && c2 == c(b'"') && c3 != c(b'"'))
        {
            // $"...
            // $""
            // not $"""
            *kind = InterpolatedStringKind::Normal;
            *starting_dollar_sign_count = 1;
            *starting_quote_count = 1;
            window.advance_char_n(2);
            return true;
        }

        // From this point we have either a complete error case that we cannot process further, or a raw literal of
        // some sort.
        let prefix_at_count = self.lexer.consume_at_sign_sequence();
        *starting_dollar_sign_count = self.lexer.consume_dollar_sign_sequence();

        let suffix_at_count = self.lexer.consume_at_sign_sequence();
        *starting_quote_count = self.lexer.consume_quote_sequence();

        let total_at_count = prefix_at_count + suffix_at_count;

        // We should only have gotten here if we had at least two characters that made us think we had an interpolated
        // string. Note that we may enter here on just `@@` or `$$` (without seeing anything else), so we can't put a
        // stricter bound on this here.

        if *starting_quote_count == 0 {
            // We have no quotes at all.  We cannot continue on as we have no quotes, and thus can't even find where the
            // string starts or ends.
            let position = self.lexer.text_window.position();
            self.try_set_error_at(start, position - start, ErrorCode::ERR_StringMustStartWithQuoteCharacter, &[]);
            *kind = if total_at_count == 1 && *starting_dollar_sign_count == 1 {
                InterpolatedStringKind::Verbatim
            } else {
                InterpolatedStringKind::SingleLineRaw
            };
            return false;
        }

        // @-signs with interpolations are always illegal.  Detect these and give a reasonable error message.
        // Continue on if we can.
        if total_at_count > 0 {
            let position = self.lexer.text_window.position();
            self.try_set_error_at(start, position - start, ErrorCode::ERR_IllegalAtSequence, &[]);
        }

        if *starting_quote_count < 3 {
            // 1-2 quotes present.  Not legal.  But we can give a good error message and still proceed.
            let position = self.lexer.text_window.position();
            self.try_set_error_at(
                position - *starting_quote_count,
                *starting_quote_count,
                ErrorCode::ERR_NotEnoughQuotesForRawString,
                &[],
            );
        }

        // Now see if this was a single-line or multi-line raw literal.

        let after_quote_position = self.lexer.text_window.position();
        self.lexer.consume_whitespace();
        if facts::is_new_line(self.lexer.text_window.peek_char()) {
            // We had whitespace followed by a newline.  That section is considered the open-quote section of the
            // literal.
            self.lexer.text_window.advance_past_new_line();
            *kind = InterpolatedStringKind::MultiLineRaw;
        } else {
            // wasn't multi-line, jump back to right after the quotes as what follows is content and not considered
            // part of the open quote.
            self.lexer.text_window.reset(after_quote_position);
            *kind = InterpolatedStringKind::SingleLineRaw;
        }

        true
    }

    /// `ScanInterpolatedStringLiteralEnd(kind, startingQuoteCount, out closeQuoteRange)`.
    fn scan_interpolated_string_literal_end(
        &mut self,
        kind: InterpolatedStringKind,
        starting_quote_count: u32,
        close_quote_range: &mut Range<u32>,
    ) {
        // Handles reading the end of the interpolated string literal (after where the content ends)

        let close_quote_position = self.lexer.text_window.position();

        if matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim) {
            self.scan_normal_or_verbatim_interpolated_string_literal_end(kind);
        } else {
            debug_assert!(matches!(kind, InterpolatedStringKind::SingleLineRaw | InterpolatedStringKind::MultiLineRaw));
            self.scan_raw_interpolated_string_literal_end(kind, starting_quote_count);

            if !self.is_interpolated_string {
                self.lexer.scan_utf8_suffix();
            }
        }

        // Note: this range may be empty.  For example, if we hit the end of a line for a single-line construct, or we
        // hit the end of a file for a multi-line construct.
        *close_quote_range = close_quote_position..self.lexer.text_window.position();
    }

    /// `ScanNormalOrVerbatimInterpolatedStringLiteralEnd(kind)`.
    fn scan_normal_or_verbatim_interpolated_string_literal_end(&mut self, kind: InterpolatedStringKind) {
        debug_assert!(matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim));

        if self.lexer.text_window.peek_char() != c(b'"') {
            // Didn't find a closing quote.  We hit the end of a line (in the normal case) or the end of the file in the
            // normal/verbatim case.
            let position = if self.is_at_end(true) {
                // `Position - 1`: one code unit back
                self.lexer.text_window.previous_position(self.lexer.text_window.position()).unwrap_or(0)
            } else {
                self.lexer.text_window.position()
            };
            let width = self.lexer.text_window.next_position(position) - position;
            self.try_set_error_at(position, width.max(1), ErrorCode::ERR_UnterminatedStringLit, &[]);
        } else {
            // found the closing quote
            self.lexer.text_window.advance_char(); // "
        }
    }

    /// `ScanRawInterpolatedStringLiteralEnd(kind, startingQuoteCount)`.
    fn scan_raw_interpolated_string_literal_end(&mut self, kind: InterpolatedStringKind, starting_quote_count: u32) {
        debug_assert!(matches!(kind, InterpolatedStringKind::SingleLineRaw | InterpolatedStringKind::MultiLineRaw));

        if kind == InterpolatedStringKind::SingleLineRaw {
            if self.lexer.text_window.peek_char() != c(b'"') {
                // Didn't find a closing quote.  We hit the end of a line (in the normal case) or the end of the file in
                // the normal/verbatim case.
                let position = self.lexer.text_window.position();
                let width = if facts::is_new_line(self.lexer.text_window.peek_char()) {
                    self.lexer.text_window.next_position(position) - position
                } else {
                    0
                };
                self.try_set_error_at(position, width, ErrorCode::ERR_UnterminatedRawString, &[]);
            } else {
                let close_quote_count = self.lexer.consume_quote_sequence();

                // We should only hit here if we had enough close quotes to end the string.  If we didn't have enough
                // they should have just have been consumed as content, and we'd hit the 'true' case in this 'if'
                // instead.
                //
                // If we have too many close quotes for this string, report an error on the excess quotes so the user
                // knows how many they need to delete.
                if close_quote_count > starting_quote_count {
                    let excess_quote_count = close_quote_count - starting_quote_count;
                    let position = self.lexer.text_window.position();
                    self.try_set_error_at(
                        position - excess_quote_count,
                        excess_quote_count,
                        ErrorCode::ERR_TooManyQuotesForRawString,
                        &[],
                    );
                }
            }
        } else {
            // A multiline literal might end either because:
            //
            // 1. we hit the end of the file.
            // 2. we hit quotes *after* content on a line.
            // 3. we found the legitimate end to the literal.

            if self.is_at_end_kind(kind) {
                let position = self.lexer.text_window.position();
                self.try_set_error_at(position, 0, ErrorCode::ERR_UnterminatedRawString, &[]);
            } else if self.lexer.text_window.peek_char() == c(b'"') {
                // Don't allow a content line to contain a quote sequence that looks like a delimiter (or longer)
                let close_quote_count = self.lexer.consume_quote_sequence();

                // We must have too many close quotes.  If we had less, they would have just been consumed as content.
                let position = self.lexer.text_window.position();
                self.try_set_error_at(
                    position - close_quote_count,
                    close_quote_count,
                    ErrorCode::ERR_RawStringDelimiterOnOwnLine,
                    &[],
                );
            } else {
                self.lexer.text_window.advance_past_new_line();
                self.lexer.consume_whitespace();

                let close_quote_count = self.lexer.consume_quote_sequence();

                // We should only hit here if we had enough close quotes to end the string.  If we didn't have enough
                // they should have just have been consumed as content, and we'd hit one of the above cases instead.
                if close_quote_count > starting_quote_count {
                    let excess_quote_count = close_quote_count - starting_quote_count;
                    let position = self.lexer.text_window.position();
                    self.try_set_error_at(
                        position - excess_quote_count,
                        excess_quote_count,
                        ErrorCode::ERR_TooManyQuotesForRawString,
                        &[],
                    );
                }
            }
        }
    }

    /// `ScanInterpolatedStringLiteralContents(kind, startingDollarSignCount, startingQuoteCount, interpolations)`.
    fn scan_interpolated_string_literal_contents(
        &mut self,
        kind: InterpolatedStringKind,
        starting_dollar_sign_count: u32,
        starting_quote_count: u32,
        mut interpolations: Option<&mut Vec<Interpolation>>,
    ) {
        // Check for the trivial multi-line raw string literal of the form:
        //
        // $"""
        //  """
        //
        // And give the special message that a content line is required in the literal.
        if self.check_for_illegal_empty_multi_line_raw_string_literal(kind, starting_quote_count) {
            return;
        }

        loop {
            if self.is_at_end_kind(kind) {
                // error: end of line/file before end of string pop out. Error will be reported in
                // ScanInterpolatedStringLiteralEnd
                return;
            }

            if self.is_at_end_of_multi_line_raw_literal(kind, starting_quote_count) {
                return;
            }

            match ascii(self.lexer.text_window.peek_char()) {
                b'"' => {
                    // Depending on the type of string or the escapes involved, this may be the end of the string
                    // literal, or it may just be content.
                    if self.is_end_delimiter_otherwise_consume(kind, starting_quote_count) {
                        return;
                    }
                }
                b'}' if self.is_interpolated_string => {
                    self.handle_close_brace_in_content(kind, starting_dollar_sign_count)
                }
                b'{' if self.is_interpolated_string => {
                    self.handle_open_brace_in_content(kind, starting_dollar_sign_count, interpolations.as_deref_mut())
                }
                b'\\' => {
                    // In a normal interpolated string a backslash starts an escape. In all other interpolated strings
                    // it's just a backslash.
                    if kind == InterpolatedStringKind::Normal {
                        let escape_start = self.lexer.text_window.position();
                        let mut surrogate = INVALID_CHARACTER;
                        let ch = self.lexer.scan_escape_sequence(&mut surrogate);
                        if ch == c(b'{') || ch == c(b'}') {
                            let position = self.lexer.text_window.position();
                            let arg = DiagArg::Str(if ch == c(b'{') { "{" } else { "}" });
                            self.try_set_error_at(
                                escape_start,
                                position - escape_start,
                                ErrorCode::ERR_EscapedCurly,
                                &[arg],
                            );
                        }
                    } else {
                        self.lexer.text_window.advance_char();
                    }
                }
                _ => {
                    // found some other character in the string portion.  Just consume it as content and continue.
                    self.lexer.text_window.advance_char();
                }
            }
        }
    }

    /// `CheckForIllegalEmptyMultiLineRawStringLiteral(kind, startingQuoteCount)`.
    fn check_for_illegal_empty_multi_line_raw_string_literal(
        &mut self,
        kind: InterpolatedStringKind,
        starting_quote_count: u32,
    ) -> bool {
        if kind == InterpolatedStringKind::MultiLineRaw {
            self.lexer.consume_whitespace();
            let before_quotes_position = self.lexer.text_window.position();
            let close_quote_count = self.lexer.consume_quote_sequence();

            if close_quote_count >= starting_quote_count {
                // Found the end of the string.  reset our position so that ScanInterpolatedStringLiteralEnd can
                // consume it.
                let position = self.lexer.text_window.position();
                self.try_set_error_at(
                    position - close_quote_count,
                    close_quote_count,
                    ErrorCode::ERR_RawStringMustContainContent,
                    &[],
                );
                self.lexer.text_window.reset(before_quotes_position);
                return true;
            }
        }

        false
    }

    /// `IsAtEndOfMultiLineRawLiteral(kind, startingQuoteCount)`.
    fn is_at_end_of_multi_line_raw_literal(&mut self, kind: InterpolatedStringKind, starting_quote_count: u32) -> bool {
        if kind == InterpolatedStringKind::MultiLineRaw {
            // A multiline string ends with a newline, whitespace and at least as many quotes as we started with.

            let start_position = self.lexer.text_window.position();
            if facts::is_new_line(self.lexer.text_window.peek_char()) {
                self.lexer.text_window.advance_past_new_line();
                self.lexer.consume_whitespace();
                let close_quote_count = self.lexer.consume_quote_sequence();

                self.lexer.text_window.reset(start_position);

                if close_quote_count >= starting_quote_count {
                    return true;
                }
            }
        }

        false
    }

    /// `IsEndDelimiterOtherwiseConsume(kind, startingQuoteCount)`: `true` if the quote was an end delimiter and lexing
    /// of the contents of the interpolated string literal should stop.  If it was an end delimiter it will not be
    /// consumed.  If it is content and should not terminate the string then it will be consumed by this method.
    fn is_end_delimiter_otherwise_consume(&mut self, kind: InterpolatedStringKind, starting_quote_count: u32) -> bool {
        if matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim) {
            // When recovering from mismatched delimiters, we consume the next sequence of quote characters as the close
            // quote for the interpolated string. In practice this gets us out of trouble in scenarios we've
            // encountered. See, for example, https://github.com/dotnet/roslyn/issues/44789
            if self.recovering_from_runaway_lexing() {
                return true;
            }

            if kind == InterpolatedStringKind::Normal {
                // Was in a normal $"  string, the next " closes us.
                return true;
            }

            debug_assert!(kind == InterpolatedStringKind::Verbatim);
            // In a verbatim string a "" sequence is an escape. Otherwise this terminates us.
            if self.lexer.text_window.peek_char_at(1) != c(b'"') {
                return true;
            }

            // Was just escaped content.  Consume it.
            self.lexer.text_window.advance_char_n(2); // ""
        } else {
            debug_assert!(matches!(kind, InterpolatedStringKind::SingleLineRaw | InterpolatedStringKind::MultiLineRaw));

            let before_quote_position = self.lexer.text_window.position();
            let current_quote_count = self.lexer.consume_quote_sequence();
            if current_quote_count >= starting_quote_count {
                // we saw a long enough sequence of close quotes to finish us.  Move back to before the close quotes and
                // let the caller handle this (including error-ing if there are too many close quotes, or if the close
                // quotes are in the wrong location).
                self.lexer.text_window.reset(before_quote_position);
                return true;
            }
        }

        // otherwise, these were just quotes that we should treat as raw content.
        false
    }

    /// `HandleCloseBraceInContent(kind, startingDollarSignCount)`.
    fn handle_close_brace_in_content(&mut self, kind: InterpolatedStringKind, starting_dollar_sign_count: u32) {
        if matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim) {
            let pos = self.lexer.text_window.position();
            self.lexer.text_window.advance_char(); // }

            // ensure any } characters are doubled up
            if self.lexer.text_window.peek_char() == c(b'}') {
                self.lexer.text_window.advance_char(); // }
            } else {
                self.try_set_error_at(pos, 1, ErrorCode::ERR_UnescapedCurly, &[DiagArg::Str("}")]);
            }
        } else {
            debug_assert!(matches!(kind, InterpolatedStringKind::MultiLineRaw | InterpolatedStringKind::SingleLineRaw));

            // A close quote is normally fine as content in a raw interpolated string literal. However, similar to the
            // rules around quotes, we do not allow a subsequence of braces to be longer than the number of `$`s the
            // literal starts with.  Note: this restriction is only on *content*.  It acceptable to have a sequence of
            // braces be longer, as long as it is part content and also part of an interpolation.  In that case, the
            // content portion must abide by this rule.
            let close_brace_count = self.lexer.consume_close_brace_sequence();
            if close_brace_count >= starting_dollar_sign_count {
                let position = self.lexer.text_window.position();
                self.try_set_error_at(
                    position - close_brace_count,
                    close_brace_count,
                    ErrorCode::ERR_TooManyCloseBracesForRawString,
                    &[],
                );
            }
        }
    }

    /// `HandleOpenBraceInContent(kind, startingDollarSignCount, interpolations)`.
    fn handle_open_brace_in_content(
        &mut self,
        kind: InterpolatedStringKind,
        starting_dollar_sign_count: u32,
        interpolations: Option<&mut Vec<Interpolation>>,
    ) {
        if matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim) {
            self.handle_open_brace_in_normal_or_verbatim_content(kind, interpolations);
        } else {
            self.handle_open_brace_in_raw_content(kind, starting_dollar_sign_count, interpolations);
        }
    }

    /// `HandleOpenBraceInNormalOrVerbatimContent(kind, interpolations)`.
    fn handle_open_brace_in_normal_or_verbatim_content(
        &mut self,
        kind: InterpolatedStringKind,
        interpolations: Option<&mut Vec<Interpolation>>,
    ) {
        debug_assert!(matches!(kind, InterpolatedStringKind::Normal | InterpolatedStringKind::Verbatim));
        if self.lexer.text_window.peek_char_at(1) == c(b'{') {
            self.lexer.text_window.advance_char_n(2); // {{
        } else {
            let open_brace_position = self.lexer.text_window.position();
            self.lexer.text_window.advance_char();
            let mut colon_range = 0..0;
            self.scan_interpolated_string_literal_hole_balanced_text(kind, b'}', true, &mut colon_range);
            let close_brace_position = self.lexer.text_window.position();
            if self.lexer.text_window.peek_char() == c(b'}') {
                self.lexer.text_window.advance_char();
            } else {
                // `openBracePosition - 1`: one code unit back, the `$` or the content before the brace
                let position =
                    self.lexer.text_window.previous_position(open_brace_position).unwrap_or(open_brace_position);
                let width = self.lexer.text_window.next_position(open_brace_position) - position;
                self.try_set_error_at(position, width, ErrorCode::ERR_UnclosedExpressionHole, &[]);
            }

            if let Some(interpolations) = interpolations {
                interpolations.push(Interpolation {
                    open_brace_range: open_brace_position..open_brace_position + 1,
                    colon_range,
                    close_brace_range: close_brace_position..self.lexer.text_window.position(),
                });
            }
        }
    }

    /// `HandleOpenBraceInRawContent(kind, startingDollarSignCount, interpolations)`.
    fn handle_open_brace_in_raw_content(
        &mut self,
        kind: InterpolatedStringKind,
        starting_dollar_sign_count: u32,
        interpolations: Option<&mut Vec<Interpolation>>,
    ) {
        debug_assert!(matches!(kind, InterpolatedStringKind::SingleLineRaw | InterpolatedStringKind::MultiLineRaw));

        // In raw content we are allowed to see up to 2*N-1 open (or close) braces.  For example, if the string literal
        // starts with `$$$"""` then we can see up to `2*3-1 = 5` braces like so `$$$""" {{{{{`.  The inner three braces
        // start the interpolation.  The outer two braces are just content.  This ensures the rule that the content
        // cannot contain a sequence of open or close braces equal to (or longer than) the dollar sequence.
        let before_open_braces_position = self.lexer.text_window.position();
        let open_brace_count = self.lexer.consume_open_brace_sequence();
        if open_brace_count < starting_dollar_sign_count {
            // not enough open braces to matter.  Just treat as content.
            return;
        }

        let after_open_brace_position = self.lexer.text_window.position();
        if open_brace_count >= 2 * starting_dollar_sign_count {
            // Too many open braces.  Report an error on the portion up before the section that counts as the start of
            // the interpolation.
            self.try_set_error_at(
                before_open_braces_position,
                open_brace_count - starting_dollar_sign_count,
                ErrorCode::ERR_TooManyOpenBracesForRawString,
                &[],
            );
        }

        // Now, try to scan the contents of the interpolation.  Ending when we hit a close brace.
        let mut colon_range = 0..0;
        self.scan_interpolated_string_literal_hole_balanced_text(kind, b'}', true, &mut colon_range);

        let before_close_brace_position = self.lexer.text_window.position();
        let close_brace_count = self.lexer.consume_close_brace_sequence();

        if close_brace_count == 0 {
            // Didn't find any close braces.  Report a particular error on the open braces that they are unclosed.
            self.try_set_error_at(
                after_open_brace_position - starting_dollar_sign_count,
                starting_dollar_sign_count,
                ErrorCode::ERR_UnclosedExpressionHole,
                &[],
            );
        } else if close_brace_count < starting_dollar_sign_count {
            // not enough close braces to end the interpolation.  Report here.
            self.try_set_error_at(
                before_open_braces_position,
                open_brace_count - starting_dollar_sign_count,
                ErrorCode::ERR_NotEnoughCloseBracesForRawString,
                &[],
            );
        } else {
            // Only consume up to the minimum number of close braces we need to end the interpolation. Any excess will
            // be consumed in the content consumption pass in ScanInterpolatedStringLiteralContents.
            self.lexer.text_window.reset(before_close_brace_position + starting_dollar_sign_count);
        }

        if let Some(interpolations) = interpolations {
            interpolations.push(Interpolation {
                open_brace_range: (after_open_brace_position - starting_dollar_sign_count)..after_open_brace_position,
                colon_range,
                close_brace_range: before_close_brace_position..self.lexer.text_window.position(),
            });
        }
    }

    /// `ScanFormatSpecifier(kind)`:
    ///
    /// ```text
    /// interpolation_format
    ///     : ':' interpolation_format_character+
    /// ;
    /// interpolation_format_character
    ///     : '<Any character except \" (U+0022), : (U+003A), { (U+007B) and } (U+007D)>'
    /// ;
    /// ```
    fn scan_format_specifier(&mut self, kind: InterpolatedStringKind) {
        debug_assert!(self.lexer.text_window.peek_char() == c(b':'));
        self.lexer.text_window.advance_char();
        loop {
            let ch = self.lexer.text_window.peek_char();
            if ch == c(b'\\') && kind == InterpolatedStringKind::Normal {
                // normal string & char constants can have escapes
                let pos = self.lexer.text_window.position();
                let mut surrogate = INVALID_CHARACTER;
                let ch = self.lexer.scan_escape_sequence(&mut surrogate);
                if ch == c(b'{') || ch == c(b'}') {
                    let arg = DiagArg::Str(if ch == c(b'{') { "{" } else { "}" });
                    self.try_set_error_at(pos, 1, ErrorCode::ERR_EscapedCurly, &[arg]);
                }
            } else if ch == c(b'"') {
                if kind == InterpolatedStringKind::Verbatim && self.lexer.text_window.peek_char_at(1) == c(b'"') {
                    self.lexer.text_window.advance_char_n(2); // ""
                } else {
                    return; // premature end of string! let caller complain about unclosed interpolation
                }
            } else if ch == c(b'{') {
                let position = self.lexer.text_window.position();
                self.try_set_error_at(position, 1, ErrorCode::ERR_UnexpectedCharacter, &[DiagArg::Str("{")]);
                self.lexer.text_window.advance_char();
            } else if ch == c(b'}') {
                return; // end of interpolation
            } else if self.is_at_end(true) {
                return; // premature end; let caller complain
            } else {
                self.lexer.text_window.advance_char();
            }
        }
    }

    /// `ScanInterpolatedStringLiteralHoleBalancedText(kind, endingChar, isHole, out colonRange)`: scan past the hole
    /// inside an interpolated string literal, leaving the current character on the '}' (if any). Past
    /// `MAX_NESTED_SCAN_DEPTH` nested holes and brackets a `$`, `@` or bracket is content (Roslyn recurses on).
    fn scan_interpolated_string_literal_hole_balanced_text(
        &mut self,
        kind: InterpolatedStringKind,
        ending_char: u8,
        is_hole: bool,
        colon_range: &mut Range<u32>,
    ) {
        self.lexer.scan_depth += 1;
        let deep = self.lexer.scan_depth > MAX_NESTED_SCAN_DEPTH;
        *colon_range = 0..0;
        let ending_text = match ending_char {
            b'}' => "}",
            b')' => ")",
            _ => "]",
        };
        loop {
            let ch = self.lexer.text_window.peek_char();

            // Note: within a hole newlines are always allowed.  The restriction on if newlines are allowed or not is
            // only within a text-portion of the interpolated string.
            if self.is_at_end(true) {
                // the caller will complain
                break;
            }

            match ascii(ch) {
                b'#' => {
                    // preprocessor directives not allowed.
                    let position = self.lexer.text_window.position();
                    self.try_set_error_at(position, 1, ErrorCode::ERR_SyntaxError, &[DiagArg::Str(ending_text)]);
                    self.lexer.text_window.advance_char();
                }
                b'$' if !deep => {
                    let mut discarded = TokenInfo::default();
                    if !self.lexer.try_scan_interpolated_string(&mut discarded) {
                        // part of code in the expression hole
                        self.lexer.text_window.advance_char();
                    }
                }
                b':' if is_hole => {
                    // the first colon not nested within matching delimiters is the start of the format string
                    let position = self.lexer.text_window.position();
                    *colon_range = position..position + 1;
                    self.scan_format_specifier(kind);
                    break;
                }
                b'}' | b')' | b']' => {
                    if ch == c(ending_char) {
                        break;
                    }

                    let position = self.lexer.text_window.position();
                    self.try_set_error_at(position, 1, ErrorCode::ERR_SyntaxError, &[DiagArg::Str(ending_text)]);
                    // part of code in the expression hole
                    self.lexer.text_window.advance_char();
                }
                b'"' => {
                    if self.recovering_from_runaway_lexing() {
                        // When recovering from mismatched delimiters, we consume the next quote character as the close
                        // quote for the interpolated string. In practice this gets us out of trouble in scenarios we've
                        // encountered. See, for example, https://github.com/dotnet/roslyn/issues/44789
                        break;
                    }

                    // handle string literal inside an expression hole.
                    self.scan_interpolated_string_literal_nested_string();
                }
                b'\'' => {
                    // handle character literal inside an expression hole.
                    self.scan_interpolated_string_literal_nested_string();
                }
                b'@' if !deep => {
                    let mut discarded = TokenInfo::default();
                    if self.lexer.try_scan_at_string_token(&mut discarded) {
                        continue;
                    }

                    if self.lexer.text_window.peek_char_at(1) == c(b'*') {
                        // Razor comment. Handle for error recovery purposes. The parser will come along later to
                        // retokenize the inside of the string and report a proper error.
                        let mut is_terminated = false;
                        self.lexer.scan_multi_line_comment(&mut is_terminated, b'@');
                        continue;
                    }

                    // Wasn't an @"" or @$"" string.  Just consume this as normal code.
                    self.lexer.text_window.advance_char();
                }
                b'/' => match ascii(self.lexer.text_window.peek_char_at(1)) {
                    b'/' => self.lexer.scan_to_end_of_line(),
                    b'*' => {
                        let mut is_terminated = false;
                        self.lexer.scan_multi_line_comment(&mut is_terminated, b'/');
                    }
                    _ => self.lexer.text_window.advance_char(),
                },
                // TODO: after the colon this has no special meaning.
                b'{' if !deep => self.scan_interpolated_string_literal_hole_bracketed(kind, b'{', b'}'),
                b'(' if !deep => self.scan_interpolated_string_literal_hole_bracketed(kind, b'(', b')'),
                b'[' if !deep => self.scan_interpolated_string_literal_hole_bracketed(kind, b'[', b']'),
                _ => {
                    // part of code in the expression hole
                    self.lexer.text_window.advance_char();
                }
            }
        }
        self.lexer.scan_depth -= 1;
    }

    /// `RecoveringFromRunawayLexing()`: the lexer can run away consuming the rest of the input when delimiters are
    /// mismatched. This is a test for when we are attempting to recover from that situation.  Note that just running
    /// into new lines will not make us think we're in runaway lexing.
    fn recovering_from_runaway_lexing(&self) -> bool {
        self.error.is_some()
    }

    /// `ScanInterpolatedStringLiteralNestedString()`.
    fn scan_interpolated_string_literal_nested_string(&mut self) {
        let mut info = TokenInfo::default();
        self.lexer.scan_string_literal(&mut info, false);
    }

    /// `ScanInterpolatedStringLiteralHoleBracketed(kind, start, end)`.
    fn scan_interpolated_string_literal_hole_bracketed(&mut self, kind: InterpolatedStringKind, start: u8, end: u8) {
        debug_assert!(c(start) == self.lexer.text_window.peek_char());
        self.lexer.text_window.advance_char();
        let mut colon_range = 0..0;
        self.scan_interpolated_string_literal_hole_balanced_text(kind, end, false, &mut colon_range);
        if self.lexer.text_window.peek_char() == c(end) {
            self.lexer.text_window.advance_char();
        } else {
            // an error was given by the caller
        }
    }
}
