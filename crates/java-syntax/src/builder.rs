//! The builder as the Java parsers see it: platform/syntax's `SyntaxTreeBuilder` (the generic `PsiBuilderImpl` of
//! `psi_builder` over `Java`), the `JavaParserUtil.stoppingBuilder` adapter, the helpers of `SyntaxBuilderUtil`, and
//! Java's `WhitespacesAndCommentsBinder`s (`WhitespacesBinders`' two defaults, `WhiteSpaceAndCommentSetHolder`'s
//! preceding and trailing comment binders).
//!
//! A parser method that takes `builder: SyntaxTreeBuilder` takes `builder: &mut SyntaxTreeBuilder<'_>`; a marker is
//! `psi_builder::Marker`, whose methods take the builder: `marker.done(builder, NodeKind::CLASS)`.
//!
//! Where `SyntaxTreeBuilderImpl` and `PsiBuilderImpl` differ, the parser lanes check the port against the oracle;
//! known points: `enforceCommentTokens` (see `SyntaxTreeBuilder::enforce_comment_tokens`), `lastDoneMarker` (a
//! `psi_builder` method), `getStartOffset`/`getEndOffset` (relative to the builder's text, which is all the Java
//! parsers compare).

use std::borrow::Cow;

use psi_builder::{AsPsiBuilder, Marker, WhitespacesAndCommentsBinder};

use crate::Java;
use crate::language_level::{JavaFeature, LanguageLevel};
use crate::node_kind::NodeKind;
use crate::syntax_element_types::{self, JAVA_PLAIN_COMMENT_BIT_SET};
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `SyntaxTreeBuilderImpl` for Java: `psi_builder`'s builder.
pub type PsiBuilderImpl = psi_builder::PsiBuilderImpl<Java>;
/// The buffers a Java builder works in.
pub type BuilderBuffers = psi_builder::BuilderBuffers<Java>;

/// Java's `WhitespacesAndCommentsBinder`s.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Binder {
    /// `WhitespacesBinders.defaultLeftBinder()` = `greedyRightBinder()`: `tokens.size`.
    DefaultLeft,
    /// `WhitespacesBinders.defaultRightBinder()` = `greedyLeftBinder()`: 0.
    DefaultRight,
    /// `PrecedingWhitespacesAndCommentsBinder(myAfterEmptyImport, mySupportMarkdown)`.
    Preceding { after_empty_import: bool, support_markdown: bool },
    /// `TrailingWhitespacesAndCommentsBinder`.
    Trailing,
    /// `JavaDocParser.fakeCollapseRightBinder`: takes the next space if it is not an end of line.
    FakeCollapseRight,
}

/// JavaDocParser's `isEolToken(type, text)`: a whitespace token (`DOC_SPACE`, `WHITE_SPACE`) with a `\n`.
pub fn is_eol_token(token_type: Option<TokenKind>, text: &[u8]) -> bool {
    matches!(token_type, Some(TokenKind::DOC_SPACE | TokenKind::WHITE_SPACE)) && text.contains(&b'\n')
}

/// `getLineBreakCount()`: `\n`, and `\r` not followed by `\n`.
fn line_break_count(text: &[u8]) -> usize {
    let mut count = 0;
    for (i, &b) in text.iter().enumerate() {
        if b == b'\n' || (b == b'\r' && text.get(i + 1) != Some(&b'\n')) {
            count += 1;
        }
    }
    count
}

/// `containsLineBreak()`.
fn contains_line_break(text: &[u8]) -> bool {
    text.iter().any(|&b| b == b'\n' || b == b'\r')
}

impl WhitespacesAndCommentsBinder<TokenKind> for Binder {
    const DEFAULT_LEFT_BINDER: Binder = Binder::DefaultLeft;
    const DEFAULT_RIGHT_BINDER: Binder = Binder::DefaultRight;

