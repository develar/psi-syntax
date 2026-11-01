//! The port of `AbstractKotlinParsing`: the helpers both `KotlinParsing` and `KotlinExpressionParsing` inherit. The
//! abstract class is a trait; its methods are provided methods, in the Java order, over `my_builder()`.
//!
//! Java overloads get suffixed names: `expect(t)`, `expect(t, message)` = `expect_with`, `expect(t, message, set)` =
//! `expect_with_recovery`; `advance()` / `advance(n)` = `advance_n`; `errorAndAdvance(message, n)` =
//! `error_and_advance_n`. A `null` token set is `None`, a `null` token type is `None`.

use std::borrow::Cow;

use crate::builder::{Marker, SemanticWhitespaceAwarePsiBuilder, WhitespacesAndCommentsBinder};
use crate::parsing::kotlin_parsing::KotlinParsing;
use crate::parsing::patterns::TokenStreamPattern;
use crate::tokens::TokenKind::*;
use crate::tokens::{ElementType, SOFT_KEYWORDS, TokenKind, TokenSet, WHITE_SPACE_OR_COMMENT_BIT_SET};

/// `AbstractKotlinParsing`. Implemented by `KotlinParsing` and `KotlinExpressionParsing`.
pub trait AbstractKotlinParsing<'b>: Sized {
    /// `myBuilder`.
    fn my_builder(&mut self) -> &mut SemanticWhitespaceAwarePsiBuilder<'b>;

    /// `isLazy`.
    fn is_lazy(&self) -> bool;

    /// `createTruncatedBuilder(eofPosition)`: `create(new TruncatedSemanticWhitespaceAwarePsiBuilder(myBuilder,
    /// eofPosition))`, where `create` is the implementor's (`KotlinParsing.create` or `KotlinExpressionParsing.create`).
    fn create_truncated_builder(&mut self, eof_position: i32) -> KotlinParsing<'_>;

    /// `getLastToken()`: the last token before the current position that is not whitespace or a comment.
    fn get_last_token(&mut self) -> Option<TokenKind> {
        let mut i = 1;
        let current_offset = self.my_builder().get_current_offset();
        while i <= current_offset && WHITE_SPACE_OR_COMMENT_BIT_SET.contains(self.my_builder().raw_lookup(-i)) {
            i += 1;
        }
        self.my_builder().raw_lookup(-i)
    }

    /// `expect(expectation, message)`.
    fn expect_with(&mut self, expectation: TokenKind, message: impl Into<Cow<'static, str>>) -> bool {
        self.expect_with_recovery(expectation, message, None)
    }

    /// `mark()`.
    fn mark(&mut self) -> Marker {
        self.my_builder().mark()
    }

    /// `error(message)`.
    fn error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.my_builder().error(message);
    }

    /// `expect(expectation, message, recoverySet)`.
    fn expect_with_recovery(
        &mut self,
        expectation: TokenKind,
        message: impl Into<Cow<'static, str>>,
        recovery_set: Option<&TokenSet>,
    ) -> bool {
        if self.expect(expectation) {
            return true;
        }

        self.error_with_recovery(message, recovery_set);

        false
    }

    /// `expect(expectation)`.
    fn expect(&mut self, expectation: TokenKind) -> bool {
        if self.at(expectation) {
            self.advance(); // expectation
            return true;
        }

        if expectation == IDENTIFIER && self.my_builder().get_token_text() == Some("`") {
            self.advance();
        }

        false
    }

    /// `expectNoAdvance(expectation, message)`.
    fn expect_no_advance(&mut self, expectation: TokenKind, message: impl Into<Cow<'static, str>>) {
        if self.at(expectation) {
            self.advance(); // expectation
            return;
        }

        self.error(message);
    }

    /// `errorWithRecovery(message, recoverySet)`.
    fn error_with_recovery(&mut self, message: impl Into<Cow<'static, str>>, recovery_set: Option<&TokenSet>) {
        let tt = self.tt();
        let at_recovery = match recovery_set {
            None => true,
            Some(recovery_set) => {
                recovery_set.contains(tt)
                    || tt == LBRACE
                    || tt == RBRACE
                    || (recovery_set.contains(EOL_OR_SEMICOLON)
                        && (self.eof() || tt == SEMICOLON || self.my_builder().newline_before_current_token()))
            }
        };
        if at_recovery {
            self.error(message);
        } else {
            self.error_and_advance(message);
        }
    }

    /// `errorAndAdvance(message)`.
    fn error_and_advance(&mut self, message: impl Into<Cow<'static, str>>) {
        self.error_and_advance_n(message, 1);
    }

    /// `errorAndAdvance(message, advanceTokenCount)`.
    fn error_and_advance_n(&mut self, message: impl Into<Cow<'static, str>>, advance_token_count: i32) {
        let err = self.mark();
        self.advance_n(advance_token_count);
        err.error(self.my_builder(), message);
    }

    /// `eof()`.
    fn eof(&mut self) -> bool {
        self.my_builder().eof()
    }

    /// `advance()`.
    fn advance(&mut self) {
        // TODO: how to report errors on bad characters? (Other than highlighting)
        self.my_builder().advance_lexer();
    }

    /// `advance(advanceTokenCount)`.
    fn advance_n(&mut self, advance_token_count: i32) {
        for _ in 0..advance_token_count {
            self.advance(); // erroneous token
        }
    }

    /// `advanceAt(current)`.
    fn advance_at(&mut self, current: TokenKind) {
        debug_assert!(self._at(current));
        self.my_builder().advance_lexer();
    }

    /// `getTokenId()`: the current token's `KtTokens` id, `INVALID_Id` at the end or for a non-Kotlin token.
    fn get_token_id(&mut self) -> i32 {
        self.tt().map_or(crate::tokens::INVALID_ID, TokenKind::token_id)
    }

    /// `tt()`.
    fn tt(&mut self) -> Option<TokenKind> {
        self.my_builder().get_token_type()
    }

    /// `_at(expectation)`: side-effect-free version of `at()`.
    fn _at(&mut self, expectation: TokenKind) -> bool {
        let token = self.tt();
        self.token_matches(token, expectation)
    }

    /// `tokenMatches(token, expectation)`.
    fn token_matches(&mut self, token: Option<TokenKind>, expectation: TokenKind) -> bool {
        if token == expectation {
            return true;
        }
        if expectation == EOL_OR_SEMICOLON {
            if self.eof() {
                return true;
            }
            if token == SEMICOLON {
                return true;
            }
            if self.my_builder().newline_before_current_token() {
                return true;
            }
        }
        false
    }

    /// `at(expectation)`: like `_at`, remapping a soft keyword's identifier to the keyword or back when expected.
    fn at(&mut self, expectation: TokenKind) -> bool {
        if self._at(expectation) {
            return true;
        }
        let token = self.tt();
        // `expectation instanceof KtKeywordToken`, and the expected keyword is soft with the token's text
        if token == IDENTIFIER
            && expectation.is_keyword()
            && expectation.is_soft_keyword()
            && expectation.value() == self.my_builder().get_token_text()
        {
            self.my_builder().remap_current_token(expectation);
            return true;
        }
        if expectation == IDENTIFIER
            && let Some(keyword_token) = token
            && keyword_token.is_keyword()
            && keyword_token.is_soft_keyword()
        {
            self.my_builder().remap_current_token(IDENTIFIER);
            return true;
        }
        false
    }

    /// `_atSet(set)`: side-effect-free version of `atSet()`.
    fn _at_set(&mut self, set: &TokenSet) -> bool {
        let token = self.tt();
        if set.contains(token) {
            return true;
        }
        if set.contains(EOL_OR_SEMICOLON) {
            if self.eof() {
                return true;
            }
            if token == SEMICOLON {
                return true;
            }
            if self.my_builder().newline_before_current_token() {
                return true;
            }
        }
        false
    }

    /// `atSet(set)`.
    fn at_set(&mut self, set: &TokenSet) -> bool {
        if self._at_set(set) {
            return true;
        }
        let token = self.tt();
        if token == IDENTIFIER {
            let keyword_token =
                self.my_builder().get_token_text().and_then(|text| TokenKind::soft_keyword_by_text(text.as_bytes()));
            if let Some(keyword_token) = keyword_token
                && set.contains(keyword_token)
            {
                self.my_builder().remap_current_token(keyword_token);
                return true;
            }
        } else {
            // We know at this point that <code>set</code> does not contain <code>token</code>
            if set.contains(IDENTIFIER)
                && let Some(keyword_token) = token
                && keyword_token.is_keyword()
                && keyword_token.is_soft_keyword()
            {
                self.my_builder().remap_current_token(IDENTIFIER);
                return true;
            }
        }
        false
    }

    /// `lookahead(k)`.
    fn lookahead(&mut self, k: i32) -> Option<TokenKind> {
        self.my_builder().look_ahead(k)
    }

    /// `consumeIf(token)`.
    fn consume_if(&mut self, token: TokenKind) -> bool {
        if self.at(token) {
            self.advance(); // token
            return true;
        }
        false
    }

    /// `skipUntil(tokenSet)`.
    // TODO: Migrate to predicates
    fn skip_until(&mut self, token_set: &TokenSet) {
        let stop_at_eol_or_semi = token_set.contains(EOL_OR_SEMICOLON);
        while !self.eof() && !token_set.contains(self.tt()) && !(stop_at_eol_or_semi && self.at(EOL_OR_SEMICOLON)) {
            self.advance();
        }
    }

    /// `errorUntil(message, tokenSet)`.
    fn error_until(&mut self, message: impl Into<Cow<'static, str>>, token_set: &TokenSet) {
        debug_assert!(token_set.contains(LBRACE), "Cannot include LBRACE into error element!");
        debug_assert!(token_set.contains(RBRACE), "Cannot include RBRACE into error element!");
        let error = self.mark();
        self.skip_until(token_set);
        error.error(self.my_builder(), message);
    }

    /// `errorIf(marker, condition, message)`.
    fn error_if(&mut self, marker: Marker, condition: bool, message: impl Into<Cow<'static, str>>) {
        if condition {
            marker.error(self.my_builder(), message);
        } else {
            marker.drop(self.my_builder());
        }
    }

    /// `matchTokenStreamPredicate(pattern)`: runs `pattern` over the tokens ahead, tracking bracket nesting, and rolls
    /// back; returns `pattern.result()`.
    fn match_token_stream_predicate(&mut self, pattern: &mut dyn TokenStreamPattern<Self>) -> i32 {
        let current_position = self.mark();
        let mut opens: Vec<TokenKind> = Vec::new();
        let mut open_angle_brackets = 0;
        let mut open_braces = 0;
        let mut open_parentheses = 0;
        let mut open_brackets = 0;
        while !self.eof() {
            let offset = self.my_builder().get_current_offset();
            let top_level = pattern.is_top_level(open_angle_brackets, open_brackets, open_braces, open_parentheses);
            if pattern.process_token(self, offset, top_level) {
                break;
            }
            match self.tt() {
                Some(LPAR) => {
                    open_parentheses += 1;
                    opens.push(LPAR);
                }
                Some(LT) => {
                    open_angle_brackets += 1;
                    opens.push(LT);
                }
                Some(LBRACE) => {
                    open_braces += 1;
                    opens.push(LBRACE);
                }
                Some(LBRACKET) => {
                    open_brackets += 1;
                    opens.push(LBRACKET);
                }
                Some(RPAR) => {
                    open_parentheses -= 1;
                    if opens.pop() != Some(LPAR) {
                        // Java `break`s out of the switch here either way: the token is still skipped.
                        let _ = pattern.handle_unmatched_closing(RPAR);
                    }
                }
                Some(GT) => open_angle_brackets -= 1,
                Some(RBRACE) => open_braces -= 1,
                Some(RBRACKET) => open_brackets -= 1,
                _ => {}
            }

            self.advance(); // skip token
        }

        current_position.rollback_to(self.my_builder());

        pattern.result()
    }

    /// `eol()`.
    fn eol(&mut self) -> bool {
        self.my_builder().newline_before_current_token() || self.eof()
    }

    /// `closeDeclarationWithCommentBinders(marker, elementType, precedingNonDocComments)`.
    fn close_declaration_with_comment_binders(
        &mut self,
        marker: Marker,
        element_type: impl Into<ElementType>,
        preceding_non_doc_comments: bool,
    ) {
        marker.done(self.my_builder(), element_type);
        marker.set_custom_edge_token_binders(
            self.my_builder(),
            Some(if preceding_non_doc_comments {
                WhitespacesAndCommentsBinder::PrecedingComments
            } else {
                WhitespacesAndCommentsBinder::PrecedingDocComments
            }),
            Some(WhitespacesAndCommentsBinder::TrailingComments),
        );
    }

    /// `currentContext()` (`@TestOnly`): the text around the current offset.
    fn current_context(&mut self) -> String {
        let offset = self.my_builder().get_current_offset().max(0) as usize;
        let text = self.my_builder().get_original_text();
        let start = offset.saturating_sub(20);
        let end = (offset + 20).min(text.len());
        format!(
            "{}<caret>{}",
            String::from_utf8_lossy(&text[start..offset.min(text.len())]),
            String::from_utf8_lossy(&text[offset.min(text.len())..end])
        )
    }
}

