//! Kotlin's builder layers over the platform's `PsiBuilderImpl` (`psi_builder`): its binders (the platform's
//! `WhitespacesBinders` and `KotlinWhitespaceAndCommentsBinders`) and `SemanticWhitespaceAwarePsiBuilderImpl` with its
//! adapters `TruncatedSemanticWhitespaceAwarePsiBuilder` and `SemanticWhitespaceAwarePsiBuilderForByClause`. The
//! builder's names are re-exported here, for Kotlin's types, so the parsers name them as before.

use std::borrow::Cow;

pub use psi_builder::builder::{
    AsPsiBuilder, Marker, NESTING_CHAMELEON, NESTING_LIMIT, Nesting, REPARSED_LIMIT, TOO_DEEPLY_NESTED,
    TYPE_ARGUMENTS_LIMIT,
};

use crate::Kotlin;
use crate::tokens::{TokenKind, TokenSet};

/// `PsiBuilderImpl` over Kotlin's tokens.
pub type PsiBuilderImpl = psi_builder::PsiBuilderImpl<Kotlin>;
/// The buffers a Kotlin builder works in.
pub type BuilderBuffers = psi_builder::BuilderBuffers<Kotlin>;

/// Kotlin's `WhitespacesAndCommentsBinder`s: where a marker's edge goes among the whitespace and comments around it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Binder {
    /// `WhitespacesBinders.DEFAULT_LEFT_BINDER` (= `GREEDY_RIGHT_BINDER`): the element starts after them.
    DefaultLeft,
    /// `WhitespacesBinders.DEFAULT_RIGHT_BINDER` (= `GREEDY_LEFT_BINDER`): the element ends before them.
    DefaultRight,
    /// `PrecedingCommentsBinder`.
    PrecedingComments,
    /// `PrecedingDocCommentsBinder`.
    PrecedingDocComments,
    /// `TrailingCommentsBinder`: binds comments on the same line.
    TrailingComments,
    /// `PRECEDING_ALL_COMMENTS_BINDER` (`AllCommentsBinder(false)`).
    PrecedingAllComments,
    /// `TRAILING_ALL_COMMENTS_BINDER` (`AllCommentsBinder(true)`).
    TrailingAllComments,
    /// `DoNotBindAnything`.
    DoNotBindAnything,
    /// `BindFirstShebangWithWhitespaceOnly`.
    BindFirstShebangWithWhitespaceOnly,
    /// `PRECEDING_ALL_BINDER` (`BindAll(false)`).
    PrecedingAll,
    /// `TRAILING_ALL_BINDER` (`BindAll(true)`).
    TrailingAll,
}

/// `WhitespacesBinders.DEFAULT_LEFT_BINDER`.
pub const DEFAULT_LEFT_BINDER: Binder = Binder::DefaultLeft;
/// `WhitespacesBinders.DEFAULT_RIGHT_BINDER`.
pub const DEFAULT_RIGHT_BINDER: Binder = Binder::DefaultRight;
/// `WhitespacesBinders.GREEDY_LEFT_BINDER`, the same object as `DEFAULT_RIGHT_BINDER`.
pub const GREEDY_LEFT_BINDER: Binder = Binder::DefaultRight;
/// `WhitespacesBinders.GREEDY_RIGHT_BINDER`, the same object as `DEFAULT_LEFT_BINDER`.
pub const GREEDY_RIGHT_BINDER: Binder = Binder::DefaultLeft;
/// `KotlinWhitespaceAndCommentsBindersKt.PRECEDING_ALL_COMMENTS_BINDER`.
pub const PRECEDING_ALL_COMMENTS_BINDER: Binder = Binder::PrecedingAllComments;
/// `KotlinWhitespaceAndCommentsBindersKt.TRAILING_ALL_COMMENTS_BINDER`.
pub const TRAILING_ALL_COMMENTS_BINDER: Binder = Binder::TrailingAllComments;
/// `KotlinWhitespaceAndCommentsBindersKt.PRECEDING_ALL_BINDER`.
pub const PRECEDING_ALL_BINDER: Binder = Binder::PrecedingAll;
/// `KotlinWhitespaceAndCommentsBindersKt.TRAILING_ALL_BINDER`.
pub const TRAILING_ALL_BINDER: Binder = Binder::TrailingAll;

fn line_break_count(text: &[u8]) -> usize {
    text.iter().filter(|&&b| b == b'\n').count()
}

fn contains_line_break(text: &[u8]) -> bool {
    text.iter().any(|&b| b == b'\n' || b == b'\r')
}

impl psi_builder::WhitespacesAndCommentsBinder<TokenKind> for Binder {
    const DEFAULT_LEFT_BINDER: Binder = Binder::DefaultLeft;
    const DEFAULT_RIGHT_BINDER: Binder = Binder::DefaultRight;