    fn get_edge_position<'t>(
        self,
        tokens: &[TokenKind],
        at_stream_edge: bool,
        getter: &dyn Fn(usize) -> &'t [u8],
    ) -> usize {
        use TokenKind::*;
        match self {
            Binder::DefaultLeft => tokens.len(),
            Binder::DefaultRight => 0,
            Binder::Preceding { after_empty_import, support_markdown } => {
                if tokens.is_empty() {
                    return 0;
                }

                // 1. bind doc comment
                // now there are markdown comments.
                if support_markdown {
                    //collect everything
                    for idx in (0..tokens.len()).rev() {
                        if tokens[idx] == DOC_COMMENT || tokens[idx] == DOC_MARKDOWN_COMMENT {
                            return idx;
                        }
                    }
                } else {
                    // To preserve previous orders, let's try to find the first non-markdown comment (and skip markdown
                    // comments). If there is no non-markdown, take the first markdown
                    for idx in (0..tokens.len()).rev() {
                        if tokens[idx] == DOC_COMMENT {
                            return idx;
                        }
                    }

                    for idx in (0..tokens.len()).rev() {
                        if tokens[idx] == DOC_MARKDOWN_COMMENT {
                            return idx;
                        }
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
                    } else if JAVA_PLAIN_COMMENT_BIT_SET.contains(token_type) {
                        if at_stream_edge
                            || (idx == 0 && after_empty_import)
                            || (idx > 0 && tokens[idx - 1] == WHITE_SPACE) && contains_line_break(getter(idx - 1))
                        {
                            result = idx;
                        }
                    } else {
                        break;
                    }
                }

                result
            }
            Binder::Trailing => {
                if tokens.is_empty() {
                    return 0;
                }

                let mut result = 0;
                for (idx, &token_type) in tokens.iter().enumerate() {
                    if token_type == WHITE_SPACE {
                        if contains_line_break(getter(idx)) {
                            break;
                        }
                    } else if JAVA_PLAIN_COMMENT_BIT_SET.contains(token_type) {
                        result = idx + 1;
                    } else {
                        break;
                    }
                }

                result
            }
            Binder::FakeCollapseRight => {
                if !tokens.is_empty() && !is_eol_token(Some(tokens[0]), getter(0)) {
                    return 1;
                }
                0
            }
        }
    }

    /// With a negative size the views are not empty and have no indices: the preceding binder falls through to
    /// `tokens.size`, the trailing one to 0; JavaDocParser's reads token 0 unchecked.
    fn negative_size_edge<'t>(
        self,
        size: i32,
        get: impl Fn(i32) -> Option<TokenKind>,
        text: &dyn Fn(i32) -> &'t [u8],
    ) -> i32 {
        match self {
            Binder::DefaultLeft | Binder::Preceding { .. } => size,
            Binder::DefaultRight | Binder::Trailing => 0,
            Binder::FakeCollapseRight => !is_eol_token(get(0), text(0)) as i32,
        }
    }
}

/// `SyntaxTreeBuilder` as a Java parser method receives it: the builder, or `JavaParserUtil.stoppingBuilder` over it
/// (a `SyntaxTreeBuilderAdapter` whose `tokenType` is null and `eof()` true from the offset `stopAt` on; nested ones
/// stop at the nearest). Every other method is the builder's own.
pub struct SyntaxTreeBuilder<'b> {
    b: &'b mut PsiBuilderImpl,
    /// The `stopAt` of the innermost stopping builder; `i32::MAX` when there is none.
    stop_at: i32,
}

impl AsPsiBuilder for SyntaxTreeBuilder<'_> {
    type Lang = Java;
    #[inline(always)]
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl {
        self.b
    }
}

impl<'b> SyntaxTreeBuilder<'b> {
    /// The builder itself, without an adapter.
    pub fn new(b: &'b mut PsiBuilderImpl) -> Self {
        SyntaxTreeBuilder { b, stop_at: i32::MAX }
    }