/// `AbstractKotlinParsing.OptionalMarker`: a marker only when `actuallyMark`, that never makes an empty error.
#[derive(Copy, Clone, Debug)]
pub struct OptionalMarker {
    marker: Option<Marker>,
    offset: i32,
}

impl OptionalMarker {
    /// `new OptionalMarker(actuallyMark)`.
    pub fn new<'b>(parsing: &mut impl AbstractKotlinParsing<'b>, actually_mark: bool) -> Self {
        let marker = if actually_mark { Some(parsing.mark()) } else { None };
        let offset = parsing.my_builder().get_current_offset();
        OptionalMarker { marker, offset }
    }

    pub fn done<'b>(&self, parsing: &mut impl AbstractKotlinParsing<'b>, element_type: impl Into<ElementType>) {
        let Some(marker) = self.marker else { return };
        marker.done(parsing.my_builder(), element_type);
    }

    pub fn error<'b>(&self, parsing: &mut impl AbstractKotlinParsing<'b>, message: impl Into<Cow<'static, str>>) {
        let Some(marker) = self.marker else { return };
        if self.offset == parsing.my_builder().get_current_offset() {
            marker.drop(parsing.my_builder()); // no empty errors
        } else {
            marker.error(parsing.my_builder(), message);
        }
    }

    pub fn drop<'b>(&self, parsing: &mut impl AbstractKotlinParsing<'b>) {
        let Some(marker) = self.marker else { return };
        marker.drop(parsing.my_builder());
    }
}

/// `AbstractKotlinParsing.SOFT_KEYWORD_TEXTS`: the soft keywords by their text.
pub fn soft_keyword_texts(text: &str) -> Option<TokenKind> {
    let keyword = TokenKind::soft_keyword_by_text(text.as_bytes())?;
    debug_assert!(SOFT_KEYWORDS.contains(keyword));
    Some(keyword)
}
