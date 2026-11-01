//! The port of IntelliJ's `PsiBuilderImpl` (with `MarkerProduction`, `MarkerPool`, `MarkerOptionalData`), the
//! platform's `WhitespacesBinders` and Kotlin's `KotlinWhitespaceAndCommentsBinders`, and of Kotlin's
//! `SemanticWhitespaceAwarePsiBuilderImpl` with its adapters `TruncatedSemanticWhitespaceAwarePsiBuilder` and
//! `SemanticWhitespaceAwarePsiBuilderForByClause`.
//!
//! A builder holds the tokens of one text (a file, or the text of a lazily parsed element), a cursor that skips
//! whitespace and comments, and the production list of markers. `begin_bind` balances the markers' edges over
//! whitespace and comments (`balanceWhiteSpaces`) and appends the tree to the arena (`prepareLightTree` + `bind`).
//!
//! Where the Java code throws or only logs on a broken marker sequence (a parser bug), the port does nothing or keeps
//! the tree lossless instead; it never panics.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::node_kind::NodeKind;
use crate::tokens::{ElementType, TokenKind, TokenSet};
use crate::tree::TreeData;

/// A marker (`PsiBuilder.Marker`): a handle into its builder's marker pool. Its methods take the builder, as the Java
/// `marker.done(type)` reaches its builder through the marker: `marker.done(builder, FUN)`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Marker(u32);

/// `WhitespacesAndCommentsBinder`: where a marker's edge goes among the whitespace and comments around it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WhitespacesAndCommentsBinder {
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

pub use WhitespacesAndCommentsBinder as Binder;

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

impl WhitespacesAndCommentsBinder {
    /// `isRecursive()`: none of these binders is.
    pub fn is_recursive(self) -> bool {
        false
    }

    /// `getEdgePosition(tokens, atStreamEdge, getter)`: the edge as an index into `tokens`.
    pub fn get_edge_position<'t>(
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
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum MarkerKind {
    Start,
    Error,
}

/// A `ProductionMarker` (`StartMarker` or `ErrorItem`) with its `MarkerOptionalData`. The light tree's links are not
/// stored: `bind` reads the production in order instead (see there).
#[derive(Clone, Debug)]
struct MarkerData {
    kind: MarkerKind,
    /// `myLexemeIndex`; -1 when the marker is free.
    lexeme_index: i32,
    /// `myDoneLexeme`; -1 while not done.
    done_lexeme: i32,
    element_type: Option<ElementType>,
    /// The error message (`MarkerOptionalData.getDoneError` of a start marker, `ErrorItem.myMessage` of an error item)
    /// as its index + 1 in the builder's `messages`; 0 for none. Kept apart so the pool's records stay small.
    message: u32,
    left_binder: Binder,
    right_binder: Binder,
    collapsed: bool,
}

const NONE: u32 = 0;

impl MarkerData {
    fn new(kind: MarkerKind) -> Self {
        MarkerData {
            kind,
            lexeme_index: -1,
            done_lexeme: -1,
            element_type: None,
            message: 0,
            left_binder: DEFAULT_LEFT_BINDER,
            right_binder: DEFAULT_RIGHT_BINDER,
            collapsed: false,
        }
    }

    /// `getLexemeIndex(done)`.
    fn get_lexeme_index(&self, done: bool) -> i32 {
        if done && self.kind == MarkerKind::Start { self.done_lexeme } else { self.lexeme_index }
    }

    /// `setLexemeIndex(lexemeIndex, done)`.
    fn set_lexeme_index(&mut self, lexeme_index: i32, done: bool) {
        if done && self.kind == MarkerKind::Start {
            self.done_lexeme = lexeme_index;
        } else {
            self.lexeme_index = lexeme_index;
        }
    }

    fn is_done(&self) -> bool {
        self.done_lexeme != -1
    }
}

/// What a collapsed or lazy element is reparsed with when the tree expands it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LazyKind {
    /// `BlockExpressionElementType`: `KotlinParser.parseBlockExpression`.
    Block,
    /// `LambdaExpressionElementType`: `KotlinParser.parseLambdaExpression`.
    Lambda,
    /// `KDocTokens.KDOC`: `KDocLexer` + `KDocParser`.
    KDoc,
    /// `KDocTokens.MARKDOWN_LINK`: `KotlinLexer` + `KDocLinkParser`.
    KDocLink,
}

/// A chameleon the tree still has to expand: its node, what parses it, its text range, and the range of its tokens
/// in the file's own token list when the builder's tokens are a slice of it.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Pending {
    pub node: u32,
    pub kind: LazyKind,
    pub start: usize,
    pub end: usize,
    pub file_tokens: Option<(usize, usize)>,
}

/// Where `bind` is: its walk of the production, kept in the builder so it can stop at a chameleon and resume.
#[derive(Copy, Clone, Debug, Default)]
struct BindState {
    root_marker: u32,
    root_node: u32,
    /// The marker and node leaves and children go to; `bind_stack` holds their ancestors below the root.
    cur_marker: u32,
    cur_node: u32,
    /// The next token to insert, and the end of the tokens to insert before `then`.
    lex_index: usize,
    insert_to: usize,
    /// `lastErrorIndex`.
    last_error_index: i32,
    /// The next production entry.
    i: usize,
    then: BindStep,
}

/// What `bind` does once the tokens before it are inserted.
#[derive(Copy, Clone, Debug, Default)]
enum BindStep {
    /// Read the next production entry.
    #[default]
    Next,
    /// A done entry: close the current node.
    Close,
    /// A start entry: open its composite, or collapse it into a leaf.
    Open(u32),
    /// An error item: append its error element.
    Error(u32),
    /// The production is read: close what is still open, innermost first.
    CloseOpen,
    CloseOpenOne,
    /// Append the tokens after the root's done.
    RootTail,
    /// Set the root's end.
    Finish,
    Done,
}

/// The buffers builders work in, kept by a `Parser` from one builder to the next (cleared, not freed): the token
/// types a builder parses (filled by its caller), the marker pool with its free lists, the production, and the
/// semantic layer's state stacks and adapters. A builder takes them in `new` and gives them back in `release`.
#[derive(Default)]
pub(crate) struct BuilderBuffers {
    pub lex_types: Vec<TokenKind>,
    /// The token starts a builder takes in `new` (filled by its caller) and gives back in `release`.
    pub starts: Vec<u32>,
    markers: Vec<MarkerData>,
    free_start_markers: Vec<u32>,
    free_error_items: Vec<u32>,
    production: Vec<i32>,
    production_lexemes: Vec<i32>,
    messages: Vec<Cow<'static, str>>,
    bind_stack: Vec<(u32, u32)>,
    join_complex_tokens: Vec<bool>,
    newlines_enabled: Vec<bool>,
    adapters: Vec<Adapter>,
}