    // `isRecursive()`: none of these binders is (the default, false).

    /// `getEdgePosition(tokens, atStreamEdge, getter)`: the edge as an index into `tokens`.
    fn get_edge_position<'t>(
        self,
        tokens: &[TokenKind],
        _at_stream_edge: bool,
        getter: &dyn Fn(usize) -> &'t [u8],
    ) -> usize {
        use TokenKind::*;
        match self {
            Binder::DefaultLeft => tokens.len(),
            Binder::DefaultRight => 0,
            Binder::PrecedingComments => {
                if tokens.is_empty() {
                    return 0;
                }
                // 1. bind doc comment
                for idx in (0..tokens.len()).rev() {
                    if tokens[idx] == DOC_COMMENT {
                        return idx;
                    }
                }
                // 2. bind plain comments
                let mut result = tokens.len();
                for idx in (0..tokens.len()).rev() {
                    let token_type = tokens[idx];
                    if token_type == WHITE_SPACE {
                        if line_break_count(getter(idx)) > 1 {
                            break;
                        }
                    } else if crate::tokens::COMMENTS.contains(token_type) {
                        if idx == 0 || tokens[idx - 1] == WHITE_SPACE && contains_line_break(getter(idx - 1)) {
                            result = idx;
                        }
                    } else {
                        break;
                    }
                }
                result
            }
            Binder::PrecedingDocComments => {
                if tokens.is_empty() {
                    return 0;
                }
                for idx in (0..tokens.len()).rev() {
                    if tokens[idx] == DOC_COMMENT {
                        return idx;
                    }
                }
                tokens.len()
            }
            Binder::TrailingComments => {
                if tokens.is_empty() {
                    return 0;
                }
                let mut result = 0;
                for (idx, &token_type) in tokens.iter().enumerate() {
                    match token_type {
                        WHITE_SPACE => {
                            if contains_line_break(getter(idx)) {
                                break;
                            }
                        }
                        EOL_COMMENT | BLOCK_COMMENT => result = idx + 1,
                        _ => break,
                    }
                }
                result
            }
            Binder::PrecedingAllComments | Binder::TrailingAllComments => {
                if tokens.is_empty() {
                    return 0;
                }
                let is_trailing = self == Binder::TrailingAllComments;
                let size = tokens.len();
                // Skip one whitespace if needed. Expect that there can't be several consecutive whitespaces
                let end_token = tokens[if is_trailing { size - 1 } else { 0 }];
                let shift = if end_token == WHITE_SPACE { 1 } else { 0 };
                if is_trailing { size - shift } else { shift }
            }
            Binder::DoNotBindAnything => 0,
            Binder::BindFirstShebangWithWhitespaceOnly => {
                if tokens.first() == Some(&SHEBANG_COMMENT) {
                    return if tokens.get(1) == Some(&WHITE_SPACE) { 2 } else { 1 };
                }
                0
            }
            Binder::PrecedingAll => 0,
            Binder::TrailingAll => tokens.len(),
        }
    }

    /// `getEdgePosition` on a token view of negative `size` (see the trait).
    fn negative_size_edge<'t>(
        self,
        size: i32,
        get: impl Fn(i32) -> Option<TokenKind>,
        _text: &dyn Fn(i32) -> &'t [u8],
    ) -> i32 {
        match self {
            Binder::DefaultLeft | Binder::PrecedingComments | Binder::PrecedingDocComments | Binder::TrailingAll => {
                size
            }
            Binder::DefaultRight | Binder::TrailingComments | Binder::DoNotBindAnything | Binder::PrecedingAll => 0,
            Binder::PrecedingAllComments => (get(0) == TokenKind::WHITE_SPACE) as i32,
            Binder::TrailingAllComments => size - (get(size - 1) == TokenKind::WHITE_SPACE) as i32,
            Binder::BindFirstShebangWithWhitespaceOnly => (get(0) == TokenKind::SHEBANG_COMMENT) as i32,
        }
    }
}

/// A builder adapter of the Kotlin parser, applied on top of `SemanticWhitespaceAwarePsiBuilderImpl`.
#[derive(Copy, Clone, Debug)]
pub enum Adapter {
    /// `TruncatedSemanticWhitespaceAwarePsiBuilder`: the stream ends at `eof_position` (an offset; < 0: no limit).
    Truncated { eof_position: i32 },
    /// `SemanticWhitespaceAwarePsiBuilderForByClause`: counts its newline-state pushes.
    ForByClause { stack_size: i32 },
}

