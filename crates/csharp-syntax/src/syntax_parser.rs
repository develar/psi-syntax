//! `SyntaxParser` (Roslyn's `Parser/SyntaxParser.cs`, `SyntaxParser.ResetPoint.cs`), ported method by method in its
//! order: the token window over the lexer, the lexer mode, reset points, `EatToken` and its relatives, the error
//! helpers (`AddError`, `AddErrorToFirstToken`, ...), skipped syntax (`AddLeadingSkippedSyntax`, ...), keyword
//! conversions and the feature checks. `LanguageParser`, the directive parser and the documentation comment parser
//! each hold one (`ParserKind` says which, for the methods a subclass overrides).
//!
//! Not ported: the incremental half (`Blender`, `CurrentNode`, `EatNode`, the old tree): psi-syntax parses whole files.
//! What Roslyn keeps per blended token for a parser that changes modes (the position, directives and lexer-driven mode
//! after it) is kept for every token: a mode change re-lexes from the state after the last kept token.

use crate::error_code::{ErrorCode, MessageID};
use crate::green::{DiagArg, DiagnosticId, Green, GreenArena, Trivia, TriviaList};
use crate::lexer::{DirectiveStack, Lexer, LexerMode, LexerOptions};
use crate::syntax_kind::SyntaxKind;
use crate::tree::DocumentationMode;
use crate::{LanguageVersion, facts};

/// Which subclass of `SyntaxParser` this is: the overrides of `GetExpectedTokenError`,
/// `GetExpectedMissingNodeOrTokenError` and `WithAdditionalDiagnostics` are `DocumentationCommentParser`'s.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ParserKind {
    Language,
    Directive,
    DocumentationComment,
}

/// The lexer's state after a token (Roslyn's `Blender` of a blended token): where the next token is lexed from.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct LexerState {
    pub position: u32,
    pub directives: DirectiveStack,
    /// The lexer-driven mode bits (`MaskXmlDocCommentLocation | MaskXmlDocCommentStyle`).
    pub mode: LexerMode,
}

#[derive(Copy, Clone, Debug)]
struct BufferedToken {
    token: Green,
    after: LexerState,
}

/// The token window's buffer, kept by a `Parser` from one file to the next.
#[derive(Default)]
pub struct SyntaxParserBuffers {
    tokens: Vec<BufferedToken>,
    trivia: Vec<Trivia>,
    nodes: Vec<Green>,
}

/// `SyntaxParser.ResetPoint`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ResetPoint {
    pub(crate) reset_count: i32,
    pub(crate) mode: LexerMode,
    pub(crate) position: i32,
    pub(crate) prev_token_trailing_trivia: TriviaList,
}

/// `CachedTokenArraySize`: the token window's initial size.
const CACHED_TOKEN_ARRAY_SIZE: usize = 4096;

/// `SyntaxParser` over a lexer.
pub struct SyntaxParser<'l, 't> {
    /// `lexer`.
    pub lexer: &'l mut Lexer<'t>,
    pub kind: ParserKind,
    allow_mode_reset: bool,
    mode: LexerMode,
    /// `_firstBlender`: the lexer state before `tokens[0]`.
    first_state: LexerState,
    /// `_currentToken` (null: not fetched).
    current_token: Green,
    /// `_lexedTokens` / `_blendedTokens`, `token_count` long.
    tokens: Vec<BufferedToken>,
    /// The C# array's length (`CachedTokenArraySize`, doubled when full): when the window shifts.
    window_length: usize,
    prev_token_trailing_trivia: TriviaList,
    /// The position of `tokens[0]`.
    first_token: i32,
    /// The index of the current token in `tokens`.
    token_offset: i32,
    token_count: i32,
    reset_count: i32,
    reset_start: i32,
    trivia_scratch: Vec<Trivia>,
    node_scratch: Vec<Green>,
    /// The end of file every token reads as once the parse is abandoned (`abandon`), else null.
    forced_eof: Green,
    /// Where the parse was abandoned because the arena outgrew its budget (`abandon_over_budget`).
    pub budget_exhausted_at: Option<u32>,
}

impl<'l, 't> SyntaxParser<'l, 't> {
    /// `SyntaxParser(lexer, mode, oldTree: null, changes: null, allowModeReset, preLexIfNotIncremental)`.
    pub fn new(
        lexer: &'l mut Lexer<'t>,
        mode: LexerMode,
        kind: ParserKind,
        allow_mode_reset: bool,
        pre_lex_if_not_incremental: bool,
        buffers: SyntaxParserBuffers,
    ) -> SyntaxParser<'l, 't> {
        let mut tokens = buffers.tokens;
        tokens.clear();
        let first_state =
            LexerState { position: lexer.position(), directives: lexer.directives(), mode: LexerMode::NONE };
        let mut parser = SyntaxParser {
            lexer,
            kind,
            allow_mode_reset,
            mode,
            first_state,
            current_token: Green::NULL,
            tokens,
            window_length: CACHED_TOKEN_ARRAY_SIZE,
            prev_token_trailing_trivia: TriviaList::EMPTY,
            first_token: 0,
            token_offset: 0,
            token_count: 0,
            reset_count: 0,
            reset_start: 0,
            trivia_scratch: buffers.trivia,
            node_scratch: buffers.nodes,
            forced_eof: Green::NULL,
            budget_exhausted_at: None,
        };
        // PreLex is not cancellable: Roslyn prelexes unless a cancellation token can cancel, so always here.
        if pre_lex_if_not_incremental && !parser.allow_mode_reset {
            parser.pre_lex();
        }
        parser
    }

