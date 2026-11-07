//! The common parsing operations ported from IntelliJ's `Parsing` base class.

use crate::node_kind::NodeKind;
use crate::parser::messages::{message, message_with};
use crate::parser::parsing_context::ParsingContext;
use crate::token_sets::RESERVED_KEYWORDS;
use crate::tokens::TokenKind as T;

impl ParsingContext<'_> {
    pub(crate) fn check_matches(&mut self, token: T, error_key: &str) -> bool {
        if self.at_token(token) {
            self.next_token();
            true
        } else {
            self.builder.error(message(error_key));
            false
        }
    }

    pub(crate) fn parse_identifier_or_skip(&mut self) -> bool {
        if self.at_token(T::IDENTIFIER) {
            self.next_token();
            true
        } else {
            let name_expected = self.builder.mark();
            if !self.at_token(T::STATEMENT_BREAK) && !self.at_token(T::LPAR) {
                self.next_token();
            }
            name_expected.error(self.builder, message("parser.expected.identifier"));
            false
        }
    }

    pub(crate) fn assert_current_token(&mut self, token: T) {
        debug_assert_eq!(self.builder.get_token_type(), Some(token));
    }

    pub(crate) fn at_token(&mut self, token: T) -> bool {
        self.builder.get_token_type() == Some(token)
    }

    pub(crate) fn at_any_of_tokens(&mut self, tokens: &[Option<T>]) -> bool {
        let current = self.builder.get_token_type();
        tokens.contains(&current)
    }

    pub(crate) fn match_token(&mut self, token: T) -> bool {
        if self.at_token(token) {
            self.next_token();
            true
        } else {
            false
        }
    }

    pub(crate) fn next_token(&mut self) {
        self.builder.advance_lexer();
    }

    pub(crate) fn advance_identifier_like(&mut self) {
        if RESERVED_KEYWORDS.contains_opt(self.builder.get_token_type()) {
            let token_text = self.builder.get_token_text().unwrap_or("").to_owned();
            let error = message_with("parser.reserved.keyword.cannot.be.used.as.identifier", &[&token_text]);
            self.advance_error(error);
        } else {
            self.next_token();
        }
    }

    pub(crate) fn advance_error(&mut self, error: impl Into<String>) {
        let marker = self.builder.mark();
        self.next_token();
        marker.error(self.builder, error.into());
    }

    pub(crate) fn is_identifier_like(&mut self) -> bool {
        self.at_token(T::IDENTIFIER) || RESERVED_KEYWORDS.contains_opt(self.builder.get_token_type())
    }

    pub(crate) fn build_token_element(&mut self, element_type: NodeKind) {
        let marker = self.builder.mark();
        self.advance_identifier_like();
        marker.done(self.builder, element_type);
    }
}