    /// `JavaParserUtil.stoppingBuilder(builder, stopAt)`: this builder ending at the offset `stop_at`.
    pub fn stopping_builder(&mut self, stop_at: i32) -> SyntaxTreeBuilder<'_> {
        SyntaxTreeBuilder { b: self.b, stop_at: self.stop_at.min(stop_at) }
    }

    /// `tokenType`: the current token, `None` at the end (or from the stopping offset on).
    #[inline]
    pub fn token_type(&mut self) -> Option<TokenKind> {
        if self.stop_at != i32::MAX && self.b.get_current_offset() >= self.stop_at {
            return None;
        }
        self.b.get_token_type()
    }

    /// `eof()`.
    #[inline]
    pub fn eof(&mut self) -> bool {
        (self.stop_at != i32::MAX && self.b.get_current_offset() >= self.stop_at) || self.b.eof()
    }

    /// `advanceLexer()`.
    #[inline]
    pub fn advance_lexer(&mut self) {
        self.b.advance_lexer();
    }

    /// `mark()`.
    #[inline]
    pub fn mark(&mut self) -> Marker {
        self.b.mark()
    }

    /// `error(message)`.
    pub fn error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.b.error(message);
    }

    /// `tokenText`: the current token's text as bytes, `None` at the end (the builder's: not stopped).
    pub fn token_text(&mut self) -> Option<&[u8]> {
        self.b.get_token_bytes()
    }

    /// `text`: the builder's whole text.
    pub fn text(&self) -> &[u8] {
        self.b.get_original_text()
    }

    /// `currentOffset`.
    pub fn current_offset(&mut self) -> i32 {
        self.b.get_current_offset()
    }

    /// `lookAhead(steps)`.
    pub fn look_ahead(&self, steps: i32) -> Option<TokenKind> {
        self.b.look_ahead(steps)
    }

    /// `rawLookup(steps)`.
    pub fn raw_lookup(&self, steps: i32) -> Option<TokenKind> {
        self.b.raw_lookup(steps)
    }

    /// `rawTokenTypeStart(steps)`.
    pub fn raw_token_type_start(&self, steps: i32) -> i32 {
        self.b.raw_token_type_start(steps)
    }

    /// `rawTokenIndex()`.
    pub fn raw_token_index(&self) -> i32 {
        self.b.raw_token_index()
    }

    /// `rawAdvanceLexer(steps)`.
    pub fn raw_advance_lexer(&mut self, steps: i32) {
        self.b.raw_advance_lexer(steps);
    }

    /// `remapCurrentToken(type)`.
    pub fn remap_current_token(&mut self, element_type: TokenKind) {
        self.b.remap_current_token(element_type);
    }

    /// `hasErrorsAfter(marker)`.
    pub fn has_errors_after(&self, marker: Marker) -> bool {
        self.b.has_errors_after(marker)
    }

    /// `lastDoneMarker`.
    pub fn last_done_marker(&self) -> Option<Marker> {
        self.b.last_done_marker()
    }

    /// `enforceCommentTokens(tokens)`: JavaDocParser enforces `{DOC_COMMENT_LEADING_ASTERISKS}` as the comments of a
    /// doc comment's builder before it reads a token. `Java::is_whitespace_or_comment` already counts
    /// DOC_COMMENT_LEADING_ASTERISKS (which occurs in no other builder) and the Java comments (which occur in no doc
    /// comment's builder), so this is the same builder; it only checks that the set is that one.
    pub fn enforce_comment_tokens(&mut self, tokens: TokenSet) {
        debug_assert_eq!(tokens, TokenSet::create(&[TokenKind::DOC_COMMENT_LEADING_ASTERISKS]));
    }

    /// `Marker.getNodeType()`.
    pub fn marker_node_type(&self, marker: Marker) -> Option<ElementType> {
        self.b.marker_node_type(marker)
    }

    /// `Marker.getStartOffset()`.
    pub fn marker_start_offset(&self, marker: Marker) -> i32 {
        self.b.marker_start_offset(marker)
    }

    /// `Marker.getEndOffset()`.
    pub fn marker_end_offset(&self, marker: Marker) -> i32 {
        self.b.marker_end_offset(marker)
    }

    /// `Marker.getStartTokenIndex()`.
    pub fn marker_start_token_index(&self, marker: Marker) -> i32 {
        self.b.marker_start_token_index(marker)
    }

    /// `Marker.getEndTokenIndex()`.
    pub fn marker_end_token_index(&self, marker: Marker) -> i32 {
        self.b.marker_end_token_index(marker)
    }

    // SyntaxBuilderUtil

    /// `SyntaxBuilderUtil.advance(count)`: advances by `count` tokens, not beyond the end.
    pub fn advance(&mut self, count: i32) {
        for _ in 0..count {
            if self.eof() {
                return;
            }
            self.token_type(); // ensure token is processed
            self.advance_lexer();
        }
    }

    /// `SyntaxBuilderUtil.expect(expectedType)`: advances over the current token if it is `expected_type`.
    pub fn expect(&mut self, expected_type: Option<TokenKind>) -> bool {
        if self.token_type() == expected_type {
            self.advance_lexer();
            return true;
        }
        false
    }

    /// `SyntaxBuilderUtil.expect(expectedTypes)`.
    pub fn expect_set(&mut self, expected_types: TokenSet) -> bool {
        if expected_types.contains(self.token_type()) {
            self.advance_lexer();
            return true;
        }
        false
    }

    /// `SyntaxBuilderUtil.rawTokenText(index)`: the text of the raw token `index` steps from the current one.
    pub fn raw_token_text(&self, index: i32) -> &[u8] {
        let text = self.b.get_original_text();
        let start = self.raw_token_type_start(index).clamp(0, text.len() as i32) as usize;
        let end = self.raw_token_type_start(index + 1).clamp(0, text.len() as i32) as usize;
        &text[start..end.max(start)]
    }

    /// `SyntaxBuilderUtil.parseBlockLazy(leftBrace, rightBrace, codeBlock)`: a collapsed marker of `code_block` from
    /// `left_brace` to the `right_brace` that balances it (to the end when none does), `None` without `left_brace`.
    pub fn parse_block_lazy(
        &mut self,
        left_brace: TokenKind,
        right_brace: TokenKind,
        code_block: NodeKind,
    ) -> Option<Marker> {
        if self.token_type() != Some(left_brace) {
            return None;
        }

        let marker = self.mark();

        self.advance_lexer();

        let mut brace_count = 1;

        while brace_count > 0 && !self.eof() {
            let token_type = self.token_type();
            if token_type == Some(left_brace) {
                brace_count += 1;
            } else if token_type == Some(right_brace) {
                brace_count -= 1;
            }
            self.advance_lexer();
        }

        marker.collapse(self, code_block);

        if brace_count > 0 {
            marker.set_custom_edge_token_binders(self, None, Some(Binder::DefaultLeft));
        }

        Some(marker)
    }
}