    /// `Dispose()`: the buffers back for reuse.
    pub fn into_buffers(self) -> SyntaxParserBuffers {
        SyntaxParserBuffers { tokens: self.tokens, trivia: self.trivia_scratch, nodes: self.node_scratch }
    }

    /// `ReInitialize()`.
    pub fn re_initialize(&mut self) {
        self.first_token = 0;
        self.token_offset = 0;
        self.token_count = 0;
        self.tokens.clear();
        self.reset_count = 0;
        self.reset_start = 0;
        self.current_token = Green::NULL;
        self.prev_token_trailing_trivia = TriviaList::EMPTY;
        if self.allow_mode_reset {
            self.first_state = LexerState {
                position: self.lexer.position(),
                directives: self.lexer.directives(),
                mode: LexerMode::NONE,
            };
        }
    }

    /// `IsIncremental`: never.
    pub fn is_incremental(&self) -> bool {
        false
    }

    /// `PreLex()`: lexes up to `min(4096, length / 2)` tokens ahead (the length in UTF-16 code units, as Roslyn's).
    fn pre_lex(&mut self) {
        let window = &self.lexer.text()[self.lexer.position() as usize..self.lexer.end() as usize];
        let size = CACHED_TOKEN_ARRAY_SIZE.min(utf16_len(window) / 2);
        let mode = self.mode;
        for _ in 0..size {
            let token = self.lexer.lex(mode);
            let after = self.state_after(LexerMode::NONE);
            self.add_lexed_token(token, after);
            if self.lexer.arena_ref().kind(token) == SyntaxKind::EndOfFileToken {
                break;
            }
        }
    }

    fn state_after(&self, mode: LexerMode) -> LexerState {
        LexerState {
            position: self.lexer.position(),
            directives: self.lexer.directives(),
            mode: mode.and(LexerMode::MASK_XML_DOC_COMMENT_LOCATION.or(LexerMode::MASK_XML_DOC_COMMENT_STYLE)),
        }
    }

    /// `GetResetPoint()`.
    pub fn get_reset_point(&mut self) -> ResetPoint {
        let pos = self.current_token_position();
        if self.reset_count == 0 {
            self.reset_start = pos; // low water mark
        }
        self.reset_count += 1;
        ResetPoint {
            reset_count: self.reset_count,
            mode: self.mode,
            position: pos,
            prev_token_trailing_trivia: self.prev_token_trailing_trivia,
        }
    }

    /// `Reset(ref point)`.
    pub fn reset(&mut self, point: &ResetPoint) {
        let mut offset = point.position - self.first_token;
        debug_assert!(offset >= 0);
        if offset >= self.token_count {
            // Re-fetch tokens to the position in the reset point
            self.peek_token(offset - self.token_offset);
            // Re-calculate new offset in case tokens got shifted to the left while we were peeking.
            offset = point.position - self.first_token;
        }
        self.mode = point.mode;
        self.token_offset = offset.clamp(0, self.token_count);
        self.current_token = Green::NULL;
        self.prev_token_trailing_trivia = point.prev_token_trailing_trivia;
    }

    /// `Release(ref point)`.
    pub fn release(&mut self, point: &ResetPoint) {
        debug_assert_eq!(self.reset_count, point.reset_count);
        self.reset_count -= 1;
        if self.reset_count == 0 {
            self.reset_start = -1;
        }
    }

    /// `Options`.
    pub fn options(&self) -> LexerOptions {
        self.lexer.options()
    }

    /// `IsScript`: `.csx` scripts are out of scope.
    pub fn is_script(&self) -> bool {
        false
    }

    /// `Mode` (get).
    pub fn mode(&self) -> LexerMode {
        self.mode
    }

    /// `Mode` (set): a different mode forgets the tokens after the current one.
    pub fn set_mode(&mut self, value: LexerMode) {
        if self.mode != value {
            debug_assert!(self.allow_mode_reset);
            self.mode = value;
            self.current_token = Green::NULL;
            self.token_count = self.token_offset;
            self.tokens.truncate(self.token_count.max(0) as usize);
        }
    }

    /// The green arena.
    #[inline]
    pub fn arena(&mut self) -> &mut GreenArena {
        self.lexer.arena()
    }

    /// The green arena, read-only.
    #[inline]
    pub fn arena_ref(&self) -> &GreenArena {
        self.lexer.arena_ref()
    }