/// The state stacks of `SemanticWhitespaceAwarePsiBuilderImpl`, kept in `BuilderBuffers::layer` from one builder to
/// the next.
#[derive(Default)]
pub struct SemanticBuffers {
    join_complex_tokens: Vec<bool>,
    newlines_enabled: Vec<bool>,
    adapters: Vec<Adapter>,
}

/// `SemanticWhitespaceAwarePsiBuilderImpl.getJoinedTokenType(rawTokenType, rawLookupSteps)`: `?` before `.` is
/// `SAFE_ACCESS`, before `:` `ELVIS`; `!` before `!` is `EXCLEXCL` (`Language::joined_token_type`, which the
/// delegate's per-position cache computes).
pub(crate) fn joined_token_type(
    delegate: &PsiBuilderImpl,
    raw_token_type: Option<TokenKind>,
    raw_lookup_steps: i32,
) -> Option<TokenKind> {
    if raw_token_type == TokenKind::QUEST {
        let next_raw_token = delegate.raw_lookup(raw_lookup_steps);
        if next_raw_token == TokenKind::DOT {
            return Some(TokenKind::SAFE_ACCESS);
        }
        if next_raw_token == TokenKind::COLON {
            return Some(TokenKind::ELVIS);
        }
    } else if raw_token_type == TokenKind::EXCL {
        let next_raw_token = delegate.raw_lookup(raw_lookup_steps);
        if next_raw_token == TokenKind::EXCL {
            return Some(TokenKind::EXCLEXCL);
        }
    }
    raw_token_type
}

/// `SemanticWhitespaceAwarePsiBuilderImpl`: the newline mode stack and the joining of `?.`, `?:`, `!!` over a
/// `PsiBuilderImpl`. The adapters live here as a stack; a `SemanticWhitespaceAwarePsiBuilder` handle sees a prefix.
pub struct SemanticWhitespaceAwarePsiBuilderImpl {
    pub delegate: PsiBuilderImpl,
    join_complex_tokens: Vec<bool>,
    newlines_enabled: Vec<bool>,
    adapters: Vec<Adapter>,
}

/// `complexTokens = TokenSet.create(SAFE_ACCESS, ELVIS, EXCLEXCL)`.
const COMPLEX_TOKENS: TokenSet = TokenSet::create(&[TokenKind::SAFE_ACCESS, TokenKind::ELVIS, TokenKind::EXCLEXCL]);

impl AsPsiBuilder for SemanticWhitespaceAwarePsiBuilderImpl {
    type Lang = Kotlin;
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl {
        &mut self.delegate
    }
}

impl SemanticWhitespaceAwarePsiBuilderImpl {
    /// The semantic layer over `delegate`, its stacks and adapters in `bufs` (emptied until `release`).
    pub(crate) fn new(delegate: PsiBuilderImpl, bufs: &mut BuilderBuffers) -> Self {
        let mut join_complex_tokens = std::mem::take(&mut bufs.layer.join_complex_tokens);
        join_complex_tokens.clear();
        join_complex_tokens.push(true);
        let mut newlines_enabled = std::mem::take(&mut bufs.layer.newlines_enabled);
        newlines_enabled.clear();
        newlines_enabled.push(true);
        let mut adapters = std::mem::take(&mut bufs.layer.adapters);
        adapters.clear();
        SemanticWhitespaceAwarePsiBuilderImpl { delegate, join_complex_tokens, newlines_enabled, adapters }
    }

    /// Gives this layer's buffers back to `bufs` and returns the delegate, which builds the tree.
    pub(crate) fn into_delegate(self, bufs: &mut BuilderBuffers) -> PsiBuilderImpl {
        bufs.layer.join_complex_tokens = self.join_complex_tokens;
        bufs.layer.newlines_enabled = self.newlines_enabled;
        bufs.layer.adapters = self.adapters;
        self.delegate
    }

