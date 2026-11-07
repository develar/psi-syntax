//! `ExpressionParsing` from the IntelliJ Starlark parser.

use crate::node_kind::NodeKind;
use crate::parser::messages::message;
use crate::parser::parsing_context::ParsingContext;
use crate::token_sets::{
    ADDITIVE_OPERATIONS, BRACKET_COLON_COMMA, BRACKET_OR_COMMA, COMPARISON_OPERATIONS, MULTIPLICATIVE_OPERATIONS,
    PRIMARY_EXPRESSION_STARTERS, SHIFT_OPERATIONS, UNARY_OPERATIONS,
};
use crate::tokens::TokenKind as T;
use psi_builder::builder::Marker;

impl ParsingContext<'_> {
    fn parse_primary_expression(&mut self, is_target: bool) -> bool {
        if !PRIMARY_EXPRESSION_STARTERS.contains_opt(self.builder.get_token_type()) {
            return false;
        }
        match self.builder.get_token_type() {
            Some(T::IDENTIFIER) => {
                let element_type = self.get_target_or_reference_expression(is_target);
                self.build_token_element(element_type);
            }
            Some(T::INT) => self.build_token_element(NodeKind::INTEGER_LITERAL_EXPRESSION),
            Some(T::FLOAT) => self.build_token_element(NodeKind::FLOAT_LITERAL_EXPRESSION),
            Some(T::NONE_KEYWORD) => self.build_token_element(NodeKind::NONE_LITERAL_EXPRESSION),
            Some(T::TRUE_KEYWORD) => self.build_token_element(NodeKind::TRUE_LITERAL_EXPRESSION),
            Some(T::FALSE_KEYWORD) => self.build_token_element(NodeKind::FALSE_LITERAL_EXPRESSION),
            Some(T::LPAR) => self.parse_parenthesized_expression(is_target),
            Some(T::LBRACKET) => self.parse_list_literal_expression(is_target),
            Some(T::LBRACE) => self.parse_dict_display(),
            _ => self.build_token_element(NodeKind::STRING_LITERAL_EXPRESSION),
        }
        true
    }

    fn parse_list_literal_expression(&mut self, is_target: bool) {
        self.assert_current_token(T::LBRACKET);
        let expression = self.builder.mark();
        self.next_token();
        if self.at_token(T::RBRACKET) {
            self.next_token();
            expression.done(self.builder, NodeKind::LIST_LITERAL_EXPRESSION);
            return;
        }
        if !self.parse_single_expression(is_target) {
            self.builder.error(message("parser.expected.expression"));
        }
        if self.at_token(T::FOR_KEYWORD) {
            self.parse_comprehension(expression, Some(T::RBRACKET), NodeKind::LIST_COMP_EXPRESSION);
        } else {
            while !self.at_token(T::RBRACKET) {
                if !self.match_token(T::COMMA) {
                    self.builder.error(message("parser.rbracket.or.comma.expected"));
                }
                if self.at_token(T::RBRACKET) {
                    break;
                }
                if !self.parse_single_expression(is_target) {
                    self.builder.error(message("parser.expected.expr.or.comma.or.bracket"));
                    break;
                }
            }
            self.check_matches(T::RBRACKET, "parser.expected.rbracket");
            expression.done(self.builder, NodeKind::LIST_LITERAL_EXPRESSION);
        }
    }

    fn parse_comprehension(&mut self, expression: Marker, end_token: Option<T>, expression_type: NodeKind) {
        self.assert_current_token(T::FOR_KEYWORD);
        loop {
            self.next_token();
            self.parse_star_targets();
            self.parse_comprehension_range(expression_type == NodeKind::GENERATOR_EXPRESSION);
            while self.at_token(T::IF_KEYWORD) {
                self.next_token();
                if !self.parse_old_test_expression(false) {
                    self.builder.error(message("parser.expected.expression"));
                }
            }
            if self.at_token(T::FOR_KEYWORD) {
                continue;
            }
            if end_token.is_none() || end_token.is_some_and(|token| self.match_token(token)) {
                break;
            }
            self.builder.error(message("parser.expected.for.or.bracket"));
            break;
        }
        expression.done(self.builder, expression_type);
    }

    pub(crate) fn parse_star_targets(&mut self) {
        let expression = self.builder.mark();
        if !self.parse_star_expression(true) {
            self.builder.error(message("parser.expected.expression"));
            expression.drop(self.builder);
            return;
        }
        if self.at_token(T::COMMA) {
            while self.at_token(T::COMMA) {
                self.next_token();
                let expression2 = self.builder.mark();
                if !self.parse_star_expression(true) {
                    self.builder.error(message("parser.expected.expression"));
                    expression2.rollback_to(self.builder);
                    break;
                }
                expression2.drop(self.builder);
            }
            expression.done(self.builder, NodeKind::TUPLE_EXPRESSION);
        } else {
            expression.drop(self.builder);
        }
    }

    fn parse_comprehension_range(&mut self, generator_expression: bool) {
        self.check_matches(T::IN_KEYWORD, "parser.expected.in");
        let parsed = if generator_expression {
            self.parse_or_test_expression(false)
        } else {
            self.parse_tuple_expression(false, true)
        };
        if !parsed {
            self.builder.error(message("parser.expected.expression"));
        }
    }

    fn parse_dict_display(&mut self) {
        self.assert_current_token(T::LBRACE);
        let expression = self.builder.mark();
        self.next_token();
        if self.match_token(T::RBRACE) {
            expression.done(self.builder, NodeKind::DICT_LITERAL_EXPRESSION);
            return;
        }
        if self.at_token(T::EXP) {
            if !self.parse_double_star_expression() {
                self.builder.error(message("parser.expected.expression"));
                expression.done(self.builder, NodeKind::DICT_LITERAL_EXPRESSION);
                return;
            }
            self.parse_dict_literal_content_tail(expression);
            return;
        }
        self.parse_dict_tail(expression, false);
    }

    fn parse_dict_tail(&mut self, expression: Marker, is_dict: bool) {
        let first_expression = self.builder.mark();
        if !self.parse_single_expression(false) {
            self.builder.error(message("parser.expected.expression"));
            first_expression.drop(self.builder);
            expression.done(self.builder, NodeKind::DICT_LITERAL_EXPRESSION);
            return;
        }
        if self.match_token(T::COLON) {
            if !is_dict {
                first_expression.rollback_to(self.builder);
                self.parse_dict_tail(expression, true);
                return;
            }
            self.parse_dict_literal_tail(expression, first_expression);
        } else {
            self.builder.error(message("parser.expected.expression"));
            first_expression.drop(self.builder);
            expression.done(self.builder, NodeKind::DICT_LITERAL_EXPRESSION);
        }
    }

    fn parse_dict_literal_tail(&mut self, start_marker: Marker, first_key_value: Marker) {
        if !self.parse_single_expression(false) {
            self.builder.error(message("parser.expected.expression"));
            first_key_value.done(self.builder, NodeKind::KEY_VALUE_EXPRESSION);
            if self.at_token(T::RBRACE) {
                self.next_token();
            }
            start_marker.done(self.builder, NodeKind::DICT_LITERAL_EXPRESSION);
            return;
        }
        first_key_value.done(self.builder, NodeKind::KEY_VALUE_EXPRESSION);
        if self.at_token(T::FOR_KEYWORD) {
            self.parse_comprehension(start_marker, Some(T::RBRACE), NodeKind::DICT_COMP_EXPRESSION);
        } else {
            self.parse_dict_literal_content_tail(start_marker);
        }
    }

    fn parse_dict_literal_content_tail(&mut self, start_marker: Marker) {
        while !self.at_token(T::RBRACE) {
            self.check_matches(T::COMMA, "parser.expected.comma");
            if self.at_token(T::EXP) {
                if !self.parse_double_star_expression() {
                    break;
                }
            } else if !self.parse_key_value_expression() {
                break;
            }
        }
        self.check_matches(T::RBRACE, "parser.expected.rbrace");
        start_marker.done(self.builder, NodeKind::DICT_LITERAL_EXPRESSION);
    }

    fn parse_key_value_expression(&mut self) -> bool {
        let marker = self.builder.mark();
        if !self.parse_single_expression(false) {
            marker.drop(self.builder);
            return false;
        }
        self.check_matches(T::COLON, "parser.expected.colon");
        if !self.parse_single_expression(false) {
            self.builder.error(message("parser.value.expression.expected"));
            marker.drop(self.builder);
            return false;
        }
        marker.done(self.builder, NodeKind::KEY_VALUE_EXPRESSION);
        true
    }

    fn parse_parenthesized_expression(&mut self, is_target: bool) {
        self.assert_current_token(T::LPAR);
        let expression = self.builder.mark();
        self.next_token();
        if self.at_token(T::RPAR) {
            self.next_token();
            expression.done(self.builder, NodeKind::TUPLE_EXPRESSION);
        } else {
            self.parse_tuple_expression(is_target, false);
            if self.at_token(T::FOR_KEYWORD) {
                self.parse_comprehension(expression, Some(T::RPAR), NodeKind::GENERATOR_EXPRESSION);
            } else {
                let error = self.builder.mark();
                let mut empty = true;
                while !self.builder.eof()
                    && !self.at_any_of_tokens(&[Some(T::RPAR), Some(T::LINE_BREAK), Some(T::STATEMENT_BREAK)])
                {
                    self.next_token();
                    empty = false;
                }
                if !empty {
                    error.error(self.builder, message("parser.unexpected.expression.syntax"));
                } else {
                    error.drop(self.builder);
                }
                self.check_matches(T::RPAR, "parser.expected.rpar");
                expression.done(self.builder, NodeKind::PARENTHESIZED_EXPRESSION);
            }
        }
    }

    fn parse_member_expression(&mut self, is_target: bool) -> bool {
        let mut recast_first_identifier = false;
        let mut recast_qualifier = false;
        loop {
            let first_identifier_is_target = is_target && !recast_first_identifier;
            let mut expression = self.builder.mark();
            if !self.parse_primary_expression(first_identifier_is_target) {
                expression.drop(self.builder);
                return false;
            }
            loop {
                match self.builder.get_token_type() {
                    Some(T::DOT) => {
                        if first_identifier_is_target {
                            recast_first_identifier = true;
                            expression.rollback_to(self.builder);
                            break;
                        }
                        self.next_token();
                        self.check_matches(T::IDENTIFIER, "parser.expected.name");
                        if is_target
                            && !recast_qualifier
                            && !self.at_any_of_tokens(&[Some(T::DOT), Some(T::LPAR), Some(T::LBRACKET)])
                        {
                            expression.done(self.builder, NodeKind::TARGET_EXPRESSION);
                        } else {
                            expression.done(self.builder, NodeKind::REFERENCE_EXPRESSION);
                        }
                        expression = expression.precede(self.builder);
                    }
                    Some(T::LPAR) => {
                        self.parse_argument_list();
                        expression.done(self.builder, NodeKind::CALL_EXPRESSION);
                        expression = expression.precede(self.builder);
                    }
                    Some(T::LBRACKET) => {
                        self.next_token();
                        self.parse_slice_or_subscription_expression(expression, false);
                        if is_target && !recast_qualifier {
                            recast_first_identifier = true;
                            recast_qualifier = true;
                            expression.rollback_to(self.builder);
                            break;
                        }
                        expression = expression.precede(self.builder);
                    }
                    _ => {
                        expression.drop(self.builder);
                        break;
                    }
                }
                recast_first_identifier = false;
            }
            if !recast_first_identifier {
                break;
            }
        }
        true
    }

    fn parse_slice_or_subscription_expression(&mut self, expression: Marker, is_slice: bool) {
        let slice_or_tuple_start = self.builder.mark();
        let slice_item_start = self.builder.mark();
        if self.at_token(T::COLON) {
            slice_or_tuple_start.drop(self.builder);
            let slice_marker = self.builder.mark();
            slice_marker.done(self.builder, NodeKind::EMPTY_EXPRESSION);
            self.parse_slice_end(expression, slice_item_start);
        } else {
            let had_expression = self.parse_single_expression(false);
            if self.at_token(T::COLON) {
                if !is_slice {
                    slice_or_tuple_start.rollback_to(self.builder);
                    self.parse_slice_or_subscription_expression(expression, true);
                    return;
                }
                slice_or_tuple_start.drop(self.builder);
                self.parse_slice_end(expression, slice_item_start);
            } else if self.at_token(T::COMMA) {
                slice_item_start.done(self.builder, NodeKind::SLICE_ITEM);
                if !self.parse_slice_list_tail(expression, Some(slice_or_tuple_start)) {
                    slice_or_tuple_start.rollback_to(self.builder);
                    if !self.parse_tuple_expression(false, false) {
                        self.builder.error(message("parser.tuple.expression.expected"));
                    }
                    self.check_matches(T::RBRACKET, "parser.expected.rbracket");
                    expression.done(self.builder, NodeKind::SUBSCRIPTION_EXPRESSION);
                }
            } else {
                if !had_expression {
                    self.builder.error(message("parser.expected.expression"));
                }
                slice_or_tuple_start.drop(self.builder);
                slice_item_start.drop(self.builder);
                self.check_matches(T::RBRACKET, "parser.expected.rbracket");
                expression.done(self.builder, NodeKind::SUBSCRIPTION_EXPRESSION);
            }
        }
    }

    fn parse_slice_end(&mut self, expression_start: Marker, slice_item_start: Marker) {
        self.next_token();
        if self.at_token(T::RBRACKET) {
            let slice_marker = self.builder.mark();
            slice_marker.done(self.builder, NodeKind::EMPTY_EXPRESSION);
            slice_item_start.done(self.builder, NodeKind::SLICE_ITEM);
            self.next_token();
            expression_start.done(self.builder, NodeKind::SLICE_EXPRESSION);
            return;
        }
        if self.at_token(T::COLON) {
            let slice_marker = self.builder.mark();
            slice_marker.done(self.builder, NodeKind::EMPTY_EXPRESSION);
        } else {
            self.parse_single_expression(false);
        }
        if !BRACKET_COLON_COMMA.contains_opt(self.builder.get_token_type()) {
            self.builder.error(message("parser.expected.colon.or.rbracket"));
        }
        if self.match_token(T::COLON) {
            self.parse_single_expression(false);
        }
        slice_item_start.done(self.builder, NodeKind::SLICE_ITEM);
        if !BRACKET_OR_COMMA.contains_opt(self.builder.get_token_type()) {
            self.builder.error(message("parser.rbracket.or.comma.expected"));
        }
        self.parse_slice_list_tail(expression_start, None);
    }

    fn parse_slice_list_tail(&mut self, expression_start: Marker, slice_or_tuple_start: Option<Marker>) -> bool {
        let mut in_slice = slice_or_tuple_start.is_none();
        while self.at_token(T::COMMA) {
            self.next_token();
            let slice_item_start = self.builder.mark();
            self.parse_test_expression(false);
            if self.match_token(T::COLON) {
                in_slice = true;
                self.parse_test_expression(false);
                if self.match_token(T::COLON) {
                    self.parse_test_expression(false);
                }
            }
            slice_item_start.done(self.builder, NodeKind::SLICE_ITEM);
            if !BRACKET_OR_COMMA.contains_opt(self.builder.get_token_type()) {
                self.builder.error(message("parser.rbracket.or.comma.expected"));
                break;
            }
        }
        self.check_matches(T::RBRACKET, "parser.expected.rbracket");
        if in_slice {
            if let Some(slice_or_tuple_start) = slice_or_tuple_start {
                slice_or_tuple_start.drop(self.builder);
            }
            expression_start.done(self.builder, NodeKind::SLICE_EXPRESSION);
        }
        in_slice
    }

    fn parse_argument_list(&mut self) {
        self.assert_current_token(T::LPAR);
        let argument_list = self.builder.mark();
        self.next_token();
        let mut argument_count = 0;
        while !self.at_token(T::RPAR) {
            argument_count += 1;
            if argument_count > 1 {
                if self.match_token(T::COMMA) {
                    if self.at_token(T::RPAR) {
                        break;
                    }
                } else {
                    self.builder.error(message("parser.expected.comma.or.rpar"));
                    break;
                }
            }
            if self.at_any_of_tokens(&[Some(T::MULT), Some(T::EXP)]) {
                let star_argument = self.builder.mark();
                self.next_token();
                if !self.parse_single_expression(false) {
                    self.builder.error(message("parser.expected.expression"));
                }
                star_argument.done(self.builder, NodeKind::STAR_ARGUMENT_EXPRESSION);
            } else {
                if self.is_identifier_like() {
                    let named_argument = self.builder.mark();
                    self.advance_identifier_like();
                    if self.at_token(T::EQ) {
                        self.next_token();
                        if !self.parse_single_expression(false) {
                            self.builder.error(message("parser.expected.expression"));
                        }
                        named_argument.done(self.builder, NodeKind::NAMED_ARGUMENT_EXPRESSION);
                        continue;
                    }
                    named_argument.rollback_to(self.builder);
                }
                let argument = self.builder.mark();
                if !self.parse_single_expression(false) {
                    self.builder.error(message("parser.expected.expression"));
                    argument.rollback_to(self.builder);
                    break;
                }
                argument.done(self.builder, NodeKind::ARGUMENT_EXPRESSION);
            }
        }
        self.check_matches(T::RPAR, "parser.expected.rpar");
        argument_list.done(self.builder, NodeKind::ARGUMENT_LIST);
    }

    pub(crate) fn parse_expression_optional(&mut self) -> bool {
        self.parse_tuple_expression(false, false)
    }

    pub(crate) fn parse_expression(&mut self) {
        if !self.parse_expression_optional() {
            self.builder.error(message("parser.expected.expression"));
        }
    }

    pub(crate) fn parse_expression_target(&mut self, is_target: bool) {
        if !self.parse_tuple_expression(is_target, false) {
            self.builder.error(message("parser.expected.expression"));
        }
    }

    fn parse_tuple_expression(&mut self, is_target: bool, old_test: bool) -> bool {
        let expression = self.builder.mark();
        let mut parsed =
            if old_test { self.parse_old_test_expression(true) } else { self.parse_test_expression(is_target) };
        if !parsed {
            expression.drop(self.builder);
            return false;
        }
        if self.at_token(T::COMMA) {
            while self.at_token(T::COMMA) {
                self.next_token();
                let expression2 = self.builder.mark();
                parsed =
                    if old_test { self.parse_old_test_expression(true) } else { self.parse_test_expression(is_target) };
                if !parsed {
                    expression2.rollback_to(self.builder);
                    break;
                }
                expression2.drop(self.builder);
            }
            expression.done(self.builder, NodeKind::TUPLE_EXPRESSION);
        } else {
            expression.drop(self.builder);
        }
        true
    }

    pub(crate) fn parse_single_expression(&mut self, is_target: bool) -> bool {
        self.parse_test_expression(is_target)
    }

    pub(crate) fn parse_test_expression(&mut self, is_target: bool) -> bool {
        if !self.enter_nesting_level() {
            self.too_deeply_nested();
            return true;
        }
        let parsed = self.parse_test_expression_nested(is_target);
        self.exit_nesting_level();
        parsed
    }

    fn parse_test_expression_nested(&mut self, is_target: bool) -> bool {
        if self.at_token(T::LAMBDA_KEYWORD) {
            return self.parse_lambda_expression(false);
        }
        let conditional_expression = self.builder.mark();
        if !self.parse_or_test_expression(is_target) {
            conditional_expression.drop(self.builder);
            return false;
        }
        if self.at_token(T::IF_KEYWORD) {
            let condition = self.builder.mark();
            self.next_token();
            if !self.parse_or_test_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
            } else if !self.at_token(T::ELSE_KEYWORD) {
                if self.at_token(T::COLON) {
                    condition.rollback_to(self.builder);
                    conditional_expression.drop(self.builder);
                    return true;
                }
                self.builder.error(message("parser.expected.else"));
            } else {
                self.next_token();
                if !self.parse_test_expression(is_target) {
                    self.builder.error(message("parser.expected.expression"));
                }
            }
            condition.drop(self.builder);
            conditional_expression.done(self.builder, NodeKind::CONDITIONAL_EXPRESSION);
        } else {
            conditional_expression.drop(self.builder);
        }
        true
    }

    fn parse_old_test_expression(&mut self, old_test: bool) -> bool {
        if self.at_token(T::LAMBDA_KEYWORD) {
            self.parse_lambda_expression(old_test)
        } else {
            self.parse_or_test_expression(false)
        }
    }

    fn parse_lambda_expression(&mut self, old_test: bool) -> bool {
        let expression = self.builder.mark();
        self.next_token();
        self.parse_parameter_list_contents(T::COLON, false);
        let parsed = if old_test { self.parse_old_test_expression(true) } else { self.parse_single_expression(false) };
        if !parsed {
            self.builder.error(message("parser.expected.expression"));
        }
        expression.done(self.builder, NodeKind::LAMBDA_EXPRESSION);
        true
    }

    fn parse_or_test_expression(&mut self, is_target: bool) -> bool {
        let mut expression = self.builder.mark();
        if !self.parse_and_test_expression(is_target) {
            expression.drop(self.builder);
            return false;
        }
        while self.at_token(T::OR_KEYWORD) {
            self.next_token();
            if !self.parse_and_test_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::BINARY_EXPRESSION);
            expression = expression.precede(self.builder);
        }
        expression.drop(self.builder);
        true
    }

    fn parse_and_test_expression(&mut self, is_target: bool) -> bool {
        let mut expression = self.builder.mark();
        if !self.parse_comparison_expression(is_target) {
            expression.drop(self.builder);
            return false;
        }
        while self.at_token(T::AND_KEYWORD) {
            self.next_token();
            if !self.parse_comparison_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::BINARY_EXPRESSION);
            expression = expression.precede(self.builder);
        }
        expression.drop(self.builder);
        true
    }

    fn parse_comparison_expression(&mut self, is_target: bool) -> bool {
        let mut expression = self.builder.mark();
        if !self.parse_star_expression(is_target) {
            expression.drop(self.builder);
            return false;
        }
        while COMPARISON_OPERATIONS.contains_opt(self.builder.get_token_type()) {
            if self.at_token(T::NOT_KEYWORD) {
                let not_marker = self.builder.mark();
                self.next_token();
                if !self.at_token(T::IN_KEYWORD) {
                    not_marker.rollback_to(self.builder);
                    break;
                }
                not_marker.drop(self.builder);
                self.next_token();
            } else if self.at_token(T::IS_KEYWORD) {
                self.next_token();
                self.match_token(T::NOT_KEYWORD);
            } else {
                self.next_token();
            }
            if !self.parse_bitwise_or_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::BINARY_EXPRESSION);
            expression = expression.precede(self.builder);
        }
        expression.drop(self.builder);
        true
    }

    fn parse_star_expression(&mut self, is_target: bool) -> bool {
        if self.at_token(T::MULT) {
            let star_expression = self.builder.mark();
            self.next_token();
            if !self.parse_bitwise_or_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
                star_expression.drop(self.builder);
                return false;
            }
            star_expression.done(self.builder, NodeKind::STAR_EXPRESSION);
            return true;
        }
        self.parse_bitwise_or_expression(is_target)
    }

    fn parse_double_star_expression(&mut self) -> bool {
        if self.at_token(T::EXP) {
            let star_expression = self.builder.mark();
            self.next_token();
            if !self.parse_bitwise_or_expression(false) {
                self.builder.error(message("parser.expected.expression"));
                star_expression.drop(self.builder);
                return false;
            }
            star_expression.done(self.builder, NodeKind::DOUBLE_STAR_EXPRESSION);
            return true;
        }
        self.parse_bitwise_or_expression(false)
    }

    fn parse_bitwise_or_expression(&mut self, is_target: bool) -> bool {
        self.parse_binary_expression(is_target, T::OR, Self::parse_bitwise_xor_expression)
    }

    fn parse_bitwise_xor_expression(&mut self, is_target: bool) -> bool {
        self.parse_binary_expression(is_target, T::XOR, Self::parse_bitwise_and_expression)
    }

    fn parse_bitwise_and_expression(&mut self, is_target: bool) -> bool {
        self.parse_binary_expression(is_target, T::AND, Self::parse_shift_expression)
    }

    fn parse_shift_expression(&mut self, is_target: bool) -> bool {
        self.parse_binary_set_expression(is_target, SHIFT_OPERATIONS, Self::parse_additive_expression)
    }

    fn parse_additive_expression(&mut self, is_target: bool) -> bool {
        self.parse_binary_set_expression(is_target, ADDITIVE_OPERATIONS, Self::parse_multiplicative_expression)
    }

    fn parse_multiplicative_expression(&mut self, is_target: bool) -> bool {
        self.parse_binary_set_expression(is_target, MULTIPLICATIVE_OPERATIONS, Self::parse_unary_expression)
    }

    fn parse_binary_expression(
        &mut self,
        is_target: bool,
        operation: T,
        parse_operand: fn(&mut Self, bool) -> bool,
    ) -> bool {
        let mut expression = self.builder.mark();
        if !parse_operand(self, is_target) {
            expression.drop(self.builder);
            return false;
        }
        while self.at_token(operation) {
            self.next_token();
            if !parse_operand(self, is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::BINARY_EXPRESSION);
            expression = expression.precede(self.builder);
        }
        expression.drop(self.builder);
        true
    }

    fn parse_binary_set_expression(
        &mut self,
        is_target: bool,
        operations: crate::tokens::TokenSet,
        parse_operand: fn(&mut Self, bool) -> bool,
    ) -> bool {
        let mut expression = self.builder.mark();
        if !parse_operand(self, is_target) {
            expression.drop(self.builder);
            return false;
        }
        while operations.contains_opt(self.builder.get_token_type()) {
            self.next_token();
            if !parse_operand(self, is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::BINARY_EXPRESSION);
            expression = expression.precede(self.builder);
        }
        expression.drop(self.builder);
        true
    }

    fn parse_unary_expression(&mut self, is_target: bool) -> bool {
        if !self.enter_nesting_level() {
            self.too_deeply_nested();
            return true;
        }
        let parsed = self.parse_unary_expression_nested(is_target);
        self.exit_nesting_level();
        parsed
    }

    fn parse_unary_expression_nested(&mut self, is_target: bool) -> bool {
        if UNARY_OPERATIONS.contains_opt(self.builder.get_token_type()) {
            let expression = self.builder.mark();
            self.next_token();
            if !self.parse_unary_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::PREFIX_EXPRESSION);
            true
        } else {
            self.parse_power_expression(is_target)
        }
    }

    fn parse_power_expression(&mut self, is_target: bool) -> bool {
        if !self.enter_nesting_level() {
            self.too_deeply_nested();
            return true;
        }
        let parsed = self.parse_power_expression_nested(is_target);
        self.exit_nesting_level();
        parsed
    }

    fn parse_power_expression_nested(&mut self, is_target: bool) -> bool {
        let expression = self.builder.mark();
        if !self.parse_member_expression(is_target) {
            expression.drop(self.builder);
            return false;
        }
        if self.at_token(T::EXP) {
            self.next_token();
            if !self.parse_unary_expression(is_target) {
                self.builder.error(message("parser.expected.expression"));
            }
            expression.done(self.builder, NodeKind::BINARY_EXPRESSION);
        } else {
            expression.drop(self.builder);
        }
        true
    }

    fn get_target_or_reference_expression(&mut self, is_target: bool) -> NodeKind {
        let token_text = self.builder.get_token_text().unwrap_or("");
        if is_target && token_text != "_" {
            NodeKind::TARGET_EXPRESSION
        } else if token_text == "glob" {
            NodeKind::GLOB_EXPRESSION
        } else {
            NodeKind::REFERENCE_EXPRESSION
        }
    }
}