    /// The file's bytes: what token texts index.
    #[inline]
    pub fn source(&self) -> &'t [u8] {
        self.lexer.text()
    }

    /// `CurrentToken`.
    #[inline]
    pub fn current_token(&mut self) -> Green {
        if self.current_token.is_null() {
            self.current_token = self.fetch_current_token();
        }
        if self.lexer.arena_ref().is_over_budget() {
            self.abandon_over_budget();
        }
        self.current_token
    }

    /// The arena outgrew its budget (`green::budget_for`, not in Roslyn): the parse is abandoned (`abandon`), and
    /// `LanguageParser::parse_with_stack_guard` gives what Roslyn gives when its stack runs out.
    #[cold]
    #[inline(never)]
    fn abandon_over_budget(&mut self) {
        if self.budget_exhausted_at.is_none() {
            self.budget_exhausted_at = Some(self.lexer.position());
        }
        self.abandon();
    }

    /// `CurrentToken.Kind`.
    #[inline]
    pub fn current_kind(&mut self) -> SyntaxKind {
        let t = self.current_token();
        self.arena_ref().kind(t)
    }

    /// `CurrentToken.ContextualKind`.
    #[inline]
    pub fn current_contextual_kind(&mut self) -> SyntaxKind {
        let t = self.current_token();
        self.arena_ref().contextual_kind(t)
    }

    /// Abandons the parse (not in Roslyn: what its `InsufficientExecutionStackException` does by unwinding): from now
    /// on every token, current or peeked, is one end of file, so every loop of the parsers ends and they return.
    pub fn abandon(&mut self) {
        if self.forced_eof.is_null() {
            self.forced_eof = self.arena().token(SyntaxKind::EndOfFileToken);
        }
        self.current_token = self.forced_eof;
    }

    /// `FetchCurrentToken()`.
    fn fetch_current_token(&mut self) -> Green {
        if !self.forced_eof.is_null() {
            return self.forced_eof;
        }
        if self.token_offset >= self.token_count {
            self.add_new_token();
        }
        self.tokens.get(self.token_offset as usize).map_or(Green::NULL, |t| t.token)
    }

    /// `AddNewToken()`: lexes the next token from the state after the last buffered one.
    fn add_new_token(&mut self) {
        let state =
            if self.token_count > 0 { self.tokens[self.token_count as usize - 1].after } else { self.first_state };
        // `Blender.Reader.LexNewToken`
        if self.lexer.position() != state.position {
            self.lexer.reset(state.position, state.directives);
        }
        let mut mode = self.mode;
        if mode.0 >= LexerMode::XML_DOC_COMMENT.0 {
            mode = mode.or(state.mode);
        }
        let token = self.lexer.lex_ref(&mut mode);
        let after = self.state_after(mode);
        self.add_lexed_token(token, after);
    }

    /// `AddLexedToken(token)` (and `AddToken` of a blended one).
    fn add_lexed_token(&mut self, token: Green, after: LexerState) {
        if self.token_count as usize >= self.window_length {
            self.add_lexed_token_slot();
        }
        self.tokens.truncate(self.token_count as usize);
        self.tokens.push(BufferedToken { token, after });
        self.token_count += 1;
    }

    /// `AddLexedTokenSlot()` (and `AddTokenSlot`): shifts the window left when far to the right and no reset point
    /// holds its start, else grows it.
    fn add_lexed_token_slot(&mut self) {
        if self.token_offset as usize > (self.window_length >> 1)
            && (self.reset_start == -1 || self.reset_start > self.first_token)
        {
            let shift_offset =
                if self.reset_start == -1 { self.token_offset } else { self.reset_start - self.first_token };
            debug_assert!(shift_offset > 0);
            let shift = shift_offset.clamp(0, self.token_count) as usize;
            if shift > 0 {
                self.first_state = self.tokens[shift - 1].after;
            }
            self.tokens.drain(..shift);
            self.first_token += shift_offset;
            self.token_count -= shift_offset;
            self.token_offset -= shift_offset;
        } else {
            self.window_length *= 2;
        }
    }

    /// `PeekToken(n)`.
    pub fn peek_token(&mut self, n: i32) -> Green {
        debug_assert!(n >= 0);
        if self.lexer.arena_ref().is_over_budget() {
            self.abandon_over_budget();
        }
        if !self.forced_eof.is_null() {
            return self.forced_eof;
        }
        while self.token_offset + n >= self.token_count {
            let before = self.token_count;
            self.add_new_token();
            if self.token_count == before {
                break;
            }
        }
        self.tokens.get((self.token_offset + n).max(0) as usize).map_or(Green::NULL, |t| t.token)
    }

    /// `PeekToken(n).Kind`.
    pub fn peek_kind(&mut self, n: i32) -> SyntaxKind {
        let t = self.peek_token(n);
        self.arena_ref().kind(t)
    }

    /// `EatToken()`.
    #[inline]
    pub fn eat_token(&mut self) -> Green {
        let ct = self.current_token();
        self.move_to_next_token();
        ct
    }

    /// `TryEatToken(kind)`: the current token when it is of `kind` (consumed), else null.
    pub fn try_eat_token(&mut self, kind: SyntaxKind) -> Green {
        if self.current_kind() == kind { self.eat_token() } else { Green::NULL }
    }

    /// `MoveToNextToken()`.
    fn move_to_next_token(&mut self) {
        self.prev_token_trailing_trivia = self.arena_ref().trailing_trivia(self.current_token);
        self.current_token = Green::NULL;
        self.token_offset += 1;
    }

    /// Whether the parse was abandoned (`abandon`): every token reads as the end of file.
    #[inline]
    pub fn is_abandoned(&self) -> bool {
        !self.forced_eof.is_null()
    }

    /// Moves forward to the token at `position` (not in Roslyn): what eating every token up to it does, for a scan
    /// replayed from its memo (`LanguageParser::scan_tuple_type`).
    pub fn advance_to_position(&mut self, position: i32) {
        let delta = position - self.current_token_position();
        if delta <= 0 {
            return;
        }
        let last = self.peek_token(delta - 1);
        self.prev_token_trailing_trivia = self.arena_ref().trailing_trivia(last);
        self.current_token = Green::NULL;
        self.token_offset += delta;
    }

    /// `ForceEndOfFile()`.
    pub fn force_end_of_file(&mut self) {
        self.current_token = self.arena().token(SyntaxKind::EndOfFileToken);
    }

    /// `EatToken(kind)`: the current token when it is of `kind`, else a missing one with an error.
    #[inline]
    pub fn eat_token_kind(&mut self, kind: SyntaxKind) -> Green {
        debug_assert!(facts::is_any_token(kind));
        let ct = self.current_token();
        if self.arena_ref().kind(ct) == kind {
            self.move_to_next_token();
            return ct;
        }
        //slow part of EatToken(SyntaxKind kind)
        let actual = self.current_kind();
        self.create_missing_token(kind, actual)
    }

    /// `EatTokenAsKind(expected)`: the current token when it is `expected`; else a missing one with the current token
    /// skipped after it.
    pub fn eat_token_as_kind(&mut self, expected: SyntaxKind) -> Green {
        debug_assert!(facts::is_any_token(expected));
        let ct = self.current_token();
        if self.arena_ref().kind(ct) == expected {
            self.move_to_next_token();
            return ct;
        }
        let actual = self.current_kind();
        let replacement = self.create_missing_token(expected, actual);
        let eaten = self.eat_token();
        self.add_trailing_skipped_syntax(replacement, eaten)
    }

    /// `CreateMissingToken(expected, actual)`.
    pub fn create_missing_token(&mut self, expected: SyntaxKind, actual: SyntaxKind) -> Green {
        let token = self.arena().missing_token(expected);
        let error = self.get_expected_missing_node_or_token_error(token, expected, actual);
        self.with_additional_diagnostics(token, &[error])
    }

    /// `CreateMissingToken(expected, code, reportError)`.
    fn create_missing_token_code(&mut self, expected: SyntaxKind, code: ErrorCode, report_error: bool) -> Green {
        // should we eat the current ParseToken's leading trivia?
        let token = self.arena().missing_token(expected);
        if report_error { self.add_error(token, code) } else { token }
    }

    /// `EatToken(kind, reportError)`.
    pub fn eat_token_report(&mut self, kind: SyntaxKind, report_error: bool) -> Green {
        if report_error {
            return self.eat_token_kind(kind);
        }
        debug_assert!(facts::is_any_token(kind));
        if self.current_kind() != kind {
            // should we eat the current ParseToken's leading trivia?
            self.arena().missing_token(kind)
        } else {
            self.eat_token()
        }
    }

    /// `EatToken(kind, code, reportError = true)`.
    pub fn eat_token_code(&mut self, kind: SyntaxKind, code: ErrorCode, report_error: bool) -> Green {
        debug_assert!(facts::is_any_token(kind));
        if self.current_kind() != kind {
            self.create_missing_token_code(kind, code, report_error)
        } else {
            self.eat_token()
        }
    }

    /// `EatTokenEvenWithIncorrectKind(kind)`: the current token whatever its kind, with an error when it is not
    /// `kind`.
    pub fn eat_token_even_with_incorrect_kind(&mut self, kind: SyntaxKind) -> Green {
        let mut token = self.current_token();
        debug_assert!(facts::is_any_token(kind));
        let actual = self.arena_ref().kind(token);
        if actual != kind {
            // getDiagnosticSpan: on the next line, squiggle the end of the previous token, zero wide
            let (offset, width) = if self.prev_trailing_has_end_of_line() {
                let trivia_width = self.trivia_list_full_width(self.prev_token_trailing_trivia);
                let leading = self.arena_ref().leading_trivia_width(token);
                (-((trivia_width + leading) as i32), 0)
            } else {
                (0, self.arena_ref().width(token) as i32)
            };
            let error = self.get_expected_token_error(kind, actual, offset, width);
            token = self.with_additional_diagnostics(token, &[error]);
        }
        self.move_to_next_token();
        token
    }

    /// `EatTokenWithPrejudice(errorCode, args)`.
    pub fn eat_token_with_prejudice(&mut self, error_code: ErrorCode, args: &[DiagArg]) -> Green {
        let token = self.eat_token();
        let width = self.arena_ref().width(token) as i32;
        let error = self.make_error(0, width, error_code, args);
        self.with_additional_diagnostics(token, &[error])
    }

    /// `EatContextualToken(kind, code)`.
    pub fn eat_contextual_token_code(&mut self, kind: SyntaxKind, code: ErrorCode) -> Green {
        debug_assert!(facts::is_any_token(kind));
        if self.current_contextual_kind() != kind {
            self.create_missing_token_code(kind, code, true)
        } else {
            let token = self.eat_token();
            self.convert_to_keyword(token)
        }
    }

    /// `EatContextualToken(kind)`.
    pub fn eat_contextual_token(&mut self, kind: SyntaxKind) -> Green {
        debug_assert!(facts::is_any_token(kind));
        let contextual_kind = self.current_contextual_kind();
        if contextual_kind != kind {
            self.create_missing_token(kind, contextual_kind)
        } else {
            let token = self.eat_token();
            self.convert_to_keyword(token)
        }
    }

    /// `GetExpectedTokenError(expected, actual, offset, width)` (virtual: `DocumentationCommentParser` overrides it).
    pub fn get_expected_token_error(
        &mut self,
        expected: SyntaxKind,
        actual: SyntaxKind,
        offset: i32,
        width: i32,
    ) -> DiagnosticId {
        if self.kind == ParserKind::DocumentationComment {
            return crate::parser::doc_comment_parser::get_expected_token_error(self, expected, actual, offset, width);
        }
        self.base_get_expected_token_error(expected, actual, offset, width)
    }

    /// `SyntaxParser.GetExpectedTokenError`, not overridden.
    pub fn base_get_expected_token_error(
        &mut self,
        expected: SyntaxKind,
        actual: SyntaxKind,
        offset: i32,
        width: i32,
    ) -> DiagnosticId {
        let code = get_expected_token_error_code(expected, actual);
        let args = match code {
            ErrorCode::ERR_SyntaxError => vec![DiagArg::Str(facts::get_text(expected))],
            ErrorCode::ERR_IdentifierExpectedKW => vec![DiagArg::Str(""), DiagArg::Str(facts::get_text(actual))],
            _ => Vec::new(),
        };
        self.make_error(offset, width, code, &args)
    }

    /// `GetExpectedMissingNodeOrTokenError(missingNodeOrToken, expected, actual)` (virtual).
    pub fn get_expected_missing_node_or_token_error(
        &mut self,
        missing_node_or_token: Green,
        expected: SyntaxKind,
        actual: SyntaxKind,
    ) -> DiagnosticId {
        if self.kind == ParserKind::DocumentationComment {
            return crate::parser::doc_comment_parser::get_expected_missing_node_or_token_error(
                self,
                missing_node_or_token,
                expected,
                actual,
            );
        }
        self.base_get_expected_missing_node_or_token_error(missing_node_or_token, expected, actual)
    }

    /// `SyntaxParser.GetExpectedMissingNodeOrTokenError`, not overridden.
    pub fn base_get_expected_missing_node_or_token_error(
        &mut self,
        missing_node_or_token: Green,
        expected: SyntaxKind,
        actual: SyntaxKind,
    ) -> DiagnosticId {
        let (offset, width) = self.get_diagnostic_span_for_missing_node_or_token(missing_node_or_token);
        self.get_expected_token_error(expected, actual, offset, width)
    }

    /// `WithAdditionalDiagnostics(node, diagnostics)` (virtual: a documentation comment parser attaches none unless the
    /// documentation mode is at least `Diagnose`).
    pub fn with_additional_diagnostics(&mut self, node: Green, diagnostics: &[DiagnosticId]) -> Green {
        if self.kind == ParserKind::DocumentationComment && self.options().doc_mode < DocumentationMode::Diagnose {
            return node;
        }
        self.arena().with_additional_diagnostics(node, diagnostics)
    }

    /// `AddError(node, code)`.
    pub fn add_error(&mut self, node: Green, code: ErrorCode) -> Green {
        self.add_error_args(node, code, &[])
    }

    /// `AddErrorAsWarning(node, code, args)`: `WRN_ErrorOverride` with the error inside.
    pub fn add_error_as_warning(&mut self, node: Green, code: ErrorCode, args: &[DiagArg]) -> Green {
        debug_assert!(!self.arena_ref().is_missing(node));
        let inner = self.make_error_node(node, code, args);
        self.add_error_args(
            node,
            ErrorCode::WRN_ErrorOverride,
            &[DiagArg::Info(inner), DiagArg::Int(code.number() as i64)],
        )
    }

    /// `AddError(nodeOrToken, code, args)`: on the node's text, or for a missing one where
    /// `GetDiagnosticSpanForMissingNodeOrToken` says.
    pub fn add_error_args(&mut self, node_or_token: Green, code: ErrorCode, args: &[DiagArg]) -> Green {
        if !self.arena_ref().is_missing(node_or_token) {
            let error = self.make_error_node(node_or_token, code, args);
            self.with_additional_diagnostics(node_or_token, &[error])
        } else {
            let (offset, width) = self.get_diagnostic_span_for_missing_node_or_token(node_or_token);
            let error = self.make_error(offset, width, code, args);
            self.with_additional_diagnostics(node_or_token, &[error])
        }
    }

    /// `GetDiagnosticSpanForMissingNodeOrToken(missingNodeOrToken)`: on the token that follows when it is on the same
    /// line, else zero wide at the end of the previous token; on the skipped text when the missing element has some.
    pub fn get_diagnostic_span_for_missing_node_or_token(&mut self, missing_node_or_token: Green) -> (i32, i32) {
        debug_assert!(self.arena_ref().is_missing(missing_node_or_token));
        if !self.arena_ref().contains_skipped_text(missing_node_or_token) {
            // getOffsetAndWidthBasedOnPriorAndNextTokens
            if self.prev_trailing_has_end_of_line() {
                let leading = self.arena_ref().leading_trivia_width(missing_node_or_token) as i32;
                let trivia = self.trivia_list_full_width(self.prev_token_trailing_trivia) as i32;
                return (-leading - trivia, 0);
            }
            let token = self.current_token();
            let a = self.arena_ref();
            let offset = a.width(missing_node_or_token)
                + a.trailing_trivia_width(missing_node_or_token)
                + a.leading_trivia_width(token);
            return (offset as i32, a.width(token) as i32);
        }
        // getOffsetAndWidthOfSkippedToken
        let mut nodes = std::mem::take(&mut self.node_scratch);
        self.arena().enumerate_nodes(missing_node_or_token, &mut nodes);
        let mut offset = 0i32;
        let mut result = (0, 0);
        let a = self.lexer.arena_ref();
        'children: for &child in &nodes {
            if !child.is_token() {
                continue;
            }
            if !a.contains_skipped_text(child) {
                offset += a.full_width(child) as i32;
                continue;
            }
            // Now, walk the trivia of this token, looking for the skipped tokens trivia.
            for list in [a.leading_trivia(child), a.trailing_trivia(child)] {
                for &trivia in a.trivia_items(list) {
                    if !a.is_skipped_tokens_trivia(trivia) {
                        offset += a.trivia_width(trivia) as i32;
                        continue;
                    }
                    // Found the skipped tokens trivia.  Place the diagnostic on it.
                    result = (offset, a.width(a.trivia_structure(trivia)) as i32);
                    break 'children;
                }
            }
            break;
        }
        self.node_scratch = nodes;
        result
    }

    /// `AddError(node, offset, length, code, args)`.
    pub fn add_error_at(&mut self, node: Green, offset: i32, length: i32, code: ErrorCode, args: &[DiagArg]) -> Green {
        let error = self.make_error(offset, length, code, args);
        self.with_additional_diagnostics(node, &[error])
    }

    /// `AddErrorToFirstToken(node, code, args)`: on the first token's text.
    pub fn add_error_to_first_token(&mut self, node: Green, code: ErrorCode, args: &[DiagArg]) -> Green {
        let first_token = self.arena_ref().first_token(node);
        let width = self.arena_ref().width(first_token) as i32;
        let error = self.make_error(0, width, code, args);
        self.with_additional_diagnostics(node, &[error])
    }

    /// `AddErrorToLastToken(node, code)`: on the last token that is not missing.
    pub fn add_error_to_last_token(&mut self, node: Green, code: ErrorCode) -> Green {
        let (offset, width) = self.get_offset_and_width_for_last_token(node);
        let error = self.make_error(offset, width, code, &[]);
        self.with_additional_diagnostics(node, &[error])
    }

    /// `GetOffsetAndWidthForLastToken(node, out offset, out width)`.
    fn get_offset_and_width_for_last_token(&self, node: Green) -> (i32, i32) {
        let a = self.arena_ref();
        let last_token = a.last_nonmissing_token(node);
        let mut offset = (a.width(node) + a.trailing_trivia_width(node)) as i32; //advance to end of entire node
        let mut width = 0;
        if !last_token.is_null() {
            //will be null if all tokens are missing
            offset -= a.full_width(last_token) as i32; //rewind past last token
            offset += a.leading_trivia_width(last_token) as i32; //advance past last token leading trivia - now at start of last token
            width = a.width(last_token) as i32;
        }
        (offset, width)
    }

    /// `MakeError(offset, width, code, args)`.
    pub fn make_error(&mut self, offset: i32, width: i32, code: ErrorCode, args: &[DiagArg]) -> DiagnosticId {
        self.arena().make_diagnostic(offset, width, code, args)
    }

    /// `MakeError(node, code, args)`: over the node's width.
    pub fn make_error_node(&mut self, node: Green, code: ErrorCode, args: &[DiagArg]) -> DiagnosticId {
        let width = self.arena_ref().width(node) as i32;
        self.make_error(0, width, code, args)
    }

    /// `MakeError(code, args)`: zero wide at the start.
    pub fn make_error_code(&mut self, code: ErrorCode, args: &[DiagArg]) -> DiagnosticId {
        self.make_error(0, 0, code, args)
    }

    /// `AddLeadingSkippedSyntax(node, skippedSyntax)`: `skipped` before the node's first token, as trivia.
    pub fn add_leading_skipped_syntax(&mut self, node: Green, skipped_syntax: Green) -> Green {
        if skipped_syntax.is_null() {
            return node;
        }
        let old_token = if node.is_token() { node } else { self.arena_ref().first_token(node) };
        let new_token = self.add_skipped_syntax(old_token, skipped_syntax, false);
        let delta = self.arena_ref().full_width(skipped_syntax) as i32;
        self.arena().replace_first_token(node, new_token, delta)
    }

    /// `AddTrailingSkippedSyntax(list, skippedSyntax)`: after the list's last element.
    pub fn add_trailing_skipped_syntax_to_list(&mut self, list: &mut [Green], skipped_syntax: Green) {
        if let Some(last) = list.last_mut() {
            *last = self.add_trailing_skipped_syntax(*last, skipped_syntax);
        }
    }

    /// `AddTrailingSkippedSyntax(node, skippedSyntax)`: `skipped` after the node's last token, as trivia.
    pub fn add_trailing_skipped_syntax(&mut self, node: Green, skipped_syntax: Green) -> Green {
        if node.is_token() {
            self.add_skipped_syntax(node, skipped_syntax, true)
        } else {
            let last_token = self.arena_ref().last_token(node);
            let new_token = self.add_skipped_syntax(last_token, skipped_syntax, true);
            self.arena().replace_last_token(node, new_token)
        }
    }

    /// `AddSkippedSyntax(target, skippedSyntax, trailing)`: every token of `skipped` (its trivia beside it, itself as
    /// a `SkippedTokensTrivia` when it has text) added to the target's trivia, and the first diagnostic found on a
    /// node or an empty token of `skipped` moved to the target.
    pub fn add_skipped_syntax(&mut self, target: Green, skipped_syntax: Green, trailing: bool) -> Green {
        let mut builder = std::mem::take(&mut self.trivia_scratch);
        builder.clear();
        let mut target = target;
        let mut current_offset: i32;
        {
            let a = self.lexer.arena_ref();
            if trailing {
                // The normal offset for a node/token is its start (not full start).  So if we're placing the skipped
                // syntax at the end of the trivia, then the offset relative to the node/token start will be adjusted
                // forward by the width of the node/token plus the existing trailing trivia.
                current_offset = (a.width(target) + a.trailing_trivia_width(target)) as i32;
                builder.extend_from_slice(a.trivia_items(a.trailing_trivia(target)));
            } else {
                // The normal offset for a node/token is its start (not full start). So if we're placing the skipped
                // syntax at the start of the trivia, then the offset relative to the node/token start will be adjusted
                // backward by the width of the existing leading trivia plus the width of the skipped syntax we're
                // tacking on at the front.
                current_offset = -(a.leading_trivia_width(target) as i32) - a.full_width(skipped_syntax) as i32;
            }
        }
        // the error in we'll attach to the node
        let mut diagnostic: Option<DiagnosticId> = None;
        let mut final_diagnostic_offset = 0i32;
        let mut nodes = std::mem::take(&mut self.node_scratch);
        self.arena().enumerate_nodes(skipped_syntax, &mut nodes);
        for &node in &nodes {
            if node.is_token() {
                let token = node;
                // Strip the leading trivia of the token, and add it to the target's final trivia list.
                let leading = self.arena_ref().leading_trivia(token);
                builder.extend_from_slice(self.arena_ref().trivia_items(leading));
                if self.arena_ref().width(token) > 0 {
                    // Then add the token (stripped of its own trivia) to the target's final trivia list.
                    let a = self.arena();
                    let bare = a.token_with_leading_trivia(token, TriviaList::EMPTY);
                    let bare = a.token_with_trailing_trivia(bare, TriviaList::EMPTY);
                    let skipped = a.skipped_tokens_trivia(bare);
                    builder.push(a.structured_trivia(skipped));
                } else {
                    // Do not bother adding zero-width tokens to target's final trivia list.  Lots of code (like
                    // GetStructure) does not like it at all. But do keep around any diagnostics that might have
                    // been on this zero width token, and move it to the target.
                    let a = self.arena_ref();
                    if let Some(existing) = a.diagnostics(token).next() {
                        diagnostic = Some(existing);
                        final_diagnostic_offset =
                            current_offset + a.leading_trivia_width(token) as i32 + a.diagnostic(existing).offset;
                    }
                }
                // Finally strip the trailing trivia of the token, and add it to the target's final list.
                let a = self.arena_ref();
                builder.extend_from_slice(a.trivia_items(a.trailing_trivia(token)));
                current_offset += a.full_width(token) as i32;
            } else {
                let a = self.arena_ref();
                if a.contains_diagnostics(node) && diagnostic.is_none() {
                    // Ensure we don't lose any diagnostics on non-token nodes that we're diving into.
                    // Only propagate the first error to reduce noise:
                    if let Some(existing) = a.diagnostics(node).next() {
                        diagnostic = Some(existing);
                        final_diagnostic_offset =
                            current_offset + a.leading_trivia_width(node) as i32 + a.diagnostic(existing).offset;
                    }
                }
            }
        }
        self.node_scratch = nodes;
        // If we found a diagnostic on a node (or empty-width token) in the skipped syntax, ensure it is moved
        // over to the target.
        if let Some(d) = diagnostic {
            let moved = self.arena().diagnostic_with_offset(d, final_diagnostic_offset);
            target = self.with_additional_diagnostics(target, &[moved]);
        }
        // If we were adding the skipped token as trailing trivia, then at this point we're done.  Otherwise, we
        // were adding it as leading trivia, so we need to tack on the existing leading trivia of the target.
        let result = if trailing {
            let a = self.arena();
            let list = a.trivia_list(&builder);
            a.token_with_trailing_trivia(target, list)
        } else {
            let a = self.lexer.arena();
            let leading = a.leading_trivia(target);
            builder.extend_from_slice(a.trivia_items(leading));
            let list = a.trivia_list(&builder);
            a.token_with_leading_trivia(target, list)
        };
        self.trivia_scratch = builder;
        result
    }

    /// `ConvertToKeyword(token)`: a token of its contextual kind (`SyntaxFactory.Token`/`MissingToken` with the same
    /// trivia, the text the kind's), its diagnostics kept.
    pub fn convert_to_keyword(&mut self, token: Green) -> Green {
        let a = self.arena();
        let kind = a.kind(token);
        let contextual = a.contextual_kind(token);
        if kind != contextual {
            let leading = a.leading_trivia(token);
            let trailing = a.trailing_trivia(token);
            let kw = if a.is_missing(token) {
                a.missing_token_with_trivia(leading, contextual, trailing)
            } else {
                a.token_with_trivia(leading, contextual, trailing)
            };
            let d: Vec<DiagnosticId> = a.diagnostics(token).collect();
            if !d.is_empty() {
                return a.with_diagnostics(kw, &d);
            }
            return kw;
        }
        token
    }

    /// `ConvertToIdentifier(token)`: an identifier token of the same text whose contextual kind is the token's kind.
    pub fn convert_to_identifier(&mut self, token: Green) -> Green {
        let a = self.arena();
        debug_assert!(!a.is_missing(token));
        let kind = a.kind(token);
        a.token_with_kind(token, SyntaxKind::IdentifierToken, kind)
    }

    /// `Directives`.
    pub fn directives(&self) -> DirectiveStack {
        self.lexer.directives()
    }

    /// `CheckFeatureAvailability(node, feature, forceWarning)`: the "not available in version N" (or "in preview")
    /// error on `node` when the language version lacks `feature`.
    pub fn check_feature_availability(&mut self, node: Green, feature: MessageID, force_warning: bool) -> Green {
        let version = self.options().version;
        if version.is_feature_enabled(feature) {
            return node;
        }
        // GetDisabledFeatureDiagnosticInfo (no feature has a RequiredFeature flag at the pin)
        let required = feature.required_version().unwrap_or(LanguageVersion::Preview);
        let (code, args) = if required == LanguageVersion::Preview {
            (ErrorCode::ERR_FeatureInPreview, vec![DiagArg::Feature(feature)])
        } else {
            let code = version.get_error_code().unwrap_or(ErrorCode::ERR_FeatureInPreview);
            (code, vec![DiagArg::Feature(feature), DiagArg::Version(required)])
        };
        if force_warning {
            let info = self.make_error_code(code, &args);
            return self.add_error_args(
                node,
                ErrorCode::WRN_ErrorOverride,
                &[DiagArg::Info(info), DiagArg::Int(code.number() as i64)],
            );
        }
        self.add_error_args(node, code, &args)
    }

    /// `IsFeatureEnabled(feature)`.
    pub fn is_feature_enabled(&self, feature: MessageID) -> bool {
        self.options().version.is_feature_enabled(feature)
    }

    /// `IsMakingProgress(ref lastTokenPosition, assertIfFalse)`: whether the parser moved since the last call.
    pub fn is_making_progress(&mut self, last_token_position: &mut i32, assert_if_false: bool) -> bool {
        let pos = self.current_token_position();
        if pos > *last_token_position {
            *last_token_position = pos;
            return true;
        }
        debug_assert!(!assert_if_false);
        false
    }

    /// `CurrentTokenPosition`.
    pub fn current_token_position(&self) -> i32 {
        self.first_token + self.token_offset
    }

    /// `_prevTokenTrailingTrivia`.
    pub fn prev_token_trailing_trivia(&self) -> TriviaList {
        self.prev_token_trailing_trivia
    }

    /// `new SyntaxList<CSharpSyntaxNode>(_prevTokenTrailingTrivia).Any(SyntaxKind.EndOfLineTrivia)`.
    fn prev_trailing_has_end_of_line(&self) -> bool {
        let a = self.arena_ref();
        a.trivia_items(self.prev_token_trailing_trivia).iter().any(|&t| a.trivia_kind(t) == SyntaxKind::EndOfLineTrivia)
    }

    fn trivia_list_full_width(&self, list: TriviaList) -> u32 {
        let a = self.arena_ref();
        a.trivia_items(list).iter().map(|&t| a.trivia_width(t)).fold(0, u32::wrapping_add)
    }
}