    /// A handle without adapters: what `KotlinParser` passes to `KotlinParsing.createForTopLevel`.
    pub fn handle(&mut self) -> SemanticWhitespaceAwarePsiBuilder<'_> {
        SemanticWhitespaceAwarePsiBuilder { imp: self, depth: 0, top_trunc: None, min_eof: i32::MAX }
    }

    /// `isWhitespaceOrComment(elementType)`.
    pub fn is_whitespace_or_comment(&self, element_type: TokenKind) -> bool {
        self.delegate.whitespace_or_comment(element_type)
    }

    /// `newlineBeforeCurrentToken()`: whether the whitespace before the current token (comments skipped) has a newline.
    pub fn newline_before_current_token(&mut self) -> bool {
        crate::counters::add(crate::counters::Counter::NewlineBefore, 1);
        if !self.newlines_enabled.last().copied().unwrap_or(true) {
            return false;
        }
        if self.delegate.eof() {
            return true;
        }
        let current_offset = self.delegate.get_current_offset();
        let mut i = 1;
        while i <= current_offset {
            let previous_token = self.delegate.raw_lookup(-i);
            if matches!(
                previous_token,
                Some(
                    TokenKind::BLOCK_COMMENT
                        | TokenKind::DOC_COMMENT
                        | TokenKind::EOL_COMMENT
                        | TokenKind::SHEBANG_COMMENT
                )
            ) {
                i += 1;
                continue;
            }
            if previous_token != TokenKind::WHITE_SPACE {
                break;
            }
            let previous_token_start = self.delegate.raw_token_type_start(-i).max(0) as usize;
            let previous_token_end = self.delegate.raw_token_type_start(-i + 1).max(0) as usize;
            let text = self.delegate.get_original_text();
            if text[previous_token_start..previous_token_end.min(text.len())].contains(&b'\n') {
                return true;
            }
            i += 1;
        }
        false
    }

    pub fn disable_newlines(&mut self) {
        self.newlines_enabled.push(false);
    }

    pub fn enable_newlines(&mut self) {
        self.newlines_enabled.push(true);
    }

    /// `restoreNewlinesState()`; the initial state is never popped (Java asserts).
    pub fn restore_newlines_state(&mut self) {
        if self.newlines_enabled.len() > 1 {
            self.newlines_enabled.pop();
        }
    }

    fn join_complex_tokens(&self) -> bool {
        self.join_complex_tokens.last().copied().unwrap_or(true)
    }

    pub fn restore_joining_complex_tokens_state(&mut self) {
        if self.join_complex_tokens.len() > 1 {
            self.join_complex_tokens.pop();
        }
    }

    pub fn enable_joining_complex_tokens(&mut self) {
        self.join_complex_tokens.push(true);
    }

    pub fn disable_joining_complex_tokens(&mut self) {
        self.join_complex_tokens.push(false);
    }

    /// `getTokenType()`: `?` before `.` is `SAFE_ACCESS`, before `:` `ELVIS`; `!` before `!` is `EXCLEXCL`.
    /// The raw type and `getJoinedTokenType(raw, 1)` come from the delegate's per-position cache (`current_types`).
    #[inline(always)]
    pub fn get_token_type(&mut self) -> Option<TokenKind> {
        let (raw, joined) = self.delegate.current_types();
        if !self.join_complex_tokens() {
            return raw;
        }
        joined
    }

    /// `getJoinedTokenType(rawTokenType, rawLookupSteps)`.
    fn get_joined_token_type(&self, raw_token_type: Option<TokenKind>, raw_lookup_steps: i32) -> Option<TokenKind> {
        joined_token_type(&self.delegate, raw_token_type, raw_lookup_steps)
    }

    /// `advanceLexer()`: a joined token is two lexemes collapsed into one leaf.
    pub fn advance_lexer(&mut self) {
        if !self.join_complex_tokens() {
            self.delegate.advance_lexer();
            return;
        }
        let token_type = self.get_token_type();
        match token_type {
            Some(t) if COMPLEX_TOKENS.contains(t) => {
                let mark = self.delegate.mark();
                self.delegate.advance_lexer();
                self.delegate.advance_lexer();
                self.delegate.collapse(mark, t);
            }
            _ => self.delegate.advance_lexer(),
        }
    }

    /// `getTokenText()`.
    pub fn get_token_text(&mut self) -> Option<&str> {
        if self.join_complex_tokens() {
            match self.get_token_type() {
                Some(TokenKind::ELVIS) => return Some("?:"),
                Some(TokenKind::SAFE_ACCESS) => return Some("?."),
                _ => {}
            }
        }
        self.delegate.get_token_text()
    }

    /// `getTokenText()` as bytes, with no UTF-8 check: what the parser compares with keyword and word texts (invalid
    /// UTF-8, which `get_token_text` shows as U+FFFD, matches none of them either way).
    pub fn get_token_bytes(&mut self) -> Option<&[u8]> {
        if self.join_complex_tokens() {
            match self.get_token_type() {
                Some(TokenKind::ELVIS) => return Some(b"?:"),
                Some(TokenKind::SAFE_ACCESS) => return Some(b"?."),
                _ => {}
            }
        }
        self.delegate.get_token_bytes()
    }

    /// `lookAhead(steps)`.
    pub fn look_ahead(&mut self, steps: i32) -> Option<TokenKind> {
        if !self.join_complex_tokens() {
            return self.delegate.look_ahead(steps);
        }
        if COMPLEX_TOKENS.contains(self.get_token_type()) {
            return self.delegate.look_ahead(steps + 1);
        }
        let raw = self.delegate.look_ahead(steps);
        self.get_joined_token_type(raw, 2)
    }
}