/// `PsiBuilderImpl`.
pub struct PsiBuilderImpl {
    text: Arc<[u8]>,
    /// The absolute offset of this builder's text in `text`, and its length.
    base: usize,
    len: usize,
    /// `myLexStarts`, relative to `base`, `lexeme_count + 1` entries.
    lex_starts: Vec<u32>,
    /// `myLexTypes`.
    lex_types: Vec<TokenKind>,
    lexeme_count: usize,
    /// Where token 0 is in the file's token list, when the tokens are an unmodified slice of it.
    file_token: Option<usize>,
    tokens_to_restore_on_rollback: BTreeMap<usize, TokenKind>,
    current_lexeme: usize,
    token_type_checked: bool,
    /// The current token, cached for `current_types` (not in the Java): `get_token_type()` and its join with the next
    /// raw token as `SemanticWhitespaceAwarePsiBuilderImpl.getJoinedTokenType(raw, 1)` computes it. `tt_valid` is
    /// cleared wherever `current_lexeme` moves or `lex_types` is written: `advance_lexer`, `raw_advance_lexer` (and
    /// `advance_to_eof` through it), `rollback_to` (which also restores remapped tokens), `remap_current_token` and
    /// `skip_balanced_block`.
    /// `skip_whitespace` only steps over whitespace and comments up to the token both values describe, so it keeps
    /// them valid. It lives here, not in the semantic layer, because markers mutate this builder directly.
    tt_raw: Option<TokenKind>,
    tt_joined: Option<TokenKind>,
    tt_valid: bool,
    /// `MarkerPool.list`; id 0 is no marker.
    markers: Vec<MarkerData>,
    free_start_markers: Vec<u32>,
    free_error_items: Vec<u32>,
    /// `MarkerProduction`: a start marker's id, or the negated id of a done marker.
    production: Vec<i32>,
    /// `getLexemeIndexAt(i)` for every production index (not in the Java, which reads it from the marker): the
    /// marker's `getLexemeIndex(done)`, written wherever an entry is added and rewritten by `balance_white_spaces`, so
    /// the searches over the production read one contiguous array.
    production_lexemes: Vec<i32>,
    /// The error messages markers refer to (`MarkerData::message`).
    messages: Vec<Cow<'static, str>>,
    /// `bind`'s open composites below the root: (marker, node) of each one's parent.
    bind_stack: Vec<(u32, u32)>,
    bind: BindState,
}

/// The `PsiBuilder` methods a marker needs: implemented by every builder layer, so `marker.done(builder, kind)` works
/// with any of them.
pub trait AsPsiBuilder {
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl;
}

impl AsPsiBuilder for PsiBuilderImpl {
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl {
        self
    }
}

const LINEAR_SEARCH_LIMIT: usize = 20;

impl PsiBuilderImpl {
    /// A builder over `text[base..base + len]` with the token types in `bufs.lex_types` and their starts in
    /// `bufs.starts` (relative to `base`, one more entry than the types, the last being `len`). It works in `bufs`, emptied until `release`;
    /// the marker pool starts afresh (id 0, no marker), whatever the previous builder left.
    pub(crate) fn new(
        text: Arc<[u8]>,
        base: usize,
        len: usize,
        bufs: &mut BuilderBuffers,
        file_token: Option<usize>,
    ) -> Self {
        let lex_types = std::mem::take(&mut bufs.lex_types);
        let mut lex_starts = std::mem::take(&mut bufs.starts);
        let mut markers = std::mem::take(&mut bufs.markers);
        markers.clear();
        markers.push(MarkerData::new(MarkerKind::Start));
        let mut free_start_markers = std::mem::take(&mut bufs.free_start_markers);
        free_start_markers.clear();
        let mut free_error_items = std::mem::take(&mut bufs.free_error_items);
        free_error_items.clear();
        let mut production = std::mem::take(&mut bufs.production);
        production.clear();
        let mut production_lexemes = std::mem::take(&mut bufs.production_lexemes);
        production_lexemes.clear();
        let mut messages = std::mem::take(&mut bufs.messages);
        messages.clear();
        let bind_stack = std::mem::take(&mut bufs.bind_stack);
        let lexeme_count = lex_types.len();
        lex_starts.resize(lexeme_count + 1, len as u32);
        lex_starts[lexeme_count] = len as u32;
        PsiBuilderImpl {
            text,
            base,
            len,
            lex_starts,
            lex_types,
            lexeme_count,
            file_token,
            tokens_to_restore_on_rollback: BTreeMap::new(),
            current_lexeme: 0,
            token_type_checked: false,
            tt_raw: None,
            tt_joined: None,
            tt_valid: false,
            markers,
            free_start_markers,
            free_error_items,
            production,
            production_lexemes,
            messages,
            bind_stack,
            bind: BindState::default(),
        }
    }

    /// Gives the buffers back to `bufs`, the token starts to `bufs.starts`.
    pub(crate) fn release(self, bufs: &mut BuilderBuffers) {
        bufs.starts = self.lex_starts;
        bufs.lex_types = self.lex_types;
        bufs.markers = self.markers;
        bufs.free_start_markers = self.free_start_markers;
        bufs.free_error_items = self.free_error_items;
        bufs.production = self.production;
        bufs.production_lexemes = self.production_lexemes;
        bufs.messages = self.messages;
        bufs.bind_stack = self.bind_stack;
    }

    /// `getOriginalText()`: this builder's text.
    pub fn get_original_text(&self) -> &[u8] {
        &self.text[self.base..self.base + self.len]
    }

    /// `isWhitespaceOrComment(type)`: `KtTokens.WHITESPACES` or `KtTokens.COMMENTS`.
    #[inline]
    pub fn is_whitespace_or_comment(&self, element_type: TokenKind) -> bool {
        element_type.is_whitespace_or_comment()
    }

    /// `whitespaceOrComment(type)` (what `SemanticWhitespaceAwarePsiBuilderImpl.isWhitespaceOrComment` calls).
    pub fn whitespace_or_comment(&self, token: TokenKind) -> bool {
        self.is_whitespace_or_comment(token)
    }

    /// `getTokenType()`: the current token after skipping whitespace and comments; `None` at the end.
    #[inline(always)]
    pub fn get_token_type(&mut self) -> Option<TokenKind> {
        crate::counters::add(crate::counters::Counter::RawTokenType, 1);
        if self.eof() {
            return None;
        }
        Some(self.lex_types[self.current_lexeme])
    }

    /// `get_token_type()` and, for `SemanticWhitespaceAwarePsiBuilderImpl.getTokenType()` when it joins complex
    /// tokens, `getJoinedTokenType(raw, 1)`, computed once per cursor position (see `tt_valid`).
    #[inline(always)]
    pub(crate) fn current_types(&mut self) -> (Option<TokenKind>, Option<TokenKind>) {
        if !self.tt_valid {
            let raw = self.get_token_type();
            self.tt_joined = self.joined_token_type(raw, 1);
            self.tt_raw = raw;
            self.tt_valid = true;
        }
        (self.tt_raw, self.tt_joined)
    }