/// `GetExpectedTokenErrorCode(expected, actual)`.
fn get_expected_token_error_code(expected: SyntaxKind, actual: SyntaxKind) -> ErrorCode {
    match expected {
        SyntaxKind::IdentifierToken => {
            if facts::is_reserved_keyword(actual) {
                ErrorCode::ERR_IdentifierExpectedKW // A keyword -- use special message.
            } else {
                ErrorCode::ERR_IdentifierExpected
            }
        }
        SyntaxKind::SemicolonToken => ErrorCode::ERR_SemicolonExpected,
        // case TokenKind::Colon:         iError = ERR_ColonExpected;          break;
        // case TokenKind::OpenParen:     iError = ERR_LparenExpected;         break;
        SyntaxKind::CloseParenToken => ErrorCode::ERR_CloseParenExpected,
        SyntaxKind::OpenBraceToken => ErrorCode::ERR_LbraceExpected,
        SyntaxKind::CloseBraceToken => ErrorCode::ERR_RbraceExpected,
        // case TokenKind::CloseSquare:   iError = ERR_CloseSquareExpected;    break;
        _ => ErrorCode::ERR_SyntaxError,
    }
}

/// The length of UTF-8 `bytes` in UTF-16 code units (a 4-byte sequence is two; an invalid byte one).
fn utf16_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .map(|&b| {
            if b & 0xC0 == 0x80 {
                0
            } else if b >= 0xF0 {
                2
            } else {
                1
            }
        })
        .sum()
}
