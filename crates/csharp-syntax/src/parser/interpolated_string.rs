//! `LanguageParser_InterpolatedString.cs`: interpolated and raw string literals, each interpolation parsed
//! by a nested lexer and parser over its text (lane D). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private LiteralExpressionSyntax ParseRawStringToken()` (LanguageParser_InterpolatedString.cs:16).
    pub(crate) fn parse_raw_string_token(&mut self) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:16
        Default::default()
    }

    /// `private InterpolatedStringExpressionSyntax ParseInterpolatedStringToken()` (LanguageParser_InterpolatedString.cs:90).
    pub(crate) fn parse_interpolated_string_token(&mut self) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:90
        Default::default()
    }

    /// `private InterpolatedStringExpressionSyntax ParseInterpolatedOrRawStringToken( SyntaxToken originalToken, bool isInterpolatedString)` (LanguageParser_InterpolatedString.cs:126).
    pub(crate) fn parse_interpolated_or_raw_string_token(
        &mut self,
        original_token: Green,
        is_interpolated_string: bool,
    ) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:126
        Default::default()
    }

    /// `private static string CharToString(char ch)` (LanguageParser_InterpolatedString.cs:404).
    pub(crate) fn char_to_string(ch: u16) -> String {
        // TODO(port): LanguageParser_InterpolatedString.cs:404
        Default::default()
    }

    /// `private static bool CheckForSpaceDifference( ReadOnlySpan<char> currentLineWhitespace, ReadOnlySpan<char> indentationLineWhitespace, [NotNullWhen(true)] out string? currentLineMessage, [NotNullWhen(true)] out string? indentationLineMessage)` (LanguageParser_InterpolatedString.cs:419).
    pub(crate) fn check_for_space_difference(
        current_line_whitespace: &[u8],
        indentation_line_whitespace: &[u8],
        current_line_message: &mut &str,
        indentation_line_message: &mut &str,
    ) -> bool {
        // TODO(port): LanguageParser_InterpolatedString.cs:419
        Default::default()
    }

    /// `private static SyntaxToken TokenOrMissingToken(GreenNode? leading, SyntaxKind kind, string text, GreenNode? trailing)` (LanguageParser_InterpolatedString.cs:445).
    pub(crate) fn token_or_missing_token(leading: Green, kind: SyntaxKind, text: &str, trailing: Green) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:445
        Default::default()
    }

    /// `private static int SkipWhitespace(ReadOnlySpan<char> text, int currentIndex)` (LanguageParser_InterpolatedString.cs:450).
    pub(crate) fn skip_whitespace(text: &[u8], current_index: i32) -> i32 {
        // TODO(port): LanguageParser_InterpolatedString.cs:450
        Default::default()
    }

    /// `private static int ConsumeRemainingContentThroughNewLine(StringBuilder content, ReadOnlySpan<char> text, int currentIndex)` (LanguageParser_InterpolatedString.cs:457).
    pub(crate) fn consume_remaining_content_through_new_line(
        content: &mut String,
        text: &[u8],
        current_index: i32,
    ) -> i32 {
        // TODO(port): LanguageParser_InterpolatedString.cs:457
        Default::default()
    }

    /// `private static InterpolationSyntax ParseInterpolation( CSharpParseOptions options, string text, Lexer.Interpolation interpolation, Lexer.InterpolatedStringKind kind, bool isInAsyncContext, bool isInFieldKeywordContext)` (LanguageParser_InterpolatedString.cs:486).
    pub(crate) fn parse_interpolation(
        options: Green,
        text: &str,
        interpolation: Interpolation,
        kind: InterpolatedStringKind,
        is_in_async_context: bool,
        is_in_field_keyword_context: bool,
    ) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:486
        Default::default()
    }

    /// `private InterpolationSyntax ParseInterpolation( string text, Lexer.Interpolation interpolation, Lexer.InterpolatedStringKind kind, SyntaxToken openBraceToken)` (LanguageParser_InterpolatedString.cs:518).
    pub(crate) fn parse_interpolation_with_open_brace_token(
        &mut self,
        text: &str,
        interpolation: Interpolation,
        kind: InterpolatedStringKind,
        open_brace_token: Green,
    ) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:518
        Default::default()
    }

    /// `private SyntaxToken MakeInterpolatedStringTextToken(Lexer.InterpolatedStringKind kind, string text)` (LanguageParser_InterpolatedString.cs:579).
    pub(crate) fn make_interpolated_string_text_token(&mut self, kind: InterpolatedStringKind, text: &str) -> Green {
        // TODO(port): LanguageParser_InterpolatedString.cs:579
        Default::default()
    }

    /// `private static DiagnosticInfo[]? MoveDiagnostics(DiagnosticInfo[]? infos, int offset)` (LanguageParser_InterpolatedString.cs:603).
    pub(crate) fn move_diagnostics(infos: &[DiagnosticId], offset: i32) -> Vec<DiagnosticId> {
        // TODO(port): LanguageParser_InterpolatedString.cs:603
        Default::default()
    }
}