    /// `SemanticWhitespaceAwarePsiBuilderImpl.getJoinedTokenType(rawTokenType, rawLookupSteps)`: `?` before `.` is
    /// `SAFE_ACCESS`, before `:` `ELVIS`; `!` before `!` is `EXCLEXCL`.
    fn joined_token_type(&self, raw_token_type: Option<TokenKind>, raw_lookup_steps: i32) -> Option<TokenKind> {
        if raw_token_type == TokenKind::QUEST {
            let next_raw_token = self.raw_lookup(raw_lookup_steps);
            if next_raw_token == TokenKind::DOT {
                return Some(TokenKind::SAFE_ACCESS);
            }
            if next_raw_token == TokenKind::COLON {
                return Some(TokenKind::ELVIS);
            }
        } else if raw_token_type == TokenKind::EXCL {
            let next_raw_token = self.raw_lookup(raw_lookup_steps);
            if next_raw_token == TokenKind::EXCL {
                return Some(TokenKind::EXCLEXCL);
            }
        }
        raw_token_type
    }

    /// `remapCurrentToken(type)`.
    pub fn remap_current_token(&mut self, element_type: TokenKind) {
        self.tt_valid = false;
        if self.current_lexeme < self.lexeme_count {
            self.lex_types[self.current_lexeme] = element_type;
        }
    }

    /// `remapCurrentTokenAndRestoreOnRollback(type)`.
    pub fn remap_current_token_and_restore_on_rollback(&mut self, element_type: TokenKind) {
        if self.current_lexeme < self.lexeme_count {
            self.tokens_to_restore_on_rollback
                .entry(self.current_lexeme)
                .or_insert(self.lex_types[self.current_lexeme]);
        }
        self.remap_current_token(element_type);
    }

    /// `lookAhead(steps)`: the token `steps` non-whitespace tokens ahead of the current one.
    pub fn look_ahead(&self, mut steps: i32) -> Option<TokenKind> {
        crate::counters::add(crate::counters::Counter::LookAhead, 1);
        crate::counters::add(crate::counters::Counter::LookAheadSteps, steps.max(0) as u64);
        let mut cur = self.shift_over_whitespace_forward(self.current_lexeme);
        while steps > 0 {
            cur = self.shift_over_whitespace_forward(cur + 1);
            steps -= 1;
        }
        if cur < self.lexeme_count { Some(self.lex_types[cur]) } else { None }
    }

    fn shift_over_whitespace_forward(&self, mut lex_index: usize) -> usize {
        while lex_index < self.lexeme_count && self.is_whitespace_or_comment(self.lex_types[lex_index]) {
            lex_index += 1;
        }
        lex_index
    }

    /// `rawLookup(steps)`: the raw token `steps` away from the current lexeme (whitespace and comments included).
    pub fn raw_lookup(&self, steps: i32) -> Option<TokenKind> {
        crate::counters::add(crate::counters::Counter::RawLookup, 1);
        let cur = self.current_lexeme as i64 + steps as i64;
        if cur >= 0 && (cur as usize) < self.lexeme_count { Some(self.lex_types[cur as usize]) } else { None }
    }

    /// `rawTokenTypeStart(steps)`: the start offset of that raw token, -1 before the text, the text length after it.
    pub fn raw_token_type_start(&self, steps: i32) -> i32 {
        let cur = self.current_lexeme as i64 + steps as i64;
        if cur < 0 {
            return -1;
        }
        if cur as usize >= self.lexeme_count {
            return self.len as i32;
        }
        self.lex_starts[cur as usize] as i32
    }

    /// `rawTokenIndex()`.
    pub fn raw_token_index(&self) -> i32 {
        self.current_lexeme as i32
    }

    /// `rawAdvanceLexer(steps)`; a negative count does nothing (Java throws).
    pub fn raw_advance_lexer(&mut self, steps: i32) {
        if steps <= 0 {
            return;
        }
        self.current_lexeme = (self.current_lexeme + steps as usize).min(self.lexeme_count);
        self.token_type_checked = false;
        self.tt_valid = false;
    }

    /// `advanceToEOF()`.
    pub fn advance_to_eof(&mut self) {
        let steps = self.lexeme_count - self.current_lexeme;
        self.raw_advance_lexer(steps as i32);
    }

    /// `advanceLexer()`.
    pub fn advance_lexer(&mut self) {
        crate::counters::add(crate::counters::Counter::Advance, 1);
        if self.eof() {
            return;
        }
        self.token_type_checked = false;
        self.tt_valid = false;
        self.current_lexeme += 1;
    }

    /// `KotlinParsing.advanceBalancedBlock()` over the raw tokens, for `SemanticWhitespaceAwarePsiBuilder::
    /// skip_balanced_block` (not in the Java): counts `{` and `}` from the current lexeme (the block's `{` consumed) and
    /// puts the cursor after the `}` that closes the block, or at the end. Whitespace and comments are never braces,
    /// and a joined `?.`/`?:`/`!!` is two lexemes neither of which is one, so the braces counted and the lexeme the
    /// cursor stops at are the Java loop's.
    fn skip_balanced_block(&mut self) {
        let start = self.current_lexeme;
        let mut brace_count = 1i32;
        let mut i = start;
        while i < self.lexeme_count {
            let t = self.lex_types[i];
            i += 1;
            if t == TokenKind::LBRACE {
                brace_count += 1;
            } else if t == TokenKind::RBRACE {
                brace_count -= 1;
                if brace_count == 0 {
                    break;
                }
            }
        }
        if cfg!(feature = "counters") {
            let stepped = self.lex_types[start..i].iter().filter(|t| !t.is_whitespace_or_comment()).count();
            crate::counters::add(crate::counters::Counter::BalancedSkipTokens, stepped as u64);
        }
        self.current_lexeme = i;
        self.token_type_checked = false;
        self.tt_valid = false;
    }

    fn skip_whitespace(&mut self) {
        while self.current_lexeme < self.lexeme_count
            && self.is_whitespace_or_comment(self.lex_types[self.current_lexeme])
        {
            self.current_lexeme += 1;
        }
    }

    /// `getCurrentOffset()`: the start of the current token, the text length at the end.
    pub fn get_current_offset(&mut self) -> i32 {
        if self.eof() {
            return self.len as i32;
        }
        self.lex_starts[self.current_lexeme] as i32
    }

    /// `getTokenText()`: the current token's text, `None` at the end.
    pub fn get_token_text(&mut self) -> Option<&str> {
        let bytes = self.get_token_bytes()?;
        Some(std::str::from_utf8(bytes).unwrap_or("\u{FFFD}"))
    }

