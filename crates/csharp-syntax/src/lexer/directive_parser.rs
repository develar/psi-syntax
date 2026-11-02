//! `DirectiveParser.cs`: the parser of a preprocessor directive, a `SyntaxParser` in `LexerMode.Directive` over the
//! lexer that met the `#` (`Lexer.ParseDirective` makes one per directive; its token window is the lexer's buffer).
//! `#if`/`#elif` conditions are evaluated against the directives in effect (`_context`): no preprocessor symbol is
//! defined but by `#define` in the file.
//!
//! Recursion: a condition's `!`, parentheses and `==`/`!=` chains recurse (`ParseLogicalNot`, `ParsePrimary`,
//! `ParseEquality`) up to `MAX_NESTED_SCAN_DEPTH` levels; an operand deeper is a missing identifier with
//! `ERR_InvalidPreprocExpr`, and the rest of the line is skipped as `ParseEndOfDirective` skips extra tokens (Roslyn
//! has no limit there).

use super::directives::{DefineState, DirectiveStack};
use super::{Lexer, MAX_NESTED_SCAN_DEPTH, parse_int32};
use crate::error_code::{ErrorCode, MessageID};
use crate::green::{DiagArg, Green, Trivia};
use crate::language_version::LanguageVersion;
use crate::syntax_kind::SyntaxKind;
use crate::syntax_parser::{ParserKind, SyntaxParser, SyntaxParserBuffers};

const MAX_DIRECTIVE_IDENTIFIER_WIDTH: u32 = 128;
const MAX_LINE_VALUE: i32 = 0xfeefed;
const MAX_CHARACTER_VALUE: i32 = 0x10000;

/// `CommonCompiler.GetProductVersion(typeof(CSharpCompiler))` of Roslyn 5.9.0 (the oracle's).
const PRODUCT_VERSION: &str = "5.9.0-1.26357.3 (35d9211b841e7613c1d2f8f5af6d628ace696c4c)";
/// `CommonCompiler.GetAssemblyLocation(typeof(CSharpCompiler))` without the directory, as the oracle prints it.
const ASSEMBLY_PATH: &str = "Microsoft.CodeAnalysis.CSharp.dll";

/// `DirectiveParser`.
pub(crate) struct DirectiveParser<'l, 't> {
    base: SyntaxParser<'l, 't>,
    /// `_context`.
    context: DirectiveStack,
    /// The nesting of the recursive expression parsers.
    depth: u32,
}

impl<'l, 't> std::ops::Deref for DirectiveParser<'l, 't> {
    type Target = SyntaxParser<'l, 't>;

    fn deref(&self) -> &SyntaxParser<'l, 't> {
        &self.base
    }
}

impl<'l, 't> std::ops::DerefMut for DirectiveParser<'l, 't> {
    fn deref_mut(&mut self) -> &mut SyntaxParser<'l, 't> {
        &mut self.base
    }
}

impl<'l, 't> DirectiveParser<'l, 't> {
    /// `DirectiveParser(lexer)` with `ReInitialize(context)`: `LexerMode.Directive`, no mode resets.
    pub(crate) fn new(lexer: &'l mut Lexer<'t>, buffers: SyntaxParserBuffers, context: DirectiveStack) -> Self {
        DirectiveParser {
            base: SyntaxParser::new(
                lexer,
                crate::lexer::LexerMode::DIRECTIVE,
                ParserKind::Directive,
                false,
                false,
                buffers,
            ),
            context,
            depth: 0,
        }
    }

    /// The token window back for reuse.
    pub(crate) fn into_buffers(self) -> SyntaxParserBuffers {
        self.base.into_buffers()
    }