/// `SemanticWhitespaceAwarePsiBuilder` as a parser holds it: the builder seen through the first `depth` adapters.
/// `wrapped` puts a new adapter on top, as `new TruncatedSemanticWhitespaceAwarePsiBuilder(builder, eof)` and
/// `new SemanticWhitespaceAwarePsiBuilderForByClause(builder)` do; the Java adapters live as long as the parser that
/// holds them, which the borrow mirrors.
///
/// `top_trunc` and `min_eof` cache what the first `depth` adapters say on every `getTokenType()`: the topmost
/// truncation's `eof_position` and the smallest non-negative one (`i32::MAX` without). They are computed in `handle`,
/// `reborrow` and `wrapped` and stay right by the borrow: `wrapped(self)` consumes the handle and `reborrow` freezes
/// its parent, so while a handle lives every other live handle sits on a prefix at least `depth` long, `wrapped`
/// truncates no shorter than its own `depth`, and the only write into the prefix (`count_newline_state`) touches
/// by-clause stack sizes, not truncations.
pub struct SemanticWhitespaceAwarePsiBuilder<'b> {
    imp: &'b mut SemanticWhitespaceAwarePsiBuilderImpl,
    depth: usize,
    top_trunc: Option<i32>,
    min_eof: i32,
}

impl AsPsiBuilder for SemanticWhitespaceAwarePsiBuilder<'_> {
    type Lang = Kotlin;
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl {
        &mut self.imp.delegate
    }
}