    /// `getTokenText()` as bytes.
    pub fn get_token_bytes(&mut self) -> Option<&[u8]> {
        crate::counters::add(crate::counters::Counter::TokenText, 1);
        if self.eof() {
            return None;
        }
        let start = self.base + self.lex_starts[self.current_lexeme] as usize;
        let end = self.base + self.lex_starts[self.current_lexeme + 1] as usize;
        Some(&self.text[start..end])
    }

    /// `mark()`.
    pub fn mark(&mut self) -> Marker {
        crate::counters::add(crate::counters::Counter::Mark, 1);
        if !self.production.is_empty() {
            self.skip_whitespace();
        }
        let marker = self.create_marker(self.current_lexeme as i32);
        self.production.push(marker.0 as i32);
        self.production_lexemes.push(self.current_lexeme as i32);
        marker
    }

    fn create_marker(&mut self, lexeme_index: i32) -> Marker {
        let id = self.allocate(MarkerKind::Start);
        self.markers[id as usize].lexeme_index = lexeme_index;
        Marker(id)
    }

    fn allocate(&mut self, kind: MarkerKind) -> u32 {
        let free = match kind {
            MarkerKind::Start => self.free_start_markers.pop(),
            MarkerKind::Error => self.free_error_items.pop(),
        };
        match free {
            Some(id) => id,
            None => {
                self.markers.push(MarkerData::new(kind));
                (self.markers.len() - 1) as u32
            }
        }
    }

    fn free_marker(&mut self, id: u32) {
        let kind = self.markers[id as usize].kind;
        self.markers[id as usize] = MarkerData::new(kind);
        match kind {
            MarkerKind::Start => self.free_start_markers.push(id),
            MarkerKind::Error => self.free_error_items.push(id),
        }
    }

    fn marker(&self, id: u32) -> &MarkerData {
        &self.markers[id as usize]
    }

    /// Stores `message` as the marker's error message.
    fn set_message(&mut self, id: u32, message: Cow<'static, str>) {
        self.messages.push(message);
        self.markers[id as usize].message = self.messages.len() as u32;
    }

