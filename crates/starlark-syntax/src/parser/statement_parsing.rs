//! `StatementParsing` from the IntelliJ Starlark parser.

use crate::node_kind::NodeKind;
use crate::parser::messages::{message, message_with};
use crate::parser::parsing_context::ParsingContext;
use crate::token_sets::{COMPOUND_ASSIGN_OPERATIONS, ENDS_OF_STATEMENT};
use crate::tokens::TokenKind as T;
use psi_builder::builder::Marker;

impl ParsingContext<'_> {
    pub(crate) fn parse_statement(&mut self) {
        if !self.enter_nesting_level() {
            self.too_deeply_nested();
            return;
        }
        self.parse_statement_nested();
        self.exit_nesting_level();
    }

    fn parse_statement_nested(&mut self) {
        while self.at_token(T::STATEMENT_BREAK) {
            self.next_token();
        }
        let Some(first_token) = self.builder.get_token_type() else {
            return;
        };
        match first_token {
            T::IF_KEYWORD => self.parse_if_statement(),
            T::FOR_KEYWORD => {
                let end_marker = self.builder.mark();
                self.parse_for_statement(end_marker);
            }
            T::DEF_KEYWORD => {
                let function_marker = self.builder.mark();
                self.parse_function(function_marker);
            }
            _ => self.parse_simple_statement(),
        }
    }

    pub(crate) fn parse_simple_statement(&mut self) {
        let Some(first_token) = self.builder.get_token_type() else {
            return;
        };
        match first_token {
            T::BREAK_KEYWORD => self.parse_keyword_statement(NodeKind::BREAK_STATEMENT),
            T::CONTINUE_KEYWORD => self.parse_keyword_statement(NodeKind::CONTINUE_STATEMENT),
            T::PASS_KEYWORD => self.parse_keyword_statement(NodeKind::PASS_STATEMENT),
            T::RETURN_KEYWORD => self.parse_return_statement(),
            T::LOAD_KEYWORD => self.parse_load_statement(),
            _ => self.parse_expression_statement(first_token),
        }
    }

    fn report_parse_statement_error(&mut self, first_token: T) {
        let message = if first_token == T::INCONSISTENT_DEDENT {
            message("parser.unindent.does.not.match.any.outer.indent")
        } else if first_token == T::INDENT {
            message("parser.unexpected.indent")
        } else {
            message_with("parser.statement.expected.found.0", &[&format!("Starlark:{}", first_token.debug_name())])
        };
        self.builder.error(message);
    }

    fn check_end_of_statement(&mut self) {
        if self.at_token(T::STATEMENT_BREAK) {
            self.next_token();
            self.get_scope_mut().is_after_semicolon = false;
        } else if self.at_token(T::SEMICOLON) {
            if !self.get_scope().is_suite {
                self.next_token();
                self.get_scope_mut().is_after_semicolon = true;
                if self.at_token(T::STATEMENT_BREAK) {
                    self.next_token();
                    self.get_scope_mut().is_after_semicolon = false;
                }
            }
        } else if !self.builder.eof() {
            self.builder.error(message("parser.end.of.statement.expected"));
        }
    }

    fn parse_keyword_statement(&mut self, statement_type: NodeKind) {
        let statement = self.builder.mark();
        self.next_token();
        self.check_end_of_statement();
        statement.done(self.builder, statement_type);
    }

    fn parse_return_statement(&mut self) {
        self.assert_current_token(T::RETURN_KEYWORD);
        let return_statement = self.builder.mark();
        self.next_token();
        if self.builder.get_token_type().is_some() && !ENDS_OF_STATEMENT.contains_opt(self.builder.get_token_type()) {
            self.parse_expression();
        }
        self.check_end_of_statement();
        return_statement.done(self.builder, NodeKind::RETURN_STATEMENT);
    }

    fn parse_if_statement(&mut self) {
        self.assert_current_token(T::IF_KEYWORD);
        let if_statement = self.builder.mark();
        self.next_token();
        if !self.parse_test_expression(false) {
            self.builder.error(message("parser.expected.expression"));
        }
        self.parse_colon_and_suite();
        while self.at_token(T::ELIF_KEYWORD) {
            self.next_token();
            if !self.parse_test_expression(false) {
                self.builder.error(message("parser.expected.expression"));
            }
            self.parse_colon_and_suite();
        }
        if self.at_token(T::ELSE_KEYWORD) {
            self.next_token();
            self.parse_colon_and_suite();
        }
        if_statement.done(self.builder, NodeKind::IF_STATEMENT);
    }

    fn expect_colon(&mut self) -> bool {
        if self.at_token(T::COLON) {
            self.next_token();
            return true;
        }
        if self.at_token(T::STATEMENT_BREAK) {
            self.builder.error(message("parser.expected.colon"));
            return true;
        }
        let marker = self.builder.mark();
        while !self.at_any_of_tokens(&[None, Some(T::DEDENT), Some(T::STATEMENT_BREAK), Some(T::COLON)]) {
            self.next_token();
        }
        let result = self.match_token(T::COLON);
        if !result && self.at_token(T::STATEMENT_BREAK) {
            self.next_token();
        }
        marker.error(self.builder, message("parser.expected.colon"));
        result
    }

    fn parse_for_statement(&mut self, end_marker: Marker) {
        self.assert_current_token(T::FOR_KEYWORD);
        self.next_token();
        self.parse_star_targets();
        self.check_matches(T::IN_KEYWORD, "parser.expected.in");
        self.parse_expression();
        self.parse_colon_and_suite();
        end_marker.done(self.builder, NodeKind::FOR_STATEMENT);
    }

    fn parse_colon_and_suite(&mut self) {
        if self.expect_colon() {
            self.parse_suite(None, None);
        } else {
            let marker = self.builder.mark();
            marker.done(self.builder, NodeKind::STATEMENT_LIST);
        }
    }

    pub(crate) fn parse_suite(&mut self, end_marker: Option<Marker>, element_type: Option<NodeKind>) {
        if self.at_token(T::STATEMENT_BREAK) {
            self.next_token();
            let marker = self.builder.mark();
            let indent_found = self.at_token(T::INDENT);
            if indent_found {
                self.next_token();
                while !self.builder.eof() && !self.at_token(T::DEDENT) {
                    self.parse_statement();
                }
            } else {
                self.builder.error(message("parser.indent.expected"));
            }
            marker.done(self.builder, NodeKind::STATEMENT_LIST);
            if let Some(end_marker) = end_marker {
                end_marker.done(self.builder, element_type.expect("an end marker has an element type"));
            }
            if indent_found && !self.builder.eof() {
                debug_assert!(self.at_token(T::DEDENT));
                self.next_token();
            }
        } else {
            let marker = self.builder.mark();
            if self.builder.eof() {
                self.builder.error(message("parser.expected.statement"));
            } else {
                self.push_scope(self.get_scope().with_suite());
                self.parse_simple_statement();
                self.pop_scope();
                while self.match_token(T::SEMICOLON) {
                    if self.match_token(T::STATEMENT_BREAK) {
                        break;
                    }
                    self.push_scope(self.get_scope().with_suite());
                    self.parse_simple_statement();
                    self.pop_scope();
                }
            }
            marker.done(self.builder, NodeKind::STATEMENT_LIST);
            if let Some(end_marker) = end_marker {
                end_marker.done(self.builder, element_type.expect("an end marker has an element type"));
            }
        }
    }

    fn parse_load_statement(&mut self) {
        self.assert_current_token(T::LOAD_KEYWORD);
        let load_statement = self.builder.mark();
        self.next_token();
        self.parse_load_value_list();
        self.check_end_of_statement();
        load_statement.done(self.builder, NodeKind::LOAD_STATEMENT);
    }

    fn parse_load_value_list(&mut self) {
        if !self.at_token(T::LPAR) {
            self.builder.error(message("parser.expected.lpar"));
            return;
        }
        self.next_token();
        let mut first_value = true;
        while !self.at_token(T::RPAR) {
            if !first_value {
                if self.match_token(T::COMMA) {
                    if self.at_token(T::RPAR) {
                        break;
                    }
                } else {
                    self.builder.error(message("parser.expected.comma.or.rpar"));
                    break;
                }
            }
            if first_value {
                first_value = false;
                let filename_load_value = self.builder.mark();
                self.parse_load_value();
                filename_load_value.done(self.builder, NodeKind::FILENAME_LOAD_VALUE);
                continue;
            }
            if self.is_identifier_like() {
                let named_load_value = self.builder.mark();
                self.advance_identifier_like();
                if self.at_token(T::EQ) {
                    self.next_token();
                    self.parse_load_value();
                    named_load_value.done(self.builder, NodeKind::NAMED_LOAD_VALUE);
                    continue;
                }
                named_load_value.rollback_to(self.builder);
            }
            let load_value = self.builder.mark();
            self.parse_load_value();
            load_value.done(self.builder, NodeKind::STRING_LOAD_VALUE);
        }
        self.check_matches(T::RPAR, "parser.expected.rpar");
    }

    fn parse_load_value(&mut self) {
        if self.at_token(T::STRING) {
            self.build_token_element(NodeKind::STRING_LITERAL_EXPRESSION);
        } else {
            self.advance_error(message("parser.expected.string"));
        }
    }

    fn parse_expression_statement(&mut self, first_token: T) {
        let mut expression_statement = self.builder.mark();
        if self.parse_expression_optional() {
            let mut statement_type = NodeKind::EXPRESSION_STATEMENT;
            if COMPOUND_ASSIGN_OPERATIONS.contains_opt(self.builder.get_token_type()) {
                statement_type = NodeKind::AUG_ASSIGNMENT_STATEMENT;
                self.next_token();
                if !self.parse_expression_optional() {
                    self.builder.error(message("parser.expected.expression"));
                }
            } else if self.at_token(T::EQ) {
                expression_statement.rollback_to(self.builder);
                expression_statement = self.builder.mark();
                self.parse_expression_target(true);
                self.assert_current_token(T::EQ);
                statement_type = NodeKind::ASSIGNMENT_STATEMENT;
                self.next_token();
                loop {
                    let maybe_expression = self.builder.mark();
                    if !self.parse_expression_optional() {
                        maybe_expression.drop(self.builder);
                        self.builder.error(message("parser.expected.expression"));
                        break;
                    }
                    if self.at_token(T::EQ) {
                        maybe_expression.rollback_to(self.builder);
                        self.parse_expression_target(true);
                        self.assert_current_token(T::EQ);
                        self.next_token();
                    } else {
                        maybe_expression.drop(self.builder);
                        break;
                    }
                }
            }
            self.check_end_of_statement();
            expression_statement.done(self.builder, statement_type);
            return;
        }
        expression_statement.drop(self.builder);
        self.next_token();
        self.report_parse_statement_error(first_token);
    }
}