impl<'b> SemanticWhitespaceAwarePsiBuilder<'b> {
    /// The same builder for a shorter borrow (to hand to a nested parser).
    pub fn reborrow(&mut self) -> SemanticWhitespaceAwarePsiBuilder<'_> {
        SemanticWhitespaceAwarePsiBuilder {
            imp: self.imp,
            depth: self.depth,
            top_trunc: self.top_trunc,
            min_eof: self.min_eof,
        }
    }

    /// This builder under a new adapter.
    pub fn wrapped(self, adapter: Adapter) -> SemanticWhitespaceAwarePsiBuilder<'b> {
        self.imp.adapters.truncate(self.depth);
        self.imp.adapters.push(adapter);
        let (top_trunc, min_eof) = match adapter {
            Adapter::Truncated { eof_position } => {
                (Some(eof_position), if eof_position >= 0 { self.min_eof.min(eof_position) } else { self.min_eof })
            }
            Adapter::ForByClause { .. } => (self.top_trunc, self.min_eof),
        };
        SemanticWhitespaceAwarePsiBuilder { depth: self.depth + 1, imp: self.imp, top_trunc, min_eof }
    }

    fn adapters(&self) -> &[Adapter] {
        &self.imp.adapters[..self.depth.min(self.imp.adapters.len())]
    }

    /// The topmost truncation: what `lookAhead` goes through (cached, see the struct).
    #[inline]
    fn top_truncated(&self) -> Option<i32> {
        self.top_trunc
    }

    /// `SemanticWhitespaceAwarePsiBuilderForByClause.getStackSize()` of the by-clause adapter this builder was created
    /// with (the top one); 0 without one.
    pub fn get_stack_size(&self) -> i32 {
        match self.adapters().last() {
            Some(Adapter::ForByClause { stack_size }) => *stack_size,
            _ => 0,
        }
    }

    fn count_newline_state(&mut self, delta: i32) {
        let depth = self.depth.min(self.imp.adapters.len());
        for adapter in &mut self.imp.adapters[..depth] {
            if let Adapter::ForByClause { stack_size } = adapter {
                *stack_size += delta;
            }
        }
    }

    // SemanticWhitespaceAwarePsiBuilder

    pub fn newline_before_current_token(&mut self) -> bool {
        self.imp.newline_before_current_token()
    }

    pub fn disable_newlines(&mut self) {
        self.count_newline_state(1);
        self.imp.disable_newlines();
    }

    pub fn enable_newlines(&mut self) {
        self.count_newline_state(1);
        self.imp.enable_newlines();
    }

    pub fn restore_newlines_state(&mut self) {
        self.count_newline_state(-1);
        self.imp.restore_newlines_state();
    }

    pub fn restore_joining_complex_tokens_state(&mut self) {
        self.imp.restore_joining_complex_tokens_state();
    }

    pub fn enable_joining_complex_tokens(&mut self) {
        self.imp.enable_joining_complex_tokens();
    }

    pub fn disable_joining_complex_tokens(&mut self) {
        self.imp.disable_joining_complex_tokens();
    }

    pub fn is_whitespace_or_comment(&self, element_type: TokenKind) -> bool {
        self.imp.is_whitespace_or_comment(element_type)
    }

    // PsiBuilder, through the adapters

    fn is_offset_beyond_eof(eof_position: i32, offset_from_current: i32) -> bool {
        eof_position >= 0 && offset_from_current >= eof_position
    }

    /// `eof()`: the stream's end, or the current offset at or beyond a truncation's end. Any truncation in the chain
    /// ends the stream, so the smallest non-negative `eof_position` (`min_eof`) decides.
    #[inline(always)]
    pub fn eof(&mut self) -> bool {
        if self.imp.delegate.eof() {
            return true;
        }
        if self.top_truncated().is_none() {
            return false;
        }
        self.imp.delegate.get_current_offset() >= self.min_eof
    }

    /// `getTokenType()`.
    #[inline(always)]
    pub fn get_token_type(&mut self) -> Option<TokenKind> {
        crate::counters::add(crate::counters::Counter::TokenType, 1);
        if self.top_truncated().is_some() && self.eof() {
            return None;
        }
        self.imp.get_token_type()
    }

    /// `getTokenText()`.
    pub fn get_token_text(&mut self) -> Option<&str> {
        if self.top_truncated().is_some() && self.eof() {
            return None;
        }
        self.imp.get_token_text()
    }

    /// `getTokenText()` as bytes (see `SemanticWhitespaceAwarePsiBuilderImpl::get_token_bytes`).
    pub fn get_token_bytes(&mut self) -> Option<&[u8]> {
        if self.top_truncated().is_some() && self.eof() {
            return None;
        }
        self.imp.get_token_bytes()
    }

    /// `lookAhead(steps)`.
    pub fn look_ahead(&mut self, steps: i32) -> Option<TokenKind> {
        let Some(eof_position) = self.top_truncated() else { return self.imp.look_ahead(steps) };
        if self.eof() {
            return None;
        }
        let raw_look_ahead_steps = self.raw_look_ahead(steps);
        if Self::is_offset_beyond_eof(eof_position, self.imp.delegate.raw_token_type_start(raw_look_ahead_steps)) {
            return None;
        }
        self.imp.delegate.raw_lookup(raw_look_ahead_steps)
    }

    /// `TruncatedSemanticWhitespaceAwarePsiBuilder.rawLookAhead(steps)`.
    fn raw_look_ahead(&self, mut steps: i32) -> i32 {
        let mut cur = 0;
        while steps > 0 {
            cur += 1;
            let mut raw_token_type = self.imp.delegate.raw_lookup(cur);
            while let Some(t) = raw_token_type {
                if !self.is_whitespace_or_comment(t) {
                    break;
                }
                cur += 1;
                raw_token_type = self.imp.delegate.raw_lookup(cur);
            }
            steps -= 1;
        }
        cur
    }

    /// `advanceLexer()`.
    pub fn advance_lexer(&mut self) {
        self.imp.advance_lexer();
    }

    /// `mark()`.
    pub fn mark(&mut self) -> Marker {
        self.imp.delegate.mark()
    }

    /// `error(message)`.
    pub fn error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.imp.delegate.error(message);
    }

    /// `remapCurrentToken(type)`.
    pub fn remap_current_token(&mut self, element_type: TokenKind) {
        self.imp.delegate.remap_current_token(element_type);
    }

    /// `KotlinParsing.advanceBalancedBlock()` as one scan of the raw tokens (`PsiBuilderImpl::skip_balanced_block`)
    /// when no truncation applies; false, with nothing done, under a `TruncatedSemanticWhitespaceAwarePsiBuilder`,
    /// whose end the Java loop must check on every token.
    ///
    /// The Java loop advances token by token, so with complex tokens joined each `?.`, `?:` and `!!` in the block
    /// becomes a collapse marker (`advance_lexer`); the scan adds none, and no output can show it. Both callers collapse
    /// the marker around the block (`parseBlock` into `BLOCK`, `parseFunctionLiteral` into `LAMBDA_EXPRESSION`), and
    /// `bind` steps over a collapsed marker's children without reading them; the block is parsed again from its text
    /// when the tree expands it. The omitted markers create no error item, so neither `error`'s check of the last
    /// production item nor `prepareLightTree`'s error dedup changes. In `balanceWhiteSpaces` they would sit at their own
    /// lexemes, a mark on the `?`/`!` and a done right after the joined pair, in lexeme order: each keeps its position,
    /// `last_index` never exceeds the enclosing done, and the walk back of that done's right edge stops at the last
    /// non-whitespace token before reaching any of them, so every edge the tree uses is the same. `index_of` is a
    /// marker's own node, which they do not change.
    pub fn skip_balanced_block(&mut self) -> bool {
        if self.top_truncated().is_some() {
            return false;
        }
        self.imp.delegate.skip_balanced_block(TokenKind::LBRACE, TokenKind::RBRACE);
        true
    }

    /// `rawLookup(steps)`.
    pub fn raw_lookup(&self, steps: i32) -> Option<TokenKind> {
        self.imp.delegate.raw_lookup(steps)
    }

    /// `rawTokenTypeStart(steps)`.
    pub fn raw_token_type_start(&self, steps: i32) -> i32 {
        self.imp.delegate.raw_token_type_start(steps)
    }

    /// `rawTokenIndex()`.
    pub fn raw_token_index(&self) -> i32 {
        self.imp.delegate.raw_token_index()
    }

    /// `getCurrentOffset()`.
    pub fn get_current_offset(&mut self) -> i32 {
        self.imp.delegate.get_current_offset()
    }

    /// `getOriginalText()`.
    pub fn get_original_text(&self) -> &[u8] {
        self.imp.delegate.get_original_text()
    }

    /// Not in the Java: moves the cursor over the rest of a construct nested too deeply, from the current token to the
    /// first that closes a bracket opened before it (`PsiBuilderImpl::skip_too_deeply_nested`), or to the stream's end; over
    /// one token at least unless at the end, so that the callers' loops go on. The tokens stay raw: a `?.` inside is
    /// `?` and `.`.
    pub fn skip_too_deeply_nested(&mut self) {
        if self.eof() {
            return;
        }
        // A truncation ends the stream at the first token starting at its end or later (`eof`), after the current one.
        let eof = self.top_trunc.map(|_| self.min_eof.max(0) as u32);
        self.imp.delegate.skip_too_deeply_nested(eof);
    }

    /// `PsiBuilderImpl::enter_nesting`.
    #[inline(always)]
    pub fn enter_nesting(&mut self, kind: Nesting) -> bool {
        self.imp.delegate.enter_nesting(kind)
    }

    /// `PsiBuilderImpl::exit_nesting`.
    #[inline(always)]
    pub fn exit_nesting(&mut self, kind: Nesting) {
        self.imp.delegate.exit_nesting(kind);
    }

    /// `hasErrorsAfter(marker)` (on `PsiBuilderImpl`).
    pub fn has_errors_after(&self, marker: Marker) -> bool {
        self.imp.delegate.has_errors_after(marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::psi_to_string;
    use crate::node_kind::NodeKind::*;
    use crate::tokens::TokenKind::*;
    use crate::tree::{FileKind, parse_with};

    fn dump(text: &str, parse: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilderImpl)) -> String {
        let tree = parse_with(text.as_bytes(), FileKind::Kt, parse);
        psi_to_string(&tree, "a.kt")
    }

    fn file(
        b: &mut SemanticWhitespaceAwarePsiBuilderImpl,
        body: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilder<'_>),
    ) {
        let mut h = b.handle();
        let root = h.mark();
        body(&mut h);
        while !h.eof() {
            h.advance_lexer();
        }
        root.done(&mut h, KT_FILE);
    }

    #[test]
    fn default_binders_leave_whitespace_and_comments_outside() {
        let out = dump("// c\n a  b ", |b| {
            file(b, |h| {
                let m = h.mark();
                h.advance_lexer();
                let empty = h.mark();
                empty.done(h, VALUE_ARGUMENT_LIST);
                h.advance_lexer();
                m.done(h, CALL_EXPRESSION);
            })
        });
        assert_eq!(
            out,
            "KtFile: a.kt\n  PsiComment(EOL_COMMENT)('// c')\n  PsiWhiteSpace('\\n ')\n  CALL_EXPRESSION\n    PsiElement(IDENTIFIER)('a')\n    PsiWhiteSpace('  ')\n    VALUE_ARGUMENT_LIST\n      <empty list>\n    PsiElement(IDENTIFIER)('b')\n  PsiWhiteSpace(' ')"
        );
    }

    #[test]
    fn comment_binders_of_declarations() {
        let out = dump("// doc\nfun f() // trailing\n// next\n\nval x", |b| {
            file(b, |h| {
                let decl = h.mark();
                for _ in 0..4 {
                    h.advance_lexer();
                }
                decl.done(h, FUN);
                decl.set_custom_edge_token_binders(h, Some(Binder::PrecedingComments), Some(Binder::TrailingComments));
                let property = h.mark();
                h.advance_lexer();
                h.advance_lexer();
                property.done(h, PROPERTY);
                property.set_custom_edge_token_binders(
                    h,
                    Some(Binder::PrecedingComments),
                    Some(Binder::TrailingComments),
                );
            })
        });
        let lines: Vec<&str> = out.lines().map(str::trim).collect();
        assert_eq!(
            &lines[1..5],
            ["FUN", "PsiComment(EOL_COMMENT)('// doc')", "PsiWhiteSpace('\\n')", "PsiElement(fun)('fun')"]
        );
        assert!(out.contains("    PsiComment(EOL_COMMENT)('// trailing')\n  PsiWhiteSpace('\\n')\n  PsiComment(EOL_COMMENT)('// next')\n  PsiWhiteSpace('\\n\\n')\n  PROPERTY"), "{out}");
    }

    #[test]
    fn joined_tokens_collapse_into_one_leaf() {
        let out = dump("a?.b?:c!!", |b| {
            file(b, |h| {
                assert_eq!(h.get_token_type(), Some(IDENTIFIER));
                assert_eq!(h.look_ahead(1), Some(SAFE_ACCESS));
                h.advance_lexer();
                assert_eq!(h.get_token_type(), Some(SAFE_ACCESS));
                assert_eq!(h.get_token_text(), Some("?."));
                h.advance_lexer();
                h.advance_lexer();
                assert_eq!(h.get_token_type(), Some(ELVIS));
                h.disable_joining_complex_tokens();
                assert_eq!(h.get_token_type(), Some(QUEST));
                h.restore_joining_complex_tokens_state();
                h.advance_lexer();
                h.advance_lexer();
                assert_eq!(h.get_token_type(), Some(EXCLEXCL));
                h.advance_lexer();
                assert!(h.eof());
            })
        });
        assert!(
            out.contains("PsiElement(SAFE_ACCESS)('?.')")
                && out.contains("PsiElement(ELVIS)('?:')")
                && out.contains("PsiElement(EXCLEXCL)('!!')"),
            "{out}"
        );
    }

    #[test]
    fn newline_before_current_token_skips_comments() {
        dump("a /* c */\n/* d */ b c", |b| {
            file(b, |h| {
                assert!(!h.newline_before_current_token());
                h.advance_lexer();
                assert!(h.newline_before_current_token());
                h.disable_newlines();
                assert!(!h.newline_before_current_token());
                h.restore_newlines_state();
                h.advance_lexer();
                assert!(!h.newline_before_current_token());
            })
        });
    }

    #[test]
    fn rollback_precede_done_before_and_errors() {
        let out = dump("a b c", |b| {
            file(b, |h| {
                let start = h.mark();
                h.advance_lexer();
                let inner = h.mark();
                h.error("dropped");
                h.advance_lexer();
                start.rollback_to(h);
                assert_eq!(h.get_token_type(), Some(IDENTIFIER));
                assert_eq!(h.get_current_offset(), 0);
                let a = h.mark();
                h.advance_lexer();
                a.done(h, REFERENCE_EXPRESSION);
                let outer = a.precede(h);
                let b_marker = h.mark();
                h.advance_lexer();
                b_marker.done(h, REFERENCE_EXPRESSION);
                // the cursor is on the whitespace after `b`: an error item binds before it (DEFAULT_RIGHT_BINDER)
                h.error("first");
                h.error("same lexeme: ignored");
                outer.done_before(h, BINARY_EXPRESSION, b_marker);
                let _ = inner;
            })
        });
        assert_eq!(
            out,
            "KtFile: a.kt\n  BINARY_EXPRESSION\n    REFERENCE_EXPRESSION\n      PsiElement(IDENTIFIER)('a')\n  PsiWhiteSpace(' ')\n  REFERENCE_EXPRESSION\n    PsiElement(IDENTIFIER)('b')\n  PsiErrorElement:first\n    <empty list>\n  PsiWhiteSpace(' ')\n  PsiElement(IDENTIFIER)('c')"
        );
    }

    #[test]
    fn truncated_and_by_clause_adapters() {
        dump("a b c", |b| {
            let mut h = b.handle();
            let root = h.mark();
            {
                let mut t = h.reborrow().wrapped(Adapter::Truncated { eof_position: 4 });
                assert_eq!(t.look_ahead(1), Some(IDENTIFIER));
                assert_eq!(t.look_ahead(2), None);
                t.advance_lexer();
                assert!(!t.eof());
                t.advance_lexer();
                assert!(t.eof());
                assert_eq!(t.get_token_type(), None);
                let mut by = t.wrapped(Adapter::ForByClause { stack_size: 0 });
                by.enable_newlines();
                assert_eq!(by.get_stack_size(), 1);
                by.restore_newlines_state();
                assert_eq!(by.get_stack_size(), 0);
            }
            assert!(!h.eof());
            assert_eq!(h.get_token_type(), Some(IDENTIFIER));
            h.advance_lexer();
            root.done(&mut h, KT_FILE);
        });
    }
}