    /// The marker's error message, moved out (the tree is built once).
    fn take_message(&mut self, id: u32) -> Cow<'static, str> {
        match self.markers[id as usize].message {
            0 => Cow::Borrowed(""),
            i => std::mem::take(&mut self.messages[i as usize - 1]),
        }
    }

    /// `eof()`: skips whitespace and comments the first time after the cursor moved.
    #[inline(always)]
    pub fn eof(&mut self) -> bool {
        if !self.token_type_checked {
            self.token_type_checked = true;
            self.skip_whitespace();
        }
        self.current_lexeme >= self.lexeme_count
    }

    /// `error(messageText)`: an error element at the current token, unless the last production is an error item there.
    pub fn error(&mut self, message_text: impl Into<Cow<'static, str>>) {
        if let Some(&last) = self.production.last()
            && last > 0
        {
            let last_marker = self.marker(last as u32);
            if last_marker.kind == MarkerKind::Error && last_marker.lexeme_index == self.current_lexeme as i32 {
                return;
            }
        }
        let id = self.allocate(MarkerKind::Error);
        self.set_message(id, message_text.into());
        self.markers[id as usize].lexeme_index = self.current_lexeme as i32;
        self.production.push(id as i32);
        self.production_lexemes.push(self.current_lexeme as i32);
    }

    /// `hasErrorsAfter(marker)`: whether an error item or error marker was added after `marker` and not dropped.
    pub fn has_errors_after(&self, marker: Marker) -> bool {
        let Some(idx) = self.index_of(marker.0) else { return false };
        self.production[idx + 1..].iter().any(|&id| {
            id > 0 && {
                let m = self.marker(id as u32);
                m.kind == MarkerKind::Error
                    || (m.element_type == Some(ElementType::Node(NodeKind::ERROR_ELEMENT)) && m.message != 0)
            }
        })
    }

    // MarkerProduction

    /// `MarkerProduction.indexOf`: the production index of a start marker.
    fn index_of(&self, marker_id: u32) -> Option<usize> {
        let id = marker_id as i32;
        let size = self.production.len();
        let low = size.saturating_sub(LINEAR_SEARCH_LIMIT);
        for i in (low..size).rev() {
            if self.production[i] == id {
                return Some(i);
            }
        }
        crate::counters::add(crate::counters::Counter::IndexOfSearch, 1);
        let lexeme_index = self.marker(marker_id).get_lexeme_index(false);
        let from = self.find_marker_at_lexeme(lexeme_index).unwrap_or(0);
        (from..size).find(|&i| self.production[i] == id)
    }

    fn find_marker_at_lexeme(&self, lexeme_index: i32) -> Option<usize> {
        let end = self.production.len().saturating_sub(LINEAR_SEARCH_LIMIT);
        let (mut lo, mut hi) = (0usize, end);
        let mut found = None;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.get_lexeme_index_at(mid).cmp(&lexeme_index) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    found = Some(mid);
                    break;
                }
            }
        }
        found.map(|i| self.find_same_lexeme_group_start(lexeme_index, i))
    }

    fn find_same_lexeme_group_start(&self, lexeme_index: i32, mut prod_index: usize) -> usize {
        while prod_index > 0 && self.get_lexeme_index_at(prod_index - 1) == lexeme_index {
            prod_index -= 1;
        }
        prod_index
    }

    fn get_lexeme_index_at(&self, production_index: usize) -> i32 {
        let lexeme = self.production_lexemes[production_index];
        if cfg!(debug_assertions) {
            let id = self.production[production_index];
            debug_assert_eq!(lexeme, self.marker(id.unsigned_abs()).get_lexeme_index(id < 0));
        }
        lexeme
    }

    fn production_rollback_to(&mut self, marker_id: u32) {
        let Some(idx) = self.index_of(marker_id) else { return };
        for i in (idx..self.production.len()).rev() {
            let id = self.production[i];
            if id > 0 {
                self.free_marker(id as u32);
            }
        }
        self.production.truncate(idx);
        self.production_lexemes.truncate(idx);
    }

    fn drop_marker(&mut self, marker_id: u32) {
        if self.marker(marker_id).is_done()
            && let Some(i) = self.production.iter().rposition(|&id| id == -(marker_id as i32))
        {
            self.production.remove(i);
            self.production_lexemes.remove(i);
        }
        if let Some(i) = self.index_of(marker_id) {
            self.production.remove(i);
            self.production_lexemes.remove(i);
        }
        self.free_marker(marker_id);
    }

    fn add_done(&mut self, marker_id: u32, anchor_before: Option<u32>) {
        let index = match anchor_before {
            None => Some(self.production.len()),
            Some(anchor) => self.index_of(anchor),
        };
        if let Some(index) = index {
            self.production.insert(index, -(marker_id as i32));
            self.production_lexemes.insert(index, self.marker(marker_id).done_lexeme);
        }
    }

    fn add_before(&mut self, marker_id: u32, anchor: u32) {
        if let Some(index) = self.index_of(anchor) {
            self.production.insert(index, marker_id as i32);
            self.production_lexemes.insert(index, self.marker(marker_id).lexeme_index);
        }
    }

    // StartMarker

    /// `Marker.precede()`.
    pub fn precede(&mut self, marker: Marker) -> Marker {
        crate::counters::add(crate::counters::Counter::Precede, 1);
        let lexeme_index = self.marker(marker.0).lexeme_index.max(0);
        let pre = self.create_marker(lexeme_index);
        self.add_before(pre.0, marker.0);
        pre
    }

    /// `Marker.drop()`.
    pub fn drop(&mut self, marker: Marker) {
        crate::counters::add(crate::counters::Counter::Drop, 1);
        self.drop_marker(marker.0);
    }

    /// `Marker.rollbackTo()`.
    pub fn rollback_to(&mut self, marker: Marker) {
        let lexeme_index = self.marker(marker.0).lexeme_index;
        if lexeme_index < 0 {
            return;
        }
        crate::counters::add(crate::counters::Counter::Rollback, 1);
        crate::counters::add(
            crate::counters::Counter::RollbackLexemes,
            self.current_lexeme.saturating_sub(lexeme_index as usize) as u64,
        );
        if !self.tokens_to_restore_on_rollback.is_empty() {
            let restored = self.tokens_to_restore_on_rollback.split_off(&(lexeme_index as usize));
            for (index, element_type) in restored {
                self.lex_types[index] = element_type;
            }
        }
        self.current_lexeme = lexeme_index as usize;
        self.token_type_checked = true;
        self.tt_valid = false;
        self.production_rollback_to(marker.0);
    }

    /// `Marker.done(type)`.
    pub fn done(&mut self, marker: Marker, element_type: impl Into<ElementType>) {
        self.markers[marker.0 as usize].element_type = Some(element_type.into());
        self.process_done(marker.0, None, None);
    }

    /// `Marker.collapse(type)`: done, and the tree gets one leaf of `type` over the marker's tokens.
    pub fn collapse(&mut self, marker: Marker, element_type: impl Into<ElementType>) {
        self.done(marker, element_type);
        self.markers[marker.0 as usize].collapsed = true;
    }

    /// `Marker.doneBefore(type, before)`.
    pub fn done_before(&mut self, marker: Marker, element_type: impl Into<ElementType>, before: Marker) {
        self.markers[marker.0 as usize].element_type = Some(element_type.into());
        self.process_done(marker.0, None, Some(before.0));
    }

    /// `Marker.doneBefore(type, before, errorMessage)`: also an error element right before `before`.
    pub fn done_before_with_error(
        &mut self,
        marker: Marker,
        element_type: impl Into<ElementType>,
        before: Marker,
        error_message: impl Into<Cow<'static, str>>,
    ) {
        let id = self.allocate(MarkerKind::Error);
        self.set_message(id, error_message.into());
        self.markers[id as usize].lexeme_index = self.marker(before.0).lexeme_index;
        self.add_before(id, before.0);
        self.done_before(marker, element_type, before);
    }

    /// `Marker.error(message)`: done as an error element.
    pub fn error_marker(&mut self, marker: Marker, message: impl Into<Cow<'static, str>>) {
        self.markers[marker.0 as usize].element_type = Some(ElementType::Node(NodeKind::ERROR_ELEMENT));
        self.process_done(marker.0, Some(message.into()), None);
    }

    /// `Marker.errorBefore(message, before)`.
    pub fn error_before(&mut self, marker: Marker, message: impl Into<Cow<'static, str>>, before: Marker) {
        self.markers[marker.0 as usize].element_type = Some(ElementType::Node(NodeKind::ERROR_ELEMENT));
        self.process_done(marker.0, Some(message.into()), Some(before.0));
    }

    /// `Marker.setCustomEdgeTokenBinders(left, right)`; `None` keeps the current binder.
    pub fn set_custom_edge_token_binders(&mut self, marker: Marker, left: Option<Binder>, right: Option<Binder>) {
        let m = &mut self.markers[marker.0 as usize];
        if let Some(left) = left {
            m.left_binder = left;
        }
        if let Some(right) = right {
            m.right_binder = right;
        }
    }

    fn process_done(&mut self, marker_id: u32, error_message: Option<Cow<'static, str>>, before: Option<u32>) {
        crate::counters::add(crate::counters::Counter::Done, 1);
        // doValidityChecks: "Marker already done." is only logged in Java.
        if let Some(message) = error_message {
            self.set_message(marker_id, message);
        }
        let done_lexeme = match before {
            None => self.current_lexeme as i32,
            Some(before) => self.marker(before).lexeme_index,
        };
        let m = self.marker(marker_id);
        let left_bound = matches!(m.element_type, Some(ElementType::Node(n)) if n.is_left_bound());
        if left_bound && self.is_empty(m.lexeme_index, done_lexeme) {
            self.set_custom_edge_token_binders(Marker(marker_id), Some(DEFAULT_RIGHT_BINDER), None);
        }
        if self.marker(marker_id).is_done() {
            // Done twice (Java only logs it): the earlier done entry reads the new done lexeme, as Java's does.
            for i in 0..self.production.len() {
                if self.production[i] == -(marker_id as i32) {
                    self.production_lexemes[i] = done_lexeme;
                }
            }
        }
        self.markers[marker_id as usize].done_lexeme = done_lexeme;
        self.add_done(marker_id, before);
    }

    fn is_empty(&self, start_idx: i32, end_idx: i32) -> bool {
        let start = start_idx.max(0) as usize;
        let end = (end_idx.max(0) as usize).min(self.lexeme_count);
        (start..end).all(|i| self.is_whitespace_or_comment(self.lex_types[i]))
    }

    // Building the tree

    /// `getTreeBuilt()`, first part: balances the edges and starts appending this builder's tree to `tree`, its root
    /// as the node `root` when given (the chameleon being expanded) or as a new node; returns the root. Then
    /// `resume_bind` appends the rest.
    pub(crate) fn begin_bind(&mut self, tree: &mut TreeData, root: Option<u32>) -> u32 {
        let root_marker = self.prepare_light_tree();
        let base = self.base as u32;
        let root_type = if root_marker == NONE {
            ElementType::Node(NodeKind::KT_FILE)
        } else {
            self.marker(root_marker).element_type.unwrap_or(ElementType::Node(NodeKind::KT_FILE))
        };
        let root_node = match root {
            Some(node) => node,
            None => tree.add_composite(u32::MAX, root_type, base),
        };
        self.bind_stack.clear();
        // At most a leaf per token and a node per production entry (a start or an error).
        tree.reserve(self.lexeme_count + self.production.len());
        self.bind = if root_marker == NONE {
            // "Parser produced no markers": keep the tree lossless.
            BindState {
                root_marker,
                root_node,
                cur_marker: root_marker,
                cur_node: root_node,
                lex_index: 0,
                last_error_index: -1,
                i: 0,
                insert_to: self.lexeme_count,
                then: BindStep::Finish,
            }
        } else {
            let lex_index = self.marker(root_marker).lexeme_index.max(0) as usize;
            BindState {
                root_marker,
                root_node,
                cur_marker: root_marker,
                cur_node: root_node,
                lex_index,
                last_error_index: -1,
                i: 1,
                insert_to: lex_index,
                then: BindStep::Next,
            }
        };
        root_node
    }

    /// `getTreeBuilt()`, the rest: appends nodes in document order until a chameleon (a doc comment, a markdown link,
    /// a collapsed lazy block or lambda) has been appended, and returns it; the caller appends the chameleon's own
    /// subtree right after it (so node ids are in preorder) and calls this again. `None` when the tree is complete.
    pub(crate) fn resume_bind(&mut self, tree: &mut TreeData) -> Option<Pending> {
        self.resume_bind_steps(tree)
    }

    /// `prepareLightTree()`: balances the edges; returns the root marker (0 if there are no markers). The Java then
    /// links the markers into a tree, which `bind` instead reads straight from the production.
    fn prepare_light_tree(&mut self) -> u32 {
        let Some(&first) = self.production.first() else { return NONE };
        if first <= 0 {
            return NONE;
        }
        let root_marker = first as u32;
        self.token_type_checked = true;
        self.balance_white_spaces();
        root_marker
    }

    /// `balanceWhiteSpaces()`: moves each marker edge over the whitespace and comments around it as its binder says.
    fn balance_white_spaces(&mut self) {
        let mut last_index: i32 = 0;
        let size = self.production.len();
        if size < 2 {
            return;
        }
        for i in 1..size - 1 {
            let id = self.production[i];
            let done = id < 0;
            let item_id = id.unsigned_abs();
            let item = self.marker(item_id);
            let binder = if item.kind == MarkerKind::Error {
                DEFAULT_RIGHT_BINDER
            } else if done {
                item.right_binder
            } else {
                item.left_binder
            };
            let mut lexeme_index = self.get_lexeme_index_at(i);

            let recursive = binder.is_recursive();
            let prev_production_lex_index = if recursive { 0 } else { self.get_lexeme_index_at(i - 1) };
            let mut ws_start_index = lexeme_index.max(last_index);
            while ws_start_index > prev_production_lex_index
                && (ws_start_index as usize) <= self.lexeme_count
                && self.is_whitespace_or_comment(self.lex_types[ws_start_index as usize - 1])
            {
                ws_start_index -= 1;
            }
            let ws_end_index = self.shift_over_whitespace_forward(lexeme_index.max(0) as usize) as i32;

            if ws_start_index < ws_end_index {
                let start = ws_start_index as usize;
                let end = ws_end_index as usize;
                let at_end = ws_start_index == 0 || end == self.lexeme_count;
                let tokens = &self.lex_types[start..end];
                let text = &self.text[self.base..self.base + self.len];
                let starts = &self.lex_starts;
                let getter = |k: usize| -> &[u8] { &text[starts[start + k] as usize..starts[start + k + 1] as usize] };
                let edge = binder.get_edge_position(tokens, at_end, &getter).min(end - start);
                lexeme_index = ws_start_index + edge as i32;
                self.markers[item_id as usize].set_lexeme_index(lexeme_index, done);
                self.production_lexemes[i] = lexeme_index;
            } else if ws_start_index > ws_end_index {
                // An earlier edge moved past this marker's whitespace: Java configures the token view with a negative
                // size and the binder still answers.
                lexeme_index = ws_start_index + self.negative_size_edge(binder, ws_start_index, ws_end_index);
                self.markers[item_id as usize].set_lexeme_index(lexeme_index, done);
                self.production_lexemes[i] = lexeme_index;
            } else if lexeme_index < ws_start_index {
                lexeme_index = ws_start_index;
                self.markers[item_id as usize].set_lexeme_index(ws_start_index, done);
                self.production_lexemes[i] = ws_start_index;
            }

            last_index = lexeme_index;
        }
    }

    /// What `binder.getEdgePosition` returns on `RelativeTokenTypesView.configure(start, end)` with `start > end`:
    /// `size()` is negative, so `isEmpty()` is false, `indices` is empty, and `get(i)` reads `myLexTypes[start + i]`
    /// unchecked.
    fn negative_size_edge(&self, binder: Binder, ws_start: i32, ws_end: i32) -> i32 {
        let size = ws_end - ws_start;
        let get = |index: i32| -> Option<TokenKind> {
            let i = ws_start + index;
            if i >= 0 && (i as usize) < self.lexeme_count { Some(self.lex_types[i as usize]) } else { None }
        };
        match binder {
            Binder::DefaultLeft | Binder::PrecedingComments | Binder::PrecedingDocComments | Binder::TrailingAll => {
                size
            }
            Binder::DefaultRight | Binder::TrailingComments | Binder::DoNotBindAnything | Binder::PrecedingAll => 0,
            Binder::PrecedingAllComments => (get(0) == TokenKind::WHITE_SPACE) as i32,
            Binder::TrailingAllComments => size - (get(size - 1) == TokenKind::WHITE_SPACE) as i32,
            Binder::BindFirstShebangWithWhitespaceOnly => (get(0) == TokenKind::SHEBANG_COMMENT) as i32,
        }
    }

    /// `bind()` with `createRootAST`, `insertLeaves`, `collapseLeaves`: appends the leaves and composites to `tree`,
    /// resumable at each chameleon (see `resume_bind`).
    ///
    /// Java walks the light tree `prepareLightTree` links; this walks the production it is linked from, in the same
    /// order, with the same rules: after the root, a start is a child of the current marker and becomes current
    /// (a collapsed one gets its leaf, and its entries up to the done that closes it are skipped), any done closes the
    /// current marker (at its own done lexeme, not the done entry's) and makes its parent current, so entries after the
    /// root's done are the root's children, and an error item is a child unless it is at the same lexeme as the last
    /// error item before it anywhere in the production (`lastErrorIndex`, collapsed markers' items included). Markers
    /// still open at the end are closed innermost first, then the root.
    ///
    /// Every leaf goes to the current node, so the walk is a loop of two steps: insert the tokens up to `insert_to`
    /// as leaves of the current node, then do `then`. A leaf that is a chameleon stops it there (`resume_bind`).
    fn resume_bind_steps(&mut self, tree: &mut TreeData) -> Option<Pending> {
        // The state lives in a local while the walk runs (kept in registers), and goes back on a stop.
        let mut st = self.bind;
        let chameleon = self.bind_steps(&mut st, tree);
        self.bind = st;
        chameleon
    }

    #[inline(always)]
    fn bind_steps(&mut self, st: &mut BindState, tree: &mut TreeData) -> Option<Pending> {
        loop {
            while st.lex_index < st.insert_to {
                let to = st.insert_to;
                let stop = self.insert_plain_leaves(st.lex_index, to, st.cur_node, tree);
                if stop == to {
                    st.lex_index = to;
                    break;
                }
                st.lex_index = stop + 1;
                let leaf = ElementType::Token(self.lex_types[stop]);
                if let Some(chameleon) = self.create_leaf(leaf, stop, stop + 1, st.cur_node, tree) {
                    return Some(chameleon);
                }
            }
            match st.then {
                BindStep::Next => {
                    let Some(&id) = self.production.get(st.i) else {
                        st.then = BindStep::CloseOpen;
                        continue;
                    };
                    st.i += 1;
                    if id < 0 {
                        // done marker; an unbalanced one is only reported in Java
                        if !self.bind_stack.is_empty() {
                            st.insert_to = self.leaf_end(self.marker(st.cur_marker).done_lexeme);
                            st.then = BindStep::Close;
                        }
                        continue;
                    }
                    let id = id as u32;
                    let m = self.marker(id);
                    let (kind, lexeme_index) = (m.kind, m.lexeme_index);
                    if kind == MarkerKind::Start {
                        st.insert_to = self.leaf_end(lexeme_index);
                        st.then = BindStep::Open(id);
                    } else if lexeme_index != st.last_error_index {
                        // adding only the first (deepest) error from the same lexeme offset
                        st.last_error_index = lexeme_index;
                        st.insert_to = self.leaf_end(lexeme_index);
                        st.then = BindStep::Error(id);
                    }
                }
                BindStep::Close => {
                    tree.set_end(st.cur_node, self.offset_of(st.lex_index));
                    if let Some((parent_marker, parent_node)) = self.bind_stack.pop() {
                        st.cur_marker = parent_marker;
                        st.cur_node = parent_node;
                    }
                    st.then = BindStep::Next;
                }
                BindStep::Open(id) => {
                    st.then = BindStep::Next;
                    if !self.marker(id).collapsed {
                        self.bind_stack.push((st.cur_marker, st.cur_node));
                        st.cur_node = self.create_composite(id, st.cur_node, st.lex_index, tree);
                        st.cur_marker = id;
                        continue;
                    }
                    // collapseLeaves; the collapsed marker's own entries are skipped
                    let size = self.production.len();
                    let mut depth = 1;
                    while st.i < size && depth > 0 {
                        let inner = self.production[st.i];
                        st.i += 1;
                        if inner < 0 {
                            depth -= 1;
                        } else if self.marker(inner as u32).kind == MarkerKind::Start {
                            depth += 1;
                        } else {
                            st.last_error_index = self.marker(inner as u32).lexeme_index;
                        }
                    }
                    let m = self.marker(id);
                    let (start, element_type) = (m.lexeme_index.max(0) as usize, m.element_type);
                    let end = (m.done_lexeme.max(0) as usize).clamp(start, self.lexeme_count);
                    st.lex_index = end;
                    st.insert_to = end;
                    if let Some(element_type) = element_type
                        && let Some(chameleon) = self.create_leaf(element_type, start, end, st.cur_node, tree)
                    {
                        return Some(chameleon);
                    }
                }
                BindStep::Error(id) => {
                    st.then = BindStep::Next;
                    let start = self.offset_of(st.lex_index);
                    let error = tree.add_composite(st.cur_node, ElementType::Node(NodeKind::ERROR_ELEMENT), start);
                    tree.set_end(error, start);
                    let message = self.take_message(id);
                    tree.set_error(error, message);
                }
                BindStep::CloseOpen => {
                    if self.bind_stack.is_empty() {
                        st.insert_to = self.leaf_end(self.marker(st.root_marker).done_lexeme);
                        st.then = BindStep::RootTail;
                    } else {
                        st.insert_to = self.leaf_end(self.marker(st.cur_marker).done_lexeme);
                        st.then = BindStep::CloseOpenOne;
                    }
                }
                BindStep::CloseOpenOne => {
                    tree.set_end(st.cur_node, self.offset_of(st.lex_index));
                    if let Some((parent_marker, parent_node)) = self.bind_stack.pop() {
                        st.cur_marker = parent_marker;
                        st.cur_node = parent_node;
                    }
                    st.then = BindStep::CloseOpen;
                }
                BindStep::RootTail => {
                    // Tokens after the root's end are not inserted by PsiBuilderImpl (it logs "Tokens ... were not
                    // inserted into the tree"); appending them keeps the tree lossless.
                    st.cur_node = st.root_node;
                    st.insert_to = self.lexeme_count;
                    st.then = BindStep::Finish;
                }
                BindStep::Finish => {
                    tree.set_end(st.root_node, (self.base + self.len) as u32);
                    st.then = BindStep::Done;
                    return None;
                }
                BindStep::Done => return None,
            }
        }
    }

    /// `insertLeaves(curToken, lastIdx, curNode)` up to the first token that is a chameleon (a doc comment or a
    /// markdown link), whose index it returns (`to` if none): the tokens `from..to` as leaves of `cur_node`, but empty
    /// ones (DANGLING_NEWLINE), which are not inserted.
    #[inline(always)]
    fn insert_plain_leaves(&self, from: usize, to: usize, cur_node: u32, tree: &mut TreeData) -> usize {
        let base = self.base as u32;
        let types = &self.lex_types[from..to];
        let starts = &self.lex_starts[from..=to];
        for (i, (&token, edges)) in types.iter().zip(starts.windows(2)).enumerate() {
            if edges[0] < edges[1] {
                if matches!(token, TokenKind::DOC_COMMENT | TokenKind::KDOC_MARKDOWN_LINK) {
                    return from + i;
                }
                tree.add_token_leaf(cur_node, token, base + edges[0], base + edges[1]);
            }
        }
        to
    }

    /// `insertLeaves(curToken, lastIdx, curNode)` as a step of `bind`: where the tokens to insert next into the
    /// current node end.
    fn leaf_end(&self, last_idx: i32) -> usize {
        (last_idx.max(0) as usize).min(self.lexeme_count)
    }

    fn offset_of(&self, lex_index: usize) -> u32 {
        (self.base + self.lex_starts[lex_index.min(self.lexeme_count)] as usize) as u32
    }

    fn create_composite(&mut self, marker: u32, parent: u32, lex_index: usize, tree: &mut TreeData) -> u32 {
        let element_type = self.marker(marker).element_type.unwrap_or(ElementType::Node(NodeKind::ERROR_ELEMENT));
        let node = tree.add_composite(parent, element_type, self.offset_of(lex_index));
        if element_type == ElementType::Node(NodeKind::ERROR_ELEMENT) {
            let message = self.take_message(marker);
            tree.set_error(node, message);
        }
        node
    }

    /// `createLeaf(type, start, end)` over the tokens `first..last`: a leaf, or a chameleon composite (a doc comment, a
    /// markdown link, a collapsed lazy block or lambda), returned for the caller to expand before the walk goes on.
    fn create_leaf(
        &self,
        element_type: ElementType,
        first: usize,
        last: usize,
        cur_node: u32,
        tree: &mut TreeData,
    ) -> Option<Pending> {
        let start = self.base + self.lex_starts[first] as usize;
        let end = self.base + self.lex_starts[last] as usize;
        let lazy = match element_type {
            ElementType::Token(TokenKind::DOC_COMMENT) => Some(LazyKind::KDoc),
            ElementType::Token(TokenKind::KDOC_MARKDOWN_LINK) => Some(LazyKind::KDocLink),
            ElementType::Node(NodeKind::BLOCK) => Some(LazyKind::Block),
            ElementType::Node(NodeKind::LAMBDA_EXPRESSION) => Some(LazyKind::Lambda),
            _ => None,
        };
        match lazy {
            Some(kind) if start < end => {
                let node = tree.add_composite(cur_node, element_type, start as u32);
                tree.set_end(node, end as u32);
                let file_tokens = match kind {
                    LazyKind::Block | LazyKind::Lambda => self.file_token.map(|f| (f + first, f + last)),
                    LazyKind::KDoc | LazyKind::KDocLink => None,
                };
                Some(Pending { node, kind, start, end, file_tokens })
            }
            _ => {
                tree.add_leaf(cur_node, element_type, start as u32, end as u32);
                None
            }
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
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl {
        &mut self.delegate
    }
}

impl SemanticWhitespaceAwarePsiBuilderImpl {
    /// The semantic layer over `delegate`, its stacks and adapters in `bufs` (emptied until `release`).
    pub(crate) fn new(delegate: PsiBuilderImpl, bufs: &mut BuilderBuffers) -> Self {
        let mut join_complex_tokens = std::mem::take(&mut bufs.join_complex_tokens);
        join_complex_tokens.clear();
        join_complex_tokens.push(true);
        let mut newlines_enabled = std::mem::take(&mut bufs.newlines_enabled);
        newlines_enabled.clear();
        newlines_enabled.push(true);
        let mut adapters = std::mem::take(&mut bufs.adapters);
        adapters.clear();
        SemanticWhitespaceAwarePsiBuilderImpl { delegate, join_complex_tokens, newlines_enabled, adapters }
    }

    /// Gives this layer's buffers back to `bufs` and returns the delegate, which builds the tree.
    pub(crate) fn into_delegate(self, bufs: &mut BuilderBuffers) -> PsiBuilderImpl {
        bufs.join_complex_tokens = self.join_complex_tokens;
        bufs.newlines_enabled = self.newlines_enabled;
        bufs.adapters = self.adapters;
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
        self.delegate.joined_token_type(raw_token_type, raw_lookup_steps)
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
    /// non-whitespace token before reaching any of them, so every edge the tree uses is the same. Production stays
    /// sorted by lexeme, so `index_of` and `find_marker_at_lexeme` find the same markers.
    pub fn skip_balanced_block(&mut self) -> bool {
        if self.top_truncated().is_some() {
            return false;
        }
        self.imp.delegate.skip_balanced_block();
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

    /// `hasErrorsAfter(marker)` (on `PsiBuilderImpl`).
    pub fn has_errors_after(&self, marker: Marker) -> bool {
        self.imp.delegate.has_errors_after(marker)
    }
}

impl Marker {
    /// `precede()`.
    pub fn precede(self, b: &mut (impl AsPsiBuilder + ?Sized)) -> Marker {
        b.psi_builder().precede(self)
    }

    /// `drop()`.
    pub fn drop(self, b: &mut (impl AsPsiBuilder + ?Sized)) {
        b.psi_builder().drop(self);
    }

    /// `rollbackTo()`.
    pub fn rollback_to(self, b: &mut (impl AsPsiBuilder + ?Sized)) {
        b.psi_builder().rollback_to(self);
    }

    /// `done(type)`.
    pub fn done(self, b: &mut (impl AsPsiBuilder + ?Sized), element_type: impl Into<ElementType>) {
        b.psi_builder().done(self, element_type);
    }

    /// `collapse(type)`.
    pub fn collapse(self, b: &mut (impl AsPsiBuilder + ?Sized), element_type: impl Into<ElementType>) {
        b.psi_builder().collapse(self, element_type);
    }

    /// `doneBefore(type, before)`.
    pub fn done_before(
        self,
        b: &mut (impl AsPsiBuilder + ?Sized),
        element_type: impl Into<ElementType>,
        before: Marker,
    ) {
        b.psi_builder().done_before(self, element_type, before);
    }

    /// `doneBefore(type, before, errorMessage)`.
    pub fn done_before_with_error(
        self,
        b: &mut (impl AsPsiBuilder + ?Sized),
        element_type: impl Into<ElementType>,
        before: Marker,
        error_message: impl Into<Cow<'static, str>>,
    ) {
        b.psi_builder().done_before_with_error(self, element_type, before, error_message);
    }

    /// `error(message)`.
    pub fn error(self, b: &mut (impl AsPsiBuilder + ?Sized), message: impl Into<Cow<'static, str>>) {
        b.psi_builder().error_marker(self, message);
    }

    /// `errorBefore(message, before)`.
    pub fn error_before(
        self,
        b: &mut (impl AsPsiBuilder + ?Sized),
        message: impl Into<Cow<'static, str>>,
        before: Marker,
    ) {
        b.psi_builder().error_before(self, message, before);
    }

    /// `setCustomEdgeTokenBinders(left, right)` (`null` = `None`).
    pub fn set_custom_edge_token_binders(
        self,
        b: &mut (impl AsPsiBuilder + ?Sized),
        left: Option<Binder>,
        right: Option<Binder>,
    ) {
        b.psi_builder().set_custom_edge_token_binders(self, left, right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::psi_to_string;
    use crate::node_kind::NodeKind::*;
    use crate::tokens::TokenKind::*;
    use crate::tree::{FileKind, Tree};

    fn dump(text: &str, parse: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilderImpl)) -> String {
        let tree = Tree::parse_with(text.as_bytes(), FileKind::Kt, parse);
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
