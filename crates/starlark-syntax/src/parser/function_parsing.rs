//! `FunctionParsing` from the IntelliJ Starlark parser.

use crate::node_kind::NodeKind;
use crate::parser::messages::message;
use crate::parser::parsing_context::ParsingContext;
use crate::tokens::TokenKind as T;
use psi_builder::builder::Marker;

impl ParsingContext<'_> {
    pub(crate) fn parse_function(&mut self, function_marker: Marker) {
        self.assert_current_token(T::DEF_KEYWORD);
        self.next_token();
        self.parse_identifier_or_skip();
        self.parse_parameter_list();
        self.check_matches(T::COLON, "parser.expected.colon");
        self.push_scope(self.get_scope().with_function());
        self.parse_suite(Some(function_marker), Some(NodeKind::FUNCTION_DECLARATION));
        self.pop_scope();
    }

    fn parse_parameter_list(&mut self) {
        if !self.at_token(T::LPAR) {
            self.builder.error(message("parser.expected.lpar"));
            let marker = self.builder.mark();
            marker.done(self.builder, NodeKind::PARAMETER_LIST);
            return;
        }
        self.parse_parameter_list_contents(T::RPAR, true);
    }

    pub(crate) fn parse_parameter_list_contents(&mut self, end_token: T, advance_lexer: bool) {
        let parameter_list = self.builder.mark();
        if advance_lexer {
            self.next_token();
        }
        let mut first = true;
        let mut after_star_parameter = false;
        while !self.at_token(end_token) {
            if first {
                first = false;
            } else if self.at_token(T::COMMA) {
                self.next_token();
            } else {
                self.builder.error(message("parser.expected.comma.lpar.rpar"));
                break;
            }
            let is_star_parameter = self.at_any_of_tokens(&[Some(T::MULT), Some(T::EXP)]);
            if !self.parse_parameter(end_token) {
                if after_star_parameter {
                    self.builder.error(message("parser.expected.expression"));
                }
                break;
            }
            if is_star_parameter {
                after_star_parameter = true;
            }
        }
        if end_token == T::RPAR && self.at_token(end_token) {
            self.next_token();
        }
        parameter_list.done(self.builder, NodeKind::PARAMETER_LIST);
        if end_token == T::COLON && self.at_token(T::COLON) {
            self.next_token();
        }
    }

    fn parse_parameter(&mut self, end_token: T) -> bool {
        let parameter = self.builder.mark();
        let mut parameter_type = NodeKind::MANDATORY_PARAMETER;
        if self.at_token(T::MULT) {
            self.next_token();
            if self.at_token(T::COMMA) {
                parameter.done(self.builder, NodeKind::KEYWORD_ONLY_BOUNDARY);
                return true;
            }
            parameter_type = NodeKind::VARIADIC_PARAMETER;
        } else if self.at_token(T::EXP) {
            self.next_token();
            parameter_type = NodeKind::KEYWORD_VARIADIC_PARAMETER;
        }
        if self.match_token(T::IDENTIFIER) {
            if parameter_type == NodeKind::MANDATORY_PARAMETER && self.match_token(T::EQ) {
                if !self.parse_single_expression(false) {
                    let invalid_elements = self.builder.mark();
                    while !self.at_any_of_tokens(&[Some(end_token), Some(T::LINE_BREAK), Some(T::COMMA), None]) {
                        self.next_token();
                    }
                    invalid_elements.error(self.builder, message("parser.expected.expression"));
                }
                parameter.done(self.builder, NodeKind::OPTIONAL_PARAMETER);
                return true;
            }
            parameter.done(self.builder, parameter_type);
            return true;
        }

        parameter.rollback_to(self.builder);
        if self.at_token(end_token) {
            return false;
        }
        let invalid_elements = self.builder.mark();
        while !self.at_any_of_tokens(&[Some(end_token), Some(T::LINE_BREAK), Some(T::COMMA), None]) {
            self.next_token();
        }
        invalid_elements.error(self.builder, message("parser.expected.formal.param.name"));
        self.at_token(end_token) || self.at_token(T::COMMA)
    }
}