/// `WhiteSpaceAndCommentSetHolder.getPrecedingCommentBinder(level)`.
pub fn get_preceding_comment_binder(my_language_level: LanguageLevel) -> Binder {
    Binder::Preceding {
        after_empty_import: false,
        support_markdown: JavaFeature::MARKDOWN_COMMENT.is_sufficient(my_language_level),
    }
}

/// `WhiteSpaceAndCommentSetHolder.getSpecialPrecedingCommentBinder(level)`.
pub fn get_special_preceding_comment_binder(my_language_level: LanguageLevel) -> Binder {
    Binder::Preceding {
        after_empty_import: true,
        support_markdown: JavaFeature::MARKDOWN_COMMENT.is_sufficient(my_language_level),
    }
}

/// `WhiteSpaceAndCommentSetHolder.trailingCommentBinder`.
pub fn trailing_comment_binder() -> Binder {
    Binder::Trailing
}

/// `WhiteSpaceAndCommentSetHolder.precedingCommentSet`.
pub const PRECEDING_COMMENT_SET: TokenSet = TokenSet::or_set(&[
    TokenSet::create_nodes(&[NodeKind::MODULE, NodeKind::IMPLICIT_CLASS]),
    syntax_element_types::FULL_MEMBER_BIT_SET,
]);

/// `WhiteSpaceAndCommentSetHolder.trailingCommentSet`.
pub const TRAILING_COMMENT_SET: TokenSet = TokenSet::or_set(&[
    TokenSet::create_nodes(&[NodeKind::PACKAGE_STATEMENT]),
    syntax_element_types::IMPORT_STATEMENT_BASE_BIT_SET,
    syntax_element_types::FULL_MEMBER_BIT_SET,
    syntax_element_types::JAVA_STATEMENT_BIT_SET,
]);

/// `SyntaxBuilderUtil.drop(markers...)`.
pub fn drop(builder: &mut SyntaxTreeBuilder<'_>, markers: &[Option<Marker>]) {
    for marker in markers.iter().flatten() {
        marker.drop(builder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_binder_takes_comments_up_to_a_line_break() {
        use TokenKind::*;
        let texts: [&[u8]; 4] = [b" ", b"// a", b"\n", b"// b"];
        let tokens = [WHITE_SPACE, END_OF_LINE_COMMENT, WHITE_SPACE, END_OF_LINE_COMMENT];
        assert_eq!(Binder::Trailing.get_edge_position(&tokens, false, &|i| texts[i]), 2);
        let preceding = Binder::Preceding { after_empty_import: false, support_markdown: true };
        assert_eq!(preceding.get_edge_position(&tokens, false, &|i| texts[i]), 3);
    }
}
