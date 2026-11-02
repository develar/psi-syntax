//! `Lexer_RawStringLiteral.cs`: raw string literals (`"""..."""`, single and multi line) and the character sequences
//! the interpolated and raw string scanners count. The lexer reports no error in a raw string literal: the parser
//! does (`LanguageParser.ParseRawStringToken`).

use super::sliding_text_window::INVALID_CHARACTER;
use super::{Lexer, TokenInfo, c, character_info as facts};
use crate::syntax_kind::SyntaxKind;

impl<'t> Lexer<'t> {
    /// `ConsumeCharSequence(ch)`: the number of `ch` that were consumed.
    fn consume_char_sequence(&mut self, ch: u8) -> u32 {
        let start = self.text_window.position();
        while self.text_window.try_advance(ch) {}

        self.text_window.position() - start
    }

    /// `ConsumeQuoteSequence()`.
    pub(crate) fn consume_quote_sequence(&mut self) -> u32 {
        self.consume_char_sequence(b'"')
    }

    /// `ConsumeDollarSignSequence()`.
    pub(crate) fn consume_dollar_sign_sequence(&mut self) -> u32 {
        self.consume_char_sequence(b'$')
    }

    /// `ConsumeAtSignSequence()`.
    pub(crate) fn consume_at_sign_sequence(&mut self) -> u32 {
        self.consume_char_sequence(b'@')
    }

    /// `ConsumeOpenBraceSequence()`.
    pub(crate) fn consume_open_brace_sequence(&mut self) -> u32 {
        self.consume_char_sequence(b'{')
    }

    /// `ConsumeCloseBraceSequence()`.
    pub(crate) fn consume_close_brace_sequence(&mut self) -> u32 {
        self.consume_char_sequence(b'}')
    }

    /// `ConsumeWhitespace()`.
    pub(crate) fn consume_whitespace(&mut self) {
        loop {
            let ch = self.text_window.peek_char();
            if !facts::is_whitespace(ch) {
                break;
            }

            self.text_window.advance_char();
        }
    }

    /// `IsAtEndOfText(currentChar)`.
    fn is_at_end_of_text(&self, current_char: u16) -> bool {
        current_char == INVALID_CHARACTER && self.text_window.is_really_at_end()
    }

    /// `ScanRawStringLiteral(ref info, inDirective)`.
    pub(crate) fn scan_raw_string_literal(&mut self, info: &mut TokenInfo, in_directive: bool) {
        self.builder.clear();

        let starting_quote_count = self.consume_quote_sequence();

        debug_assert!(starting_quote_count >= 3);

        // Keep consuming whitespace after the initial quote sequence.
        self.consume_whitespace();

        if facts::is_new_line(self.text_window.peek_char()) {
            // Past the initial whitespace, and we hit a newline, this is a multi line raw string literal.
            self.scan_multi_line_raw_string_literal(info, starting_quote_count);
        } else {
            // Past the initial whitespace, and we hit anything else, this is a single line raw string literal.
            self.scan_single_line_raw_string_literal(info, starting_quote_count);
        }

        if !in_directive && self.scan_utf8_suffix() {
            info.kind = match info.kind {
                SyntaxKind::SingleLineRawStringLiteralToken => SyntaxKind::Utf8SingleLineRawStringLiteralToken,
                _ => SyntaxKind::Utf8MultiLineRawStringLiteralToken,
            };
        }

        // Note: we intentionally are not setting .StringValue for raw string literals.  That will be determined in the
        // parser later on in LanguageParser.ParseRawStringToken
        info.has_value = false;
    }

    /// `ScanSingleLineRawStringLiteral(ref info, startingQuoteCount)`.
    fn scan_single_line_raw_string_literal(&mut self, info: &mut TokenInfo, starting_quote_count: u32) {
        info.kind = SyntaxKind::SingleLineRawStringLiteralToken;

        loop {
            let current_char = self.text_window.peek_char();

            // See if we reached the end of the line or file before hitting the end. Errors about this will be reported
            // by the parser.
            if facts::is_new_line(current_char) || self.is_at_end_of_text(current_char) {
                return;
            }

            if current_char != c(b'"') {
                // anything not a quote sequence just moves it forward.
                self.text_window.advance_char();
                continue;
            }

            let current_quote_count = self.consume_quote_sequence();

            // A raw string literal starting with some number of quotes can contain a quote sequence with fewer quotes.
            if current_quote_count < starting_quote_count {
                continue;
            }

            // We have enough quotes to finish this string at this point.  Errors about excess quotes will be reported
            // in the parser.
            return;
        }
    }

    /// `ScanMultiLineRawStringLiteral(ref info, startingQuoteCount)`.
    fn scan_multi_line_raw_string_literal(&mut self, info: &mut TokenInfo, starting_quote_count: u32) {
        info.kind = SyntaxKind::MultiLineRawStringLiteralToken;

        debug_assert!(facts::is_new_line(self.text_window.peek_char()));
        while self.scan_multi_line_raw_string_literal_line(starting_quote_count) {}
    }

    /// `scanMultiLineRawStringLiteralLine(startingQuoteCount)` (a local function of `ScanMultiLineRawStringLiteral`).
    fn scan_multi_line_raw_string_literal_line(&mut self, starting_quote_count: u32) -> bool {
        self.text_window.advance_past_new_line();

        self.consume_whitespace();

        // After the whitespace see if this is the line that ends the multiline literal.  If so we're done scanning
        // lines.  Errors about this will be reported by the parser.
        if self.consume_quote_sequence() >= starting_quote_count {
            return false;
        }

        // We're not on the terminating line. Consume a normal content line.  Eat to the end of line (or file in the
        // case of errors).
        loop {
            let current_char = self.text_window.peek_char();

            // Check if we have an unterminated raw string. Errors about this will be reported by the parser.
            if self.is_at_end_of_text(current_char) {
                return false;
            }

            if facts::is_new_line(current_char) {
                return true;
            }

            if current_char == c(b'"') {
                // Don't allow a content line to contain a quote sequence that looks like a delimiter (or longer).
                // Errors about this will be reported by the parser.
                if self.consume_quote_sequence() >= starting_quote_count {
                    return false;
                }
            } else {
                self.text_window.advance_char();
            }
        }
    }
}