    fn source(&self) -> &'t [u8] {
        self.base.lexer.text()
    }

    /// `ParseDirective(isActive, endIsActive, isFollowingToken)`.
    pub(crate) fn parse_directive(&mut self, is_active: bool, end_is_active: bool, is_following_token: bool) -> Green {
        let hash_position = self.lexer.position();
        let mut hash = self.eat_token_report(SyntaxKind::HashToken, false);

        // The behavior of these directives when isActive is false is somewhat complicated.
        // The key functions in the native compiler are ScanPreprocessorIfSection and
        // ScanAndIgnoreDirective in CSourceData::CPreprocessor.
        // Key points:
        //   1) #error, #warning, #line, and #pragma have no effect and produce no diagnostics.
        //   2) #if, #else, #elif, #endif, #region, and #endregion must still nest correctly.
        //   3) #define and #undef produce diagnostics but have no effect.
        // #reference, #load and #! are new, but they do not require nesting behavior, so we'll
        // ignore their diagnostics (as in (1) above).

        let contextual_kind = self.current_contextual_kind();
        match contextual_kind {
            SyntaxKind::IfKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_if_directive(hash, keyword, is_active)
            }
            SyntaxKind::ElifKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_elif_directive(hash, keyword, is_active, end_is_active)
            }
            SyntaxKind::ElseKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_else_directive(hash, keyword, is_active, end_is_active)
            }
            SyntaxKind::EndIfKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_end_if_directive(hash, keyword, is_active, end_is_active)
            }
            SyntaxKind::RegionKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_region_directive(hash, keyword, is_active)
            }
            SyntaxKind::EndRegionKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_end_region_directive(hash, keyword, is_active)
            }
            SyntaxKind::DefineKeyword | SyntaxKind::UndefKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_define_or_undef_directive(hash, keyword, is_active, is_following_token)
            }
            SyntaxKind::ErrorKeyword | SyntaxKind::WarningKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_error_or_warning_directive(hash, keyword, is_active)
            }
            SyntaxKind::LineKeyword => {
                let line_keyword = self.eat_contextual_token(contextual_kind);
                if self.current_kind() == SyntaxKind::OpenParenToken {
                    self.parse_line_span_directive(hash, line_keyword, is_active)
                } else {
                    self.parse_line_directive(hash, line_keyword, is_active)
                }
            }
            SyntaxKind::PragmaKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_pragma_directive(hash, keyword, is_active)
            }
            SyntaxKind::ReferenceKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_reference_directive(hash, keyword, is_active, is_following_token)
            }
            SyntaxKind::LoadKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_load_directive(hash, keyword, is_active, is_following_token)
            }
            SyntaxKind::NullableKeyword => {
                let keyword = self.eat_contextual_token(contextual_kind);
                self.parse_nullable_directive(hash, keyword, is_active)
            }
            _ => {
                let hash_has_trailing_trivia = !self.arena_ref().trailing_trivia(hash).is_empty();
                if contextual_kind == SyntaxKind::ExclamationToken {
                    // Always parse as a shebang directive, but report an error if not at position 0
                    if hash_position != self.lexer.window_start() || hash_has_trailing_trivia {
                        hash = self.add_error(hash, ErrorCode::ERR_PPShebangNotOnFirstLine);
                    }

                    let exclamation = self.eat_token_kind(SyntaxKind::ExclamationToken);
                    self.parse_shebang_directive(hash, exclamation, is_active)
                } else if contextual_kind == SyntaxKind::ColonToken && !hash_has_trailing_trivia {
                    let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                    self.parse_ignored_directive(hash, colon, is_active, is_following_token)
                } else {
                    let mut id = self.eat_token_report(SyntaxKind::IdentifierToken, false);
                    let end = self.parse_end_of_directive(true, false, false);
                    if !self.arena_ref().is_missing(id) {
                        id = self.add_error(id, ErrorCode::ERR_PPDirectiveExpected);
                    } else {
                        hash = self.add_error(hash, ErrorCode::ERR_PPDirectiveExpected);
                    }

                    self.arena().bad_directive_trivia(hash, id, end, is_active)
                }
            }
        }
    }

    /// `ParseIfDirective(hash, keyword, isActive)`.
    fn parse_if_directive(&mut self, hash: Green, keyword: Green, is_active: bool) -> Green {
        let expr = self.parse_expression();
        let eod = self.parse_end_of_directive(false, false, false);
        let is_true = self.evaluate_bool(expr);
        let branch_taken = is_active && is_true;
        self.arena().if_directive_trivia(hash, keyword, expr, eod, is_active, branch_taken, is_true)
    }

    /// `ParseElifDirective(hash, keyword, isActive, endIsActive)`.
    fn parse_elif_directive(&mut self, hash: Green, keyword: Green, is_active: bool, end_is_active: bool) -> Green {
        // the expression's full span starts where the lexer is: a directive token has no leading trivia
        let expr_start = self.lexer.position();
        let expr = self.parse_expression();
        let mut eod = self.parse_end_of_directive(false, false, false);
        let store = self.lexer.directive_store();
        if self.context.has_previous_if_or_elif(store) {
            let is_true = self.evaluate_bool(expr);
            let store = self.lexer.directive_store();
            let branch_taken = end_is_active && is_true && !self.context.previous_branch_taken(store);
            self.arena().elif_directive_trivia(hash, keyword, expr, eod, end_is_active, branch_taken, is_true)
        } else {
            // `eod.TokenWithLeadingTrivia(SyntaxList.Concat(SyntaxFactory.DisabledText(expr.ToFullString()), ...))`
            let width = self.arena_ref().full_width(expr);
            let a = self.arena();
            let disabled = a.trivia(SyntaxKind::DisabledTextTrivia, expr_start, width);
            let disabled = a.trivia_list(&[disabled]);
            let leading = a.leading_trivia(eod);
            let leading = a.concat_trivia(disabled, leading);
            eod = a.token_with_leading_trivia(eod, leading);
            let store = self.lexer.directive_store();
            let code = if self.context.has_unfinished_region(store) {
                ErrorCode::ERR_EndRegionDirectiveExpected
            } else if self.context.has_unfinished_if(store) {
                ErrorCode::ERR_EndifDirectiveExpected
            } else {
                ErrorCode::ERR_UnexpectedDirective
            };
            let bad = self.arena().bad_directive_trivia(hash, keyword, eod, is_active);
            self.add_error(bad, code)
        }
    }

    /// `ParseElseDirective(hash, keyword, isActive, endIsActive)`.
    fn parse_else_directive(&mut self, hash: Green, keyword: Green, is_active: bool, end_is_active: bool) -> Green {
        let eod = self.parse_end_of_directive(false, false, false);
        let store = self.lexer.directive_store();
        if self.context.has_previous_if_or_elif(store) {
            let branch_taken = end_is_active && !self.context.previous_branch_taken(store);
            return self.arena().else_directive_trivia(hash, keyword, eod, end_is_active, branch_taken);
        }
        let code = if self.context.has_unfinished_region(store) {
            ErrorCode::ERR_EndRegionDirectiveExpected
        } else if self.context.has_unfinished_if(store) {
            ErrorCode::ERR_EndifDirectiveExpected
        } else {
            ErrorCode::ERR_UnexpectedDirective
        };
        let bad = self.arena().bad_directive_trivia(hash, keyword, eod, is_active);
        self.add_error(bad, code)
    }

    /// `ParseEndIfDirective(hash, keyword, isActive, endIsActive)`.
    fn parse_end_if_directive(&mut self, hash: Green, keyword: Green, is_active: bool, end_is_active: bool) -> Green {
        let eod = self.parse_end_of_directive(false, false, false);
        let store = self.lexer.directive_store();
        if self.context.has_unfinished_if(store) {
            self.arena().end_if_directive_trivia(hash, keyword, eod, end_is_active)
        } else if self.context.has_unfinished_region(store) {
            // CONSIDER: dev10 actually pops the region off the directive stack here.
            // See if (tok == PPT_ENDIF) in CSourceData::CPreprocessor::ScanPreprocessorIfSection.
            let bad = self.arena().bad_directive_trivia(hash, keyword, eod, is_active);
            self.add_error(bad, ErrorCode::ERR_EndRegionDirectiveExpected)
        } else {
            let bad = self.arena().bad_directive_trivia(hash, keyword, eod, is_active);
            self.add_error(bad, ErrorCode::ERR_UnexpectedDirective)
        }
    }

    /// `ParseRegionDirective(hash, keyword, isActive)`.
    fn parse_region_directive(&mut self, hash: Green, keyword: Green, is_active: bool) -> Green {
        let eod = self.parse_end_of_directive_with_optional_preprocessing_message();
        self.arena().region_directive_trivia(hash, keyword, eod, is_active)
    }

    /// `ParseEndRegionDirective(hash, keyword, isActive)`.
    fn parse_end_region_directive(&mut self, hash: Green, keyword: Green, is_active: bool) -> Green {
        let eod = self.parse_end_of_directive_with_optional_preprocessing_message();
        let store = self.lexer.directive_store();
        if self.context.has_unfinished_region(store) {
            self.arena().end_region_directive_trivia(hash, keyword, eod, is_active)
        } else if self.context.has_unfinished_if(store) {
            let bad = self.arena().bad_directive_trivia(hash, keyword, eod, is_active);
            self.add_error(bad, ErrorCode::ERR_EndifDirectiveExpected)
        } else {
            let bad = self.arena().bad_directive_trivia(hash, keyword, eod, is_active);
            self.add_error(bad, ErrorCode::ERR_UnexpectedDirective)
        }
    }

    /// `ParseDefineOrUndefDirective(hash, keyword, isActive, isFollowingToken)`.
    fn parse_define_or_undef_directive(
        &mut self,
        hash: Green,
        mut keyword: Green,
        is_active: bool,
        is_following_token: bool,
    ) -> Green {
        if is_following_token {
            keyword = self.add_error(keyword, ErrorCode::ERR_PPDefFollowsToken);
        }

        let name = self.eat_token_code(SyntaxKind::IdentifierToken, ErrorCode::ERR_IdentifierExpected, true);
        let name = self.truncate_identifier(name);
        let name_is_missing = self.arena_ref().is_missing(name);
        let end = self.parse_end_of_directive(name_is_missing, false, false);
        if self.arena_ref().kind(keyword) == SyntaxKind::DefineKeyword {
            self.arena().define_directive_trivia(hash, keyword, name, end, is_active)
        } else {
            self.arena().undef_directive_trivia(hash, keyword, name, end, is_active)
        }
    }

    /// `ParseErrorOrWarningDirective(hash, keyword, isActive)`: an error/warning directive tells the compiler to
    /// indicate a syntactic error/warning at the current location.
    ///
    /// Format: #error Error message string
    /// Resulting message: from the first non-whitespace character after the directive keyword until the end of the
    /// directive (aka EOD) at the line break or EOF.
    /// Resulting span: [first non-whitespace char, EOD)
    fn parse_error_or_warning_directive(&mut self, hash: Green, keyword: Green, is_active: bool) -> Green {
        let mut eod = self.parse_end_of_directive_with_optional_preprocessing_message();
        let is_error = self.arena_ref().kind(keyword) == SyntaxKind::ErrorKeyword;
        if is_active {
            // whitespace and single line comments are trailing trivia on the keyword, the rest of the error message is
            // leading trivia on the eod.
            let a = self.arena_ref();
            let mut text_start: Option<u32> = None;
            let mut trivia_width = 0u32;
            let mut skipping = true;
            let mut count = |t: Trivia| {
                text_start.get_or_insert(a.trivia_start(t));
                trivia_width += a.trivia_width(t);
            };
            for &t in a.trivia_items(a.trailing_trivia(keyword)) {
                if skipping {
                    if a.trivia_kind(t) == SyntaxKind::WhitespaceTrivia {
                        continue;
                    }

                    skipping = false;
                }

                count(t);
            }

            for &t in a.trivia_items(a.leading_trivia(eod)) {
                count(t);
            }

            // Relative to Start (not FullStart) of eod. Can be negative if part of the error text comes from the
            // trailing trivia of the keyword token.
            let trivia_offset = -(trivia_width as i32);

            // the trivia are contiguous text before the eod
            let text: &[u8] = match text_start {
                Some(start) => self.source().get(start as usize..(start + trivia_width) as usize).unwrap_or(b""),
                None => b"",
            };
            let error_text = String::from_utf8_lossy(text).into_owned();
            let (start, len) = self.arena().intern(&error_text);
            let code = if is_error { ErrorCode::ERR_ErrorDirective } else { ErrorCode::WRN_WarningDirective };
            eod = self.add_error_at(eod, trivia_offset, trivia_width as i32, code, &[DiagArg::Owned { start, len }]);

            if is_error {
                if error_text == "version" {
                    let display_language_version = match self.options().version {
                        // `specified == effective ? ... : $"{specified} ({effective})"`: a file is parsed at `latest`
                        LanguageVersion::LATEST => "latest (14.0)",
                        version => version.to_display_string(),
                    };

                    eod = self.add_error_at(
                        eod,
                        trivia_offset,
                        trivia_width as i32,
                        ErrorCode::ERR_CompilerAndLanguageVersion,
                        &[
                            DiagArg::Str(PRODUCT_VERSION),
                            DiagArg::Str(display_language_version),
                            DiagArg::Str(ASSEMBLY_PATH),
                        ],
                    );
                } else {
                    const VERSION_MARKER: &str = "version:";
                    if self.options().version != LanguageVersion::Preview
                        && let Some(rest) = error_text.strip_prefix(VERSION_MARKER)
                        && let Some(language_version) = specified_version_display(rest)
                        && let Some(error) = self.options().version.get_error_code()
                    {
                        eod = self.add_error_at(
                            eod,
                            trivia_offset,
                            trivia_width as i32,
                            error,
                            &[DiagArg::Str("version"), DiagArg::Str(language_version)],
                        );
                    }
                }
            }
        }

        if is_error {
            self.arena().error_directive_trivia(hash, keyword, eod, is_active)
        } else {
            self.arena().warning_directive_trivia(hash, keyword, eod, is_active)
        }
    }

    /// `ParseLineDirective(hash, id, isActive)`.
    fn parse_line_directive(&mut self, hash: Green, id: Green, is_active: bool) -> Green {
        let mut line;
        let mut file = Green::NULL;
        let mut saw_line_but_not_file = false;
        match self.current_kind() {
            SyntaxKind::DefaultKeyword | SyntaxKind::HiddenKeyword => {
                line = self.eat_token();
            }
            _ => {
                line =
                    self.eat_token_code(SyntaxKind::NumericLiteralToken, ErrorCode::ERR_InvalidLineNumber, is_active);
                saw_line_but_not_file = true; //assume this is the case until we (potentially) see the file name below
                if is_active
                    && !self.arena_ref().is_missing(line)
                    && self.arena_ref().kind(line) == SyntaxKind::NumericLiteralToken
                {
                    let value = self.int_value(line);
                    if value < 1 {
                        line = self.add_error(line, ErrorCode::ERR_InvalidLineNumber);
                    } else if value > MAX_LINE_VALUE {
                        line = self.add_error(line, ErrorCode::WRN_TooManyLinesForDebugger);
                    }
                }

                let current = self.current_token();
                let a = self.arena_ref();
                if a.kind(current) == SyntaxKind::StringLiteralToken
                    && (a.is_missing(line) || a.trailing_trivia_width(line) > 0 || a.leading_trivia_width(current) > 0)
                {
                    //require separation between line number and file name
                    file = self.eat_token();
                    saw_line_but_not_file = false;
                }
            }
        }

        let ignore_errors = self.arena_ref().is_missing(line) || !is_active;
        let end = self.parse_end_of_directive(ignore_errors, false, saw_line_but_not_file);
        self.arena().line_directive_trivia(hash, id, line, file, end, is_active)
    }

    /// `(int)token.Value` of a directive's numeric literal: its digits (0 when they overflow, as `GetValueInt32`).
    fn int_value(&self, token: Green) -> i32 {
        parse_int32(self.arena_ref().token_text(token, self.source())).unwrap_or(0)
    }

    /// `ParseLineSpanDirective(hash, lineKeyword, isActive)`.
    fn parse_line_span_directive(&mut self, hash: Green, line_keyword: Green, is_active: bool) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::OpenParenToken);

        let mut report_error = is_active;
        let mut start_line = 0;
        let mut start_character = 0;
        let mut start = self.parse_line_directive_position(&mut report_error, &mut start_line, &mut start_character);
        let first = self.arena_ref().first_token(start);
        if self.no_trivia_between(line_keyword, first) {
            start = self.add_error(start, ErrorCode::ERR_LineSpanDirectiveRequiresSpace);
        }

        let minus = self.eat_token_report(SyntaxKind::MinusToken, report_error);
        if self.arena_ref().is_missing(minus) {
            report_error = false;
        }

        let mut end_line = 0;
        let mut end_character = 0;
        let mut end = self.parse_line_directive_position(&mut report_error, &mut end_line, &mut end_character);
        if report_error && (end_line < start_line || (end_line == start_line && end_character < start_character)) {
            end = self.add_error(end, ErrorCode::ERR_LineSpanDirectiveEndLessThanStart);
        }

        let mut character_offset = if self.current_kind() == SyntaxKind::NumericLiteralToken {
            let mut value = 0;
            self.parse_line_directive_numeric_literal(&mut report_error, 1, MAX_CHARACTER_VALUE, &mut value)
        } else {
            Green::NULL
        };

        let end_last = self.arena_ref().last_token(end);
        if self.no_trivia_between(end_last, character_offset) {
            character_offset = self.add_error(character_offset, ErrorCode::ERR_LineSpanDirectiveRequiresSpace);
        }

        let mut file = self.eat_token_code(SyntaxKind::StringLiteralToken, ErrorCode::ERR_MissingPPFile, report_error);
        if self.arena_ref().is_missing(file) {
            report_error = false;
        }

        let before_file = if character_offset.is_null() { self.arena_ref().last_token(end) } else { character_offset };
        if self.no_trivia_between(before_file, file) {
            file = self.add_error(file, ErrorCode::ERR_LineSpanDirectiveRequiresSpace);
        }

        let end_of_directive = self.parse_end_of_directive(!report_error, false, false);
        self.arena().line_span_directive_trivia(
            hash,
            line_keyword,
            start,
            minus,
            end,
            character_offset,
            file,
            end_of_directive,
            is_active,
        )
    }

    /// `noTriviaBetween(token1, token2)` (a local function of `ParseLineSpanDirective`).
    fn no_trivia_between(&self, token1: Green, token2: Green) -> bool {
        let a = self.arena_ref();
        !token1.is_null()
            && !a.is_missing(token1)
            && !token2.is_null()
            && !a.is_missing(token2)
            // `LanguageParser.NoTriviaBetween`
            && a.trailing_trivia_width(token1) == 0
            && a.leading_trivia_width(token2) == 0
    }

    /// `ParseLineDirectivePosition(ref reportError, out line, out character)`.
    fn parse_line_directive_position(&mut self, report_error: &mut bool, line: &mut i32, character: &mut i32) -> Green {
        let open_paren = self.eat_token_report(SyntaxKind::OpenParenToken, *report_error);
        if self.arena_ref().is_missing(open_paren) {
            *report_error = false;
        }

        let line_token = self.parse_line_directive_numeric_literal(report_error, 1, MAX_LINE_VALUE, line);

        let comma = self.eat_token_report(SyntaxKind::CommaToken, *report_error);
        if self.arena_ref().is_missing(comma) {
            *report_error = false;
        }

        let character_token =
            self.parse_line_directive_numeric_literal(report_error, 1, MAX_CHARACTER_VALUE, character);

        let close_paren = self.eat_token_report(SyntaxKind::CloseParenToken, *report_error);
        if self.arena_ref().is_missing(close_paren) {
            *report_error = false;
        }

        self.arena().line_directive_position(open_paren, line_token, comma, character_token, close_paren)
    }

    /// `ParseLineDirectiveNumericLiteral(ref reportError, minValue, maxValue, out value)`.
    fn parse_line_directive_numeric_literal(
        &mut self,
        report_error: &mut bool,
        min_value: i32,
        max_value: i32,
        value: &mut i32,
    ) -> Green {
        let mut token = self.eat_token_code(
            SyntaxKind::NumericLiteralToken,
            ErrorCode::ERR_LineSpanDirectiveInvalidValue,
            *report_error,
        );
        *value = 0;
        if self.arena_ref().is_missing(token) {
            *report_error = false;
        } else if self.arena_ref().kind(token) == SyntaxKind::NumericLiteralToken {
            *value = self.int_value(token);
            if *value < min_value || *value > max_value {
                token = self.add_error(token, ErrorCode::ERR_LineSpanDirectiveInvalidValue);
                *report_error = false;
            }
        }
        token
    }

    /// `ParseReferenceDirective(hash, keyword, isActive, isFollowingToken)`: `#r` is for scripts, and a `.cs` file is
    /// no script (`Options.Kind == SourceCodeKind.Regular`).
    fn parse_reference_directive(
        &mut self,
        hash: Green,
        mut keyword: Green,
        is_active: bool,
        is_following_token: bool,
    ) -> Green {
        let _ = is_following_token;
        if is_active {
            keyword = self.add_error(keyword, ErrorCode::ERR_ReferenceDirectiveOnlyAllowedInScripts);
        }

        let file = self.eat_token_code(SyntaxKind::StringLiteralToken, ErrorCode::ERR_ExpectedPPFile, is_active);

        let ignore_errors = self.arena_ref().is_missing(file) || !is_active;
        let end = self.parse_end_of_directive(ignore_errors, false, false);
        self.arena().reference_directive_trivia(hash, keyword, file, end, is_active)
    }

    /// `ParseLoadDirective(hash, keyword, isActive, isFollowingToken)`: `#load` is for scripts.
    fn parse_load_directive(
        &mut self,
        hash: Green,
        mut keyword: Green,
        is_active: bool,
        is_following_token: bool,
    ) -> Green {
        let _ = is_following_token;
        if is_active {
            keyword = self.add_error(keyword, ErrorCode::ERR_LoadDirectiveOnlyAllowedInScripts);
        }

        let file = self.eat_token_code(SyntaxKind::StringLiteralToken, ErrorCode::ERR_ExpectedPPFile, is_active);

        let ignore_errors = self.arena_ref().is_missing(file) || !is_active;
        let end = self.parse_end_of_directive(ignore_errors, false, false);
        self.arena().load_directive_trivia(hash, keyword, file, end, is_active)
    }

    /// `ParseNullableDirective(hash, token, isActive)`.
    fn parse_nullable_directive(&mut self, hash: Green, mut token: Green, is_active: bool) -> Green {
        if is_active {
            token = self.check_feature_availability(token, MessageID::IDS_FeatureNullableReferenceTypes, false);
        }

        let setting = match self.current_kind() {
            SyntaxKind::EnableKeyword | SyntaxKind::DisableKeyword | SyntaxKind::RestoreKeyword => self.eat_token(),
            _ => self.eat_token_code(
                SyntaxKind::DisableKeyword,
                ErrorCode::ERR_NullableDirectiveQualifierExpected,
                is_active,
            ),
        };

        let setting_is_missing = self.arena_ref().is_missing(setting);
        let target = match self.current_kind() {
            SyntaxKind::WarningsKeyword | SyntaxKind::AnnotationsKeyword => self.eat_token(),
            SyntaxKind::EndOfDirectiveToken | SyntaxKind::EndOfFileToken => Green::NULL,
            _ => self.eat_token_code(
                SyntaxKind::WarningsKeyword,
                ErrorCode::ERR_NullableDirectiveTargetExpected,
                !setting_is_missing && is_active,
            ),
        };

        let target_is_missing = !target.is_null() && self.arena_ref().is_missing(target);
        let end = self.parse_end_of_directive(setting_is_missing || target_is_missing || !is_active, false, false);
        self.arena().nullable_directive_trivia(hash, token, setting, target, end, is_active)
    }

    /// `ParsePragmaDirective(hash, pragma, isActive)`.
    fn parse_pragma_directive(&mut self, hash: Green, mut pragma: Green, is_active: bool) -> Green {
        if is_active {
            pragma = self.check_feature_availability(pragma, MessageID::IDS_FeaturePragma, false);
        }

        let mut has_error = false;
        if self.current_contextual_kind() == SyntaxKind::WarningKeyword {
            let warning = self.eat_contextual_token(SyntaxKind::WarningKeyword);
            if matches!(self.current_kind(), SyntaxKind::DisableKeyword | SyntaxKind::RestoreKeyword) {
                let style = self.eat_token();

                let mut ids: Vec<Green> = Vec::new();
                while self.current_kind() != SyntaxKind::EndOfDirectiveToken {
                    let id;
                    #[allow(clippy::needless_late_init, reason = "the C# declares both, then assigns them per case")]
                    let id_expression;

                    if self.current_kind() == SyntaxKind::NumericLiteralToken {
                        // Previous versions of the compiler used to report a warning (CS1691) whenever an unrecognized
                        // warning code was supplied in a #pragma directive (or via /nowarn /warnaserror flags on the
                        // command line). Going forward, we won't generate any warning in such cases. This will make
                        // maintenance of backwards compatibility easier (we no longer need to worry about breaking
                        // existing projects / command lines if we deprecate / remove an old warning code).
                        id = self.eat_token();
                        id_expression = self.arena().literal_expression(SyntaxKind::NumericLiteralExpression, id);
                    } else if self.current_kind() == SyntaxKind::IdentifierToken {
                        // Lexing / parsing of identifiers inside #pragma warning directives is identical to that inside
                        // #define directives except that very long identifiers inside #define are truncated to 128
                        // characters to maintain backwards compatibility with previous versions of the compiler. (See
                        // TruncateIdentifier() below.) Since support for identifiers inside #pragma warning directives
                        // is new, we don't have any backwards compatibility constraints. So we can preserve the
                        // identifier exactly as it appears in source.
                        id = self.eat_token();
                        id_expression = self.arena().identifier_name(id);
                    } else {
                        id = self.eat_token_code(
                            SyntaxKind::NumericLiteralToken,
                            ErrorCode::WRN_IdentifierOrNumericLiteralExpected,
                            is_active,
                        );
                        id_expression = self.arena().literal_expression(SyntaxKind::NumericLiteralExpression, id);
                    }

                    has_error = has_error || self.arena_ref().contains_diagnostics(id);
                    ids.push(id_expression);

                    if self.current_kind() != SyntaxKind::CommaToken {
                        break;
                    }

                    let comma = self.eat_token();
                    ids.push(comma);
                }

                let end = self.parse_end_of_directive(has_error || !is_active, true, false);
                let list = self.arena().list(&ids);
                self.arena().pragma_warning_directive_trivia(hash, pragma, warning, style, list, end, is_active)
            } else {
                let style = self.eat_token_code(SyntaxKind::DisableKeyword, ErrorCode::WRN_IllegalPPWarning, is_active);
                let end = self.parse_end_of_directive(true, true, false);
                self.arena().pragma_warning_directive_trivia(hash, pragma, warning, style, Green::NULL, end, is_active)
            }
        } else if self.current_kind() == SyntaxKind::ChecksumKeyword {
            let checksum = self.eat_token();
            let file = self.eat_token_code(SyntaxKind::StringLiteralToken, ErrorCode::WRN_IllegalPPChecksum, is_active);
            let file_is_missing = self.arena_ref().is_missing(file);
            let mut guid = self.eat_token_code(
                SyntaxKind::StringLiteralToken,
                ErrorCode::WRN_IllegalPPChecksum,
                is_active && !file_is_missing,
            );
            if is_active && !self.arena_ref().is_missing(guid) {
                let value = self.arena_ref().token_value_text(guid, self.source());
                if !guid_try_parse(value) {
                    guid = self.add_error(guid, ErrorCode::WRN_IllegalPPChecksum);
                }
            }

            let guid_is_missing = self.arena_ref().is_missing(guid);
            let mut bytes = self.eat_token_code(
                SyntaxKind::StringLiteralToken,
                ErrorCode::WRN_IllegalPPChecksum,
                is_active && !guid_is_missing,
            );
            if is_active && !self.arena_ref().is_missing(bytes) {
                let value = self.arena_ref().token_value_text(bytes, self.source());
                let utf16_len = String::from_utf8_lossy(value).encode_utf16().count();
                if !utf16_len.is_multiple_of(2) || !value.iter().all(|b| b.is_ascii_hexdigit()) {
                    bytes = self.add_error(bytes, ErrorCode::WRN_IllegalPPChecksum);
                }
            }

            let a = self.arena_ref();
            has_error = a.contains_diagnostics(file) | a.contains_diagnostics(guid) | a.contains_diagnostics(bytes);
            let eod = self.parse_end_of_directive(has_error, true, false);
            self.arena().pragma_checksum_directive_trivia(hash, pragma, checksum, file, guid, bytes, eod, is_active)
        } else {
            let warning = self.eat_token_code(SyntaxKind::WarningKeyword, ErrorCode::WRN_IllegalPragma, is_active);
            let style = self.eat_token_report(SyntaxKind::DisableKeyword, false);
            let eod = self.parse_end_of_directive(true, true, false);
            self.arena().pragma_warning_directive_trivia(hash, pragma, warning, style, Green::NULL, eod, is_active)
        }
    }

    /// `ParseShebangDirective(hash, exclamation, isActive)`: a `.cs` file is neither a script nor a file-based
    /// program.
    fn parse_shebang_directive(&mut self, hash: Green, mut exclamation: Green, is_active: bool) -> Green {
        exclamation = self.add_error(exclamation, ErrorCode::ERR_PPShebangInProjectBasedProgram);

        let eod = self.parse_end_of_directive_with_optional_preprocessing_message();
        self.arena().shebang_directive_trivia(hash, exclamation, eod, is_active)
    }

    /// `ParseIgnoredDirective(hash, colon, isActive, isFollowingToken)`: `#:` needs a file-based program.
    fn parse_ignored_directive(
        &mut self,
        hash: Green,
        mut colon: Green,
        is_active: bool,
        is_following_token: bool,
    ) -> Green {
        if is_active {
            colon = self.add_error(colon, ErrorCode::ERR_PPIgnoredNeedsFileBasedProgram);

            if is_following_token {
                colon = self.add_error(colon, ErrorCode::ERR_PPIgnoredFollowsToken);
            }

            if self.context.seen_any_if_directives() {
                colon = self.add_error(colon, ErrorCode::ERR_PPIgnoredFollowsIf);
            }
        }

        let mut content = Green::NULL;
        let end_of_directive = self.lexer.lex_end_of_directive_with_optional_content(&mut content);
        self.arena().ignored_directive_trivia(hash, colon, content, end_of_directive, is_active)
    }

    /// `ParseEndOfDirectiveWithOptionalPreprocessingMessage()`.
    fn parse_end_of_directive_with_optional_preprocessing_message(&mut self) -> Green {
        self.lexer.lex_end_of_directive_with_optional_preprocessing_message()
    }

    /// `ParseEndOfDirective(ignoreErrors, afterPragma, afterLineNumber)`.
    fn parse_end_of_directive(&mut self, ignore_errors: bool, after_pragma: bool, after_line_number: bool) -> Green {
        let mut skipped_tokens: Option<Vec<Green>> = None;

        // Consume all extraneous tokens as leading SkippedTokens trivia.
        if self.current_kind() != SyntaxKind::EndOfDirectiveToken && self.current_kind() != SyntaxKind::EndOfFileToken {
            let mut skipped = Vec::new();

            if !ignore_errors {
                let error_code = if after_pragma {
                    ErrorCode::WRN_EndOfPPLineExpected
                } else if after_line_number {
                    ErrorCode::ERR_MissingPPFile
                } else {
                    ErrorCode::ERR_EndOfPPLineExpected
                };

                let token = self.eat_token();
                let token = self.arena().with_diagnostics(token, &[]);
                skipped.push(self.add_error(token, error_code));
            }

            while self.current_kind() != SyntaxKind::EndOfDirectiveToken
                && self.current_kind() != SyntaxKind::EndOfFileToken
            {
                let token = self.eat_token();
                skipped.push(self.arena().with_diagnostics(token, &[]));
            }
            skipped_tokens = Some(skipped);
        }

        // attach text from extraneous tokens as trivia to EndOfDirective token
        let mut end_of_directive = if self.current_kind() == SyntaxKind::EndOfDirectiveToken {
            self.eat_token()
        } else {
            self.arena().token(SyntaxKind::EndOfDirectiveToken)
        };

        if let Some(skipped) = skipped_tokens {
            let a = self.arena();
            let list = a.list(&skipped);
            let trivia = a.skipped_tokens_trivia(list);
            let trivia = a.structured_trivia(trivia);
            let leading = a.trivia_list(&[trivia]);
            end_of_directive = a.token_with_leading_trivia(end_of_directive, leading);
        }

        end_of_directive
    }

    /// `ParseExpression()`.
    fn parse_expression(&mut self) -> Green {
        self.parse_logical_or()
    }

    /// `ParseLogicalOr()`.
    fn parse_logical_or(&mut self) -> Green {
        let mut left = self.parse_logical_and();
        while self.current_kind() == SyntaxKind::BarBarToken {
            let op = self.eat_token();
            let right = self.parse_logical_and();
            left = self.arena().binary_expression(SyntaxKind::LogicalOrExpression, left, op, right);
        }

        left
    }

    /// `ParseLogicalAnd()`.
    fn parse_logical_and(&mut self) -> Green {
        let mut left = self.parse_equality();
        while self.current_kind() == SyntaxKind::AmpersandAmpersandToken {
            let op = self.eat_token();
            let right = self.parse_equality();
            left = self.arena().binary_expression(SyntaxKind::LogicalAndExpression, left, op, right);
        }

        left
    }

    /// `ParseEquality()`.
    fn parse_equality(&mut self) -> Green {
        let mut left = self.parse_logical_not();
        while matches!(self.current_kind(), SyntaxKind::EqualsEqualsToken | SyntaxKind::ExclamationEqualsToken) {
            let op = self.eat_token();
            let right = self.nested(Self::parse_equality);
            let kind = crate::facts::get_binary_expression(self.arena_ref().kind(op));
            left = self.arena().binary_expression(kind, left, op, right);
        }

        left
    }

    /// `ParseLogicalNot()`.
    fn parse_logical_not(&mut self) -> Green {
        if self.current_kind() == SyntaxKind::ExclamationToken {
            let op = self.eat_token();
            let operand = self.nested(Self::parse_logical_not);
            return self.arena().prefix_unary_expression(SyntaxKind::LogicalNotExpression, op, operand);
        }

        self.parse_primary()
    }

    /// `ParsePrimary()`.
    fn parse_primary(&mut self) -> Green {
        let k = self.current_kind();
        match k {
            SyntaxKind::OpenParenToken => {
                let open = self.eat_token();
                let expr = self.nested(Self::parse_expression);
                let close = self.eat_token_kind(SyntaxKind::CloseParenToken);
                self.arena().parenthesized_expression(open, expr, close)
            }
            SyntaxKind::IdentifierToken => {
                let token = self.eat_token();
                let identifier = self.truncate_identifier(token);
                self.arena().identifier_name(identifier)
            }
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => {
                let token = self.eat_token();
                self.arena().literal_expression(crate::facts::get_literal_expression(k), token)
            }
            _ => {
                let token = self.eat_token_code(SyntaxKind::IdentifierToken, ErrorCode::ERR_InvalidPreprocExpr, true);
                self.arena().identifier_name(token)
            }
        }
    }

    /// `parse` one level deeper, or past `MAX_NESTED_SCAN_DEPTH` levels a missing identifier with
    /// `ERR_InvalidPreprocExpr` that consumes nothing (the rest of the line is skipped by `ParseEndOfDirective`).
    fn nested(&mut self, parse: fn(&mut Self) -> Green) -> Green {
        if self.depth >= MAX_NESTED_SCAN_DEPTH {
            let token = self.arena().missing_token(SyntaxKind::IdentifierToken);
            let token = self.add_error(token, ErrorCode::ERR_InvalidPreprocExpr);
            return self.arena().identifier_name(token);
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    /// `TruncateIdentifier(identifier)`: ignore everything after the 128th character by setting the value text of the
    /// token from the prefix.  This is for backwards compatibility with Dev 10.
    fn truncate_identifier(&mut self, identifier: Green) -> Green {
        let Some((start, width)) = self.arena_ref().token_source_range(identifier) else { return identifier };
        let window = self.lexer.text_window;
        if window.utf16_len(start, start + width) <= MAX_DIRECTIVE_IDENTIFIER_WIDTH {
            return identifier;
        }
        let a = self.arena_ref();
        let leading = a.leading_trivia(identifier);
        let trailing = a.trailing_trivia(identifier);

        // `text.Substring(0, MAX_DIRECTIVE_IDENTIFIER_WIDTH)`: the first 128 code units (half a surrogate pair is
        // U+FFFD)
        let mut end = start;
        for _ in 0..MAX_DIRECTIVE_IDENTIFIER_WIDTH {
            end = window.next_position(end);
        }
        let mut identifier_part = String::from_utf8_lossy(&self.source()[start as usize..end as usize]).into_owned();
        if super::is_high_surrogate(window.char_at(window.previous_position(end).unwrap_or(start))) {
            identifier_part.push('\u{FFFD}');
        }

        let a = self.arena();
        let token = a.create_token(
            leading,
            SyntaxKind::IdentifierToken,
            SyntaxKind::IdentifierToken,
            crate::green::TokenText::Source { start, width },
            trailing,
        );
        a.set_token_value(token, &identifier_part);
        token
    }

    /// `EvaluateBool(expr)`.
    fn evaluate_bool(&self, expr: Green) -> bool {
        self.evaluate(expr)
    }

    /// `Evaluate(expr)`: every case is a `bool`.
    fn evaluate(&self, expr: Green) -> bool {
        let a = self.arena_ref();
        match a.kind(expr) {
            SyntaxKind::ParenthesizedExpression => self.evaluate(a.slot(expr, 1)),
            SyntaxKind::TrueLiteralExpression => true,
            SyntaxKind::FalseLiteralExpression => false,
            SyntaxKind::LogicalAndExpression | SyntaxKind::BitwiseAndExpression => {
                self.evaluate_bool(a.slot(expr, 0)) && self.evaluate_bool(a.slot(expr, 2))
            }
            SyntaxKind::LogicalOrExpression | SyntaxKind::BitwiseOrExpression => {
                self.evaluate_bool(a.slot(expr, 0)) || self.evaluate_bool(a.slot(expr, 2))
            }
            SyntaxKind::EqualsExpression => self.evaluate(a.slot(expr, 0)) == self.evaluate(a.slot(expr, 2)),
            SyntaxKind::NotEqualsExpression => self.evaluate(a.slot(expr, 0)) != self.evaluate(a.slot(expr, 2)),
            SyntaxKind::LogicalNotExpression => !self.evaluate_bool(a.slot(expr, 1)),
            SyntaxKind::IdentifierName => {
                // For backwards compatibility, we want to evaluate any unicode escape sequences in the identifier name
                // and then check again for boolean literals.  (This actually seems like a bug that we're retaining for
                // back-compat, because section 2.4.1 of the spec says that escape sequences can only appear in
                // identifiers, character literals, and regular string literals - not boolean literals.  In
                // (non-directive) C#, true is equivalent to the identifier @true, not the boolean literal true.)
                let id = a.token_value_text(a.slot(expr, 0), self.source());
                if let Some(constant_value) = bool_try_parse(id) {
                    return constant_value;
                }
                self.is_defined(id)
            }
            _ => false,
        }
    }

    /// `IsDefined(id)`: no preprocessor symbols but the file's own.
    fn is_defined(&self, id: &[u8]) -> bool {
        match self.context.is_defined(self.lexer.directive_store(), id) {
            DefineState::Defined => true,
            DefineState::Undefined | DefineState::Unspecified => false,
        }
    }
}

/// `bool.TryParse(value)`: `True` or `False` in any case, with white space and nulls around.
fn bool_try_parse(value: &[u8]) -> Option<bool> {
    let trimmed = String::from_utf8_lossy(value);
    let trimmed = trimmed.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if trimmed.eq_ignore_ascii_case("true") {
        Some(true)
    } else if trimmed.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// `LanguageVersionFacts.TryParse(version)` and `ToDisplayString()` of the specified version it gives (what
/// `CSharpRequiredLanguageVersion` prints): `None` for a text that is no version.
fn specified_version_display(version: &str) -> Option<&'static str> {
    Some(match version.to_ascii_lowercase().as_str() {
        "default" => "default",
        "latest" => "latest",
        "latestmajor" => "latestmajor",
        "preview" => "preview",
        other => LanguageVersion::from_name(other)?.to_display_string(),
    })
}

/// `Guid.TryParse(value)`: the formats `N` (32 hex digits), `D` (8-4-4-4-12), `B` (`{D}`), `P` (`(D)`) and `X`
/// (`{0xXXXXXXXX,0xXXXX,0xXXXX,{0xXX,...}}`), with white space around.
fn guid_try_parse(value: &[u8]) -> bool {
    let text = String::from_utf8_lossy(value);
    let s = text.trim().as_bytes();
    let hex = |b: &[u8]| !b.is_empty() && b.iter().all(u8::is_ascii_hexdigit);
    let d = |b: &[u8]| {
        b.len() == 36
            && b[8] == b'-'
            && b[13] == b'-'
            && b[18] == b'-'
            && b[23] == b'-'
            && hex(&b[0..8])
            && hex(&b[9..13])
            && hex(&b[14..18])
            && hex(&b[19..23])
            && hex(&b[24..36])
    };
    match s.len() {
        32 => hex(s),
        36 => d(s),
        38 => (s[0] == b'{' && s[37] == b'}' || s[0] == b'(' && s[37] == b')') && d(&s[1..37]),
        _ => guid_try_parse_x(s),
    }
}

/// The `X` format of `Guid.TryParse`: white space anywhere, then `{0x` and 1-8 hex digits, `,0x` 1-4, `,0x` 1-4,
/// `,{0x` 1-2 and seven more `,0x` 1-2, `}}`.
fn guid_try_parse_x(s: &[u8]) -> bool {
    let s: Vec<u8> = s.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut i = 0;
    let expect = |text: &[u8], i: &mut usize| -> bool {
        if s.get(*i..*i + text.len()) == Some(text) {
            *i += text.len();
            true
        } else {
            false
        }
    };
    let hex_run = |i: &mut usize, max: usize| -> bool {
        let start = *i;
        while *i < s.len() && s[*i].is_ascii_hexdigit() {
            *i += 1;
        }
        (1..=max).contains(&(*i - start))
    };
    if !expect(b"{0x", &mut i) && !expect(b"{0X", &mut i) {
        return false;
    }
    if !hex_run(&mut i, 8) {
        return false;
    }
    for _ in 0..2 {
        if !(expect(b",0x", &mut i) || expect(b",0X", &mut i)) || !hex_run(&mut i, 4) {
            return false;
        }
    }
    if !(expect(b",{0x", &mut i) || expect(b",{0X", &mut i)) || !hex_run(&mut i, 2) {
        return false;
    }
    for _ in 0..7 {
        if !(expect(b",0x", &mut i) || expect(b",0X", &mut i)) || !hex_run(&mut i, 2) {
            return false;
        }
    }
    expect(b"}}", &mut i) && i == s.len()
}
