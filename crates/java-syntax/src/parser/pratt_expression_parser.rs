//! `PrattExpressionParser.kt`: expressions, by precedence climbing over the infix operator table.

use psi_builder::Marker;

use crate::builder::{Binder, SyntaxTreeBuilder};
use crate::bundle::message;
use crate::node_kind::NodeKind;
use crate::parser::expression_parser::ExpressionParser;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{done, empty_element, error, expect_or_error, expr_type};
use crate::parser::reference_parser::ReferenceParser;
use crate::syntax_element_types::{ALL_LITERALS, MODIFIER_BIT_SET, PRIMITIVE_TYPE_BIT_SET};
use crate::tokens::{TokenKind, TokenSet};

/// `PrattExpressionParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::expression_parser`.
#[derive(Copy, Clone)]
pub struct PrattExpressionParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> PrattExpressionParser<'p> {
    /// `parse(builder)` (PrattExpressionParser.kt:33).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.try_parse_with_precedence_at_most(builder, ASSIGNMENT_PRECEDENCE, 0)
    }

    /// `parse(builder, mode)` (PrattExpressionParser.kt:37).
    pub fn parse_with(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        self.try_parse_with_precedence_at_most(builder, ASSIGNMENT_PRECEDENCE, mode)
    }

    /// `tryParseWithPrecedenceAtMost(builder, maxPrecedence, mode)` (PrattExpressionParser.kt:45): the traditional
    /// Pratt parser for infix expressions. If the marker is `None`, nothing is parsed.
    pub fn try_parse_with_precedence_at_most(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        max_precedence: i32,
        mode: i32,
    ) -> Option<Marker> {
        let mut lhs = self.parse_unary(builder, mode)?;

        while let Some(element_type) = self.get_bin_op_token(builder) {
            let Some(data) = infix_parsers(element_type) else { break };
            let op_precedence = data.my_precedence;
            if max_precedence < op_precedence {
                break;
            }
            let before_lhs = lhs.precede(builder);
            data.my_parser.parse(self, builder, before_lhs, Some(element_type), op_precedence, mode);
            lhs = before_lhs;
        }
        Some(lhs)
    }

    /// `parseUnary(builder, mode)` (PrattExpressionParser.kt:62).
    fn parse_unary(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        let token_type = builder.token_type();

        if PREFIX_OPS.contains(token_type) {
            let unary = builder.mark();
            builder.advance_lexer();

            let operand = self.parse_unary(builder, mode);
            if operand.is_none() {
                error(builder, message("expected.expression"));
            }

            unary.done(builder, NodeKind::PREFIX_EXPRESSION);
            Some(unary)
        } else if token_type == Some(TokenKind::LPARENTH) {
            let type_cast = builder.mark();
            builder.advance_lexer();

            let type_info = self.my_parser.reference_parser().parse_type_info(
                builder,
                ReferenceParser::EAT_LAST_DOT
                    | ReferenceParser::WILDCARD
                    | ReferenceParser::CONJUNCTIONS
                    | ReferenceParser::INCOMPLETE_ANNO,
            );
            let Some(type_info) = type_info.filter(|_| builder.expect(Some(TokenKind::RPARENTH))) else {
                type_cast.rollback_to(builder);
                return self.parse_postfix(builder, mode);
            };

            if PREF_ARITHMETIC_OPS.contains(builder.token_type()) && !type_info.is_primitive {
                type_cast.rollback_to(builder);
                return self.parse_postfix(builder, mode);
            }

            let expr = self.parse_unary(builder, mode);
            if expr.is_none() {
                if !type_info.is_parameterized {
                    // cannot parse correct parenthesized expression after correct parameterized type
                    type_cast.rollback_to(builder);
                    return self.parse_postfix(builder, mode);
                } else {
                    error(builder, message("expected.expression"));
                }
            }

            type_cast.done(builder, NodeKind::TYPE_CAST_EXPRESSION);
            Some(type_cast)
        } else if token_type == Some(TokenKind::SWITCH_KEYWORD) {
            Some(self.my_parser.statement_parser().parse_expr_in_parenth_with_block(
                builder,
                NodeKind::SWITCH_EXPRESSION,
                true,
            ))
        } else {
            self.parse_postfix(builder, mode)
        }
    }

    /// `parsePostfix(builder, mode)` (PrattExpressionParser.kt:118).
    fn parse_postfix(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        let mut operand = self.parse_primary(builder, None, -1, mode)?;

        while POSTFIX_OPS.contains(builder.token_type()) {
            let postfix = operand.precede(builder);
            builder.advance_lexer();
            postfix.done(builder, NodeKind::POSTFIX_EXPRESSION);
            operand = postfix;
        }

        Some(operand)
    }

    /// `parsePrimary(builder, breakPoint, breakOffset, mode)` (PrattExpressionParser.kt:131).
    fn parse_primary(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        break_point: Option<BreakPoint>,
        break_offset: i32,
        mode: i32,
    ) -> Option<Marker> {
        let mut start_marker = builder.mark();

        let Some(mut expr) = self.parse_primary_expression_start(builder, mode) else {
            start_marker.drop(builder);
            return None;
        };

        loop {
            let token_type = builder.token_type();
            if token_type == Some(TokenKind::DOT) {
                let dot_pos = builder.mark();
                let dot_offset = builder.current_offset();
                builder.advance_lexer();

                let mut dot_token_type = builder.token_type();
                if dot_token_type == Some(TokenKind::AT) {
                    self.my_parser.declaration_parser().parse_annotations(builder);
                    dot_token_type = builder.token_type();
                }

                if dot_token_type == Some(TokenKind::CLASS_KEYWORD)
                    && expr_type(builder, Some(expr)) == NodeKind::REFERENCE_EXPRESSION
                {
                    if break_point == Some(BreakPoint::P1) && builder.current_offset() == break_offset {
                        error(builder, message("expected.identifier"));
                        crate::builder::drop(builder, &[Some(start_marker), Some(dot_pos)]);
                        return Some(expr);
                    }

                    let copy = start_marker.precede(builder);
                    let offset = builder.current_offset();
                    start_marker.rollback_to(builder);

                    let class_obj_access = self.parse_class_access_or_method_reference(builder);
                    let Some(class_obj_access) = class_obj_access.filter(|_| builder.current_offset() >= offset) else {
                        copy.rollback_to(builder);
                        return self.parse_primary(builder, Some(BreakPoint::P1), offset, mode);
                    };

                    start_marker = copy;
                    expr = class_obj_access;
                } else if dot_token_type == Some(TokenKind::NEW_KEYWORD) {
                    dot_pos.drop(builder);
                    expr = self.parse_new(builder, Some(expr));
                } else if dot_token_type == Some(TokenKind::SUPER_KEYWORD)
                    && builder.look_ahead(1) == Some(TokenKind::LPARENTH)
                {
                    dot_pos.drop(builder);
                    let ref_expr = expr.precede(builder);
                    builder.mark().done(builder, NodeKind::REFERENCE_PARAMETER_LIST);
                    builder.advance_lexer();
                    ref_expr.done(builder, NodeKind::REFERENCE_EXPRESSION);
                    expr = ref_expr;
                } else if dot_token_type == Some(TokenKind::STRING_TEMPLATE_BEGIN)
                    || dot_token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_BEGIN)
                {
                    dot_pos.drop(builder);
                    expr = self.parse_string_template(
                        builder,
                        Some(expr),
                        dot_token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_BEGIN),
                    );
                } else if dot_token_type == Some(TokenKind::STRING_LITERAL)
                    || dot_token_type == Some(TokenKind::TEXT_BLOCK_LITERAL)
                {
                    dot_pos.drop(builder);
                    let template_expression = expr.precede(builder);
                    let literal = builder.mark();
                    builder.advance_lexer();
                    literal.done(builder, NodeKind::LITERAL_EXPRESSION);
                    template_expression.done(builder, NodeKind::TEMPLATE_EXPRESSION);
                    expr = template_expression;
                } else if THIS_OR_SUPER.contains(dot_token_type)
                    && expr_type(builder, Some(expr)) == NodeKind::REFERENCE_EXPRESSION
                {
                    if break_point == Some(BreakPoint::P2) && builder.current_offset() == break_offset {
                        dot_pos.rollback_to(builder);
                        start_marker.drop(builder);
                        return Some(expr);
                    }

                    let copy = start_marker.precede(builder);
                    let offset = builder.current_offset();
                    start_marker.rollback_to(builder);

                    let reference =
                        self.my_parser.reference_parser().parse_java_code_reference(builder, false, true, false, false);
                    let reference = reference.filter(|_| {
                        builder.token_type() == Some(TokenKind::DOT) && builder.current_offset() == dot_offset
                    });
                    let Some(reference) = reference else {
                        copy.rollback_to(builder);
                        return self.parse_primary(builder, Some(BreakPoint::P2), offset, mode);
                    };
                    builder.advance_lexer();

                    if builder.token_type() != dot_token_type {
                        copy.rollback_to(builder);
                        return self.parse_primary(builder, Some(BreakPoint::P2), offset, mode);
                    }
                    builder.advance_lexer();

                    start_marker = copy;
                    expr = reference.precede(builder);
                    expr.done(
                        builder,
                        if dot_token_type == Some(TokenKind::THIS_KEYWORD) {
                            NodeKind::THIS_EXPRESSION
                        } else {
                            NodeKind::SUPER_EXPRESSION
                        },
                    );
                } else {
                    let ref_expr = expr.precede(builder);

                    self.my_parser.reference_parser().parse_reference_parameter_list(builder, false, false);

                    if !builder.expect_set(ID_OR_SUPER) {
                        dot_pos.rollback_to(builder);
                        builder.advance_lexer();
                        self.my_parser.reference_parser().parse_reference_parameter_list(builder, false, false);
                        error(builder, message("expected.identifier"));
                        ref_expr.done(builder, NodeKind::REFERENCE_EXPRESSION);
                        start_marker.drop(builder);
                        return Some(ref_expr);
                    }

                    dot_pos.drop(builder);
                    ref_expr.done(builder, NodeKind::REFERENCE_EXPRESSION);
                    expr = ref_expr;
                }
            } else if token_type == Some(TokenKind::LPARENTH) {
                if expr_type(builder, Some(expr)) != NodeKind::REFERENCE_EXPRESSION {
                    start_marker.drop(builder);
                    return Some(expr);
                }

                let call_expr = expr.precede(builder);
                self.parse_argument_list(builder);
                call_expr.done(builder, NodeKind::METHOD_CALL_EXPRESSION);
                expr = call_expr;
            } else if token_type == Some(TokenKind::LBRACKET) {
                if break_point == Some(BreakPoint::P4) {
                    start_marker.drop(builder);
                    return Some(expr);
                }

                builder.advance_lexer();

                if builder.token_type() == Some(TokenKind::RBRACKET)
                    && expr_type(builder, Some(expr)) == NodeKind::REFERENCE_EXPRESSION
                {
                    let pos = builder.current_offset();
                    let copy = start_marker.precede(builder);
                    start_marker.rollback_to(builder);

                    let class_obj_access = self.parse_class_access_or_method_reference(builder);
                    let Some(class_obj_access) = class_obj_access.filter(|_| builder.current_offset() > pos) else {
                        copy.rollback_to(builder);
                        return self.parse_primary(builder, Some(BreakPoint::P4), -1, mode);
                    };

                    start_marker = copy;
                    expr = class_obj_access;
                } else {
                    let array_access = expr.precede(builder);

                    let index = self.parse_with(builder, mode);
                    if index.is_none() {
                        error(builder, message("expected.expression"));
                        array_access.done(builder, NodeKind::ARRAY_ACCESS_EXPRESSION);
                        start_marker.drop(builder);
                        return Some(array_access);
                    }

                    if builder.token_type() != Some(TokenKind::RBRACKET) {
                        error(builder, message("expected.rbracket"));
                        array_access.done(builder, NodeKind::ARRAY_ACCESS_EXPRESSION);
                        start_marker.drop(builder);
                        return Some(array_access);
                    }
                    builder.advance_lexer();

                    array_access.done(builder, NodeKind::ARRAY_ACCESS_EXPRESSION);
                    expr = array_access;
                }
            } else if token_type == Some(TokenKind::DOUBLE_COLON) {
                return Some(self.parse_method_reference(builder, start_marker));
            } else {
                start_marker.drop(builder);
                return Some(expr);
            }
        }
    }

    /// `parsePrimaryExpressionStart(builder, mode)` (PrattExpressionParser.kt:316).
    fn parse_primary_expression_start(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        let mut token_type = builder.token_type();

        if token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_BEGIN)
            || token_type == Some(TokenKind::STRING_TEMPLATE_BEGIN)
        {
            return Some(self.parse_string_template(
                builder,
                None,
                token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_BEGIN),
            ));
        }

        if ALL_LITERALS.contains(token_type) {
            let literal = builder.mark();
            builder.advance_lexer();
            literal.done(builder, NodeKind::LITERAL_EXPRESSION);
            return Some(literal);
        }

        if token_type == Some(TokenKind::LBRACE) {
            return Some(self.parse_array_initializer(builder));
        }

        if token_type == Some(TokenKind::NEW_KEYWORD) {
            return Some(self.parse_new(builder, None));
        }

        if token_type == Some(TokenKind::LPARENTH) {
            if mode & ExpressionParser::FORBID_LAMBDA_MASK == 0 {
                let lambda = self.parse_lambda_after_parenth(builder);
                if lambda.is_some() {
                    return lambda;
                }
            }

            let parenth = builder.mark();
            builder.advance_lexer();

            let inner = self.parse_with(builder, mode);
            if inner.is_none() {
                error(builder, message("expected.expression"));
            }

            if !builder.expect(Some(TokenKind::RPARENTH)) && inner.is_some() {
                error(builder, message("expected.rparen"));
            }

            parenth.done(builder, NodeKind::PARENTH_EXPRESSION);
            return Some(parenth);
        }

        if TYPE_START.contains(token_type) {
            let mark = builder.mark();

            let type_info = self.my_parser.reference_parser().parse_type_info(builder, 0);
            if let Some(type_info) = type_info {
                let optional_class_keyword = type_info.is_primitive || type_info.is_array;
                if optional_class_keyword || !type_info.has_errors && type_info.is_parameterized {
                    let result =
                        self.parse_class_access_or_method_reference_with(builder, mark, optional_class_keyword);
                    if result.is_some() {
                        return result;
                    }
                }
            }

            mark.rollback_to(builder);
        }

        let mut annotation: Option<Marker> = None;
        if token_type == Some(TokenKind::AT) {
            annotation = self.my_parser.declaration_parser().parse_annotations(builder);
            token_type = builder.token_type();
        }

        if token_type == Some(TokenKind::VAR_KEYWORD) {
            token_type = Some(TokenKind::IDENTIFIER);
            builder.remap_current_token(TokenKind::IDENTIFIER);
        }
        if token_type == Some(TokenKind::IDENTIFIER) {
            if mode & ExpressionParser::FORBID_LAMBDA_MASK == 0 && builder.look_ahead(1) == Some(TokenKind::ARROW) {
                return self.parse_lambda_expression(builder, false);
            }

            let ref_expr: Marker;
            if let Some(annotation) = annotation {
                let ref_param = annotation.precede(builder);
                ref_param.done_before(builder, NodeKind::REFERENCE_PARAMETER_LIST, annotation);
                ref_expr = ref_param.precede(builder);
            } else {
                ref_expr = builder.mark();
                builder.mark().done(builder, NodeKind::REFERENCE_PARAMETER_LIST);
            }

            builder.advance_lexer();
            ref_expr.done(builder, NodeKind::REFERENCE_EXPRESSION);
            return Some(ref_expr);
        }

        if let Some(annotation) = annotation {
            annotation.rollback_to(builder);
            token_type = builder.token_type();
        }

        let mut expr: Option<Marker> = None;
        if token_type == Some(TokenKind::LT) {
            let mark = builder.mark();
            expr = Some(mark);

            if !self.my_parser.reference_parser().parse_reference_parameter_list(builder, false, false) {
                mark.rollback_to(builder);
                return None;
            }

            token_type = builder.token_type();
            if !THIS_OR_SUPER.contains(token_type) {
                mark.rollback_to(builder);
                return None;
            }
        }

        if THIS_OR_SUPER.contains(token_type) {
            let expr = match expr {
                Some(expr) => expr,
                None => {
                    let expr = builder.mark();
                    builder.mark().done(builder, NodeKind::REFERENCE_PARAMETER_LIST);
                    expr
                }
            };
            builder.advance_lexer();
            let element_type = if builder.token_type() == Some(TokenKind::LPARENTH) {
                NodeKind::REFERENCE_EXPRESSION
            } else if token_type == Some(TokenKind::THIS_KEYWORD) {
                NodeKind::THIS_EXPRESSION
            } else {
                NodeKind::SUPER_EXPRESSION
            };
            expr.done(builder, element_type);
            return Some(expr);
        }

        None
    }

    /// `parseClassAccessOrMethodReference(builder)` (PrattExpressionParser.kt:449).
    fn parse_class_access_or_method_reference(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let expr = builder.mark();

        let primitive = PRIMITIVE_TYPE_BIT_SET.contains(builder.token_type());
        if self.my_parser.reference_parser().parse_type(builder, 0).is_none() {
            expr.drop(builder);
            return None;
        }

        let result = self.parse_class_access_or_method_reference_with(builder, expr, primitive);
        if result.is_none() {
            expr.rollback_to(builder);
        }
        result
    }

    /// `parseClassAccessOrMethodReference(builder, expr, optionalClassKeyword)` (PrattExpressionParser.kt:463).
    fn parse_class_access_or_method_reference_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        expr: Marker,
        optional_class_keyword: bool,
    ) -> Option<Marker> {
        let token_type = builder.token_type();
        if token_type == Some(TokenKind::DOT) {
            return self.parse_class_object_access(builder, expr, optional_class_keyword);
        } else if token_type == Some(TokenKind::DOUBLE_COLON) {
            return Some(self.parse_method_reference(builder, expr));
        }

        None
    }

    /// `parseMethodReference(builder, start)` (PrattExpressionParser.kt:479).
    fn parse_method_reference(&self, builder: &mut SyntaxTreeBuilder<'_>, start: Marker) -> Marker {
        builder.advance_lexer();

        self.my_parser.reference_parser().parse_reference_parameter_list(builder, false, false);

        if !builder.expect(Some(TokenKind::IDENTIFIER)) && !builder.expect(Some(TokenKind::NEW_KEYWORD)) {
            error(builder, message("expected.identifier"));
        }

        start.done(builder, NodeKind::METHOD_REF_EXPRESSION);
        start
    }

    /// `parseStringTemplate(builder, start, textBlock)` (PrattExpressionParser.kt:493).
    fn parse_string_template(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        start: Option<Marker>,
        text_block: bool,
    ) -> Marker {
        let template_expression = match start {
            Some(start) => start.precede(builder),
            None => builder.mark(),
        };
        let template = builder.mark();
        let mut token_type;
        loop {
            builder.advance_lexer();
            token_type = builder.token_type();
            if if text_block {
                token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_MID)
                    || token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_END)
            } else {
                token_type == Some(TokenKind::STRING_TEMPLATE_MID) || token_type == Some(TokenKind::STRING_TEMPLATE_END)
            } {
                self.empty_expression(builder);
            } else {
                self.parse(builder);
                token_type = builder.token_type();
            }
            let more = if text_block {
                token_type == Some(TokenKind::TEXT_BLOCK_TEMPLATE_MID)
            } else {
                token_type == Some(TokenKind::STRING_TEMPLATE_MID)
            };
            if !more {
                break;
            }
        }
        if if text_block {
            token_type != Some(TokenKind::TEXT_BLOCK_TEMPLATE_END)
        } else {
            token_type != Some(TokenKind::STRING_TEMPLATE_END)
        } {
            builder.error(message("expected.template.fragment"));
        } else {
            builder.advance_lexer();
        }
        template.done(builder, NodeKind::TEMPLATE);
        template_expression.done(builder, NodeKind::TEMPLATE_EXPRESSION);
        template_expression
    }

    /// `parseNew(builder, start)` (PrattExpressionParser.kt:528).
    fn parse_new(&self, builder: &mut SyntaxTreeBuilder<'_>, start: Option<Marker>) -> Marker {
        let new_expr = match start {
            Some(start) => start.precede(builder),
            None => builder.mark(),
        };
        builder.advance_lexer();

        self.my_parser.reference_parser().parse_reference_parameter_list(builder, false, true);

        let ref_or_type: Option<Marker>;
        let mut anno = self.my_parser.declaration_parser().parse_annotations(builder);
        let token_type = builder.token_type();
        if token_type == Some(TokenKind::IDENTIFIER) {
            if let Some(anno) = anno {
                anno.rollback_to(builder);
            }
            ref_or_type = self.my_parser.reference_parser().parse_java_code_reference(builder, true, true, true, true);
            if ref_or_type.is_none() {
                error(builder, message("expected.identifier"));
                new_expr.done(builder, NodeKind::NEW_EXPRESSION);
                return new_expr;
            }
        } else if PRIMITIVE_TYPE_BIT_SET.contains(token_type) {
            ref_or_type = None;
            builder.advance_lexer();
        } else {
            error(builder, message("expected.identifier"));
            new_expr.done(builder, NodeKind::NEW_EXPRESSION);
            return new_expr;
        }

        if let Some(ref_or_type) = ref_or_type
            && builder.token_type() == Some(TokenKind::LPARENTH)
        {
            self.parse_argument_list(builder);
            if builder.token_type() == Some(TokenKind::LBRACE) {
                let class_element = ref_or_type.precede(builder);
                self.my_parser.declaration_parser().parse_class_body_with_braces(builder, false, false);
                class_element.done(builder, NodeKind::ANONYMOUS_CLASS);
            }
            new_expr.done(builder, NodeKind::NEW_EXPRESSION);
            return new_expr;
        }

        anno = self.my_parser.declaration_parser().parse_annotations(builder);

        if builder.token_type() != Some(TokenKind::LBRACKET) {
            if let Some(anno) = anno {
                anno.rollback_to(builder);
            }
            error(
                builder,
                message(if ref_or_type.is_none() { "expected.lbracket" } else { "expected.lparen.or.lbracket" }),
            );
            new_expr.done(builder, NodeKind::NEW_EXPRESSION);
            return new_expr;
        }

        let mut bracket_count = 0;
        let mut dim_count = 0;
        loop {
            anno = self.my_parser.declaration_parser().parse_annotations(builder);

            if builder.token_type() != Some(TokenKind::LBRACKET) {
                if let Some(anno) = anno {
                    anno.rollback_to(builder);
                }
                break;
            }
            builder.advance_lexer();

            if bracket_count == dim_count {
                let dim_expr = self.parse_with(builder, 0);
                if dim_expr.is_some() {
                    dim_count += 1;
                }
            }
            bracket_count += 1;

            if !expect_or_error(builder, Some(TokenKind::RBRACKET), "expected.rbracket") {
                new_expr.done(builder, NodeKind::NEW_EXPRESSION);
                return new_expr;
            }
        }

        if dim_count == 0 {
            if builder.token_type() == Some(TokenKind::LBRACE) {
                self.parse_array_initializer(builder);
            } else {
                error(builder, message("expected.array.initializer"));
            }
        }

        new_expr.done(builder, NodeKind::NEW_EXPRESSION);
        new_expr
    }

    /// `parseArrayInitializer(builder)` (PrattExpressionParser.kt:614).
    fn parse_array_initializer(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_array_initializer_with(
            builder,
            NodeKind::ARRAY_INITIALIZER_EXPRESSION,
            &|builder: &mut SyntaxTreeBuilder<'_>| self.parse(builder),
            "expected.expression",
        )
    }

    /// `parseArrayInitializer(builder, type, elementParser, missingElementKey)` (PrattExpressionParser.kt:621).
    pub fn parse_array_initializer_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        element_type: NodeKind,
        element_parser: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> Option<Marker>,
        missing_element_key: &'static str,
    ) -> Marker {
        let array_init = builder.mark();
        builder.advance_lexer();

        let mut first = true;
        loop {
            if builder.token_type() == Some(TokenKind::RBRACE) {
                builder.advance_lexer();
                break;
            }

            if builder.token_type().is_none() {
                error(builder, message("expected.rbrace"));
                break;
            }

            if element_parser(builder).is_none() {
                if builder.token_type() == Some(TokenKind::COMMA) {
                    if first && builder.look_ahead(1) == Some(TokenKind::RBRACE) {
                        builder.advance(2);
                        break;
                    }
                    builder.error(message(missing_element_key));
                } else if builder.token_type() != Some(TokenKind::RBRACE) {
                    error(builder, message("expected.rbrace"));
                    break;
                }
            }

            first = false;

            let token_type = builder.token_type();
            if !builder.expect(Some(TokenKind::COMMA)) && token_type != Some(TokenKind::RBRACE) {
                error(builder, message("expected.comma"));
            }
        }

        array_init.done(builder, element_type);
        array_init
    }

    /// `parseArgumentList(builder)` (PrattExpressionParser.kt:668).
    pub fn parse_argument_list(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let list = builder.mark();
        builder.advance_lexer();

        let mut first = true;
        loop {
            let token_type = builder.token_type();
            if first && (ARGS_LIST_END.contains(token_type) || builder.eof()) {
                break;
            }
            if !first && !ARGS_LIST_CONTINUE.contains(token_type) {
                break;
            }

            let mut has_error = false;
            if !first {
                if builder.token_type() == Some(TokenKind::COMMA) {
                    builder.advance_lexer();
                } else {
                    has_error = true;
                    error(builder, message("expected.comma.or.rparen"));
                    self.empty_expression(builder);
                }
            }
            first = false;

            let arg = self.parse_with(builder, 0);
            if arg.is_none() {
                if !has_error {
                    error(builder, message("expected.expression"));
                    self.empty_expression(builder);
                }
                if !ARGS_LIST_CONTINUE.contains(builder.token_type()) {
                    break;
                }
                if builder.token_type() != Some(TokenKind::COMMA) && !builder.eof() {
                    builder.advance_lexer();
                }
            }
        }

        let mut closed = true;
        if !builder.expect(Some(TokenKind::RPARENTH)) {
            if first {
                error(builder, message("expected.rparen"));
            } else {
                error(builder, message("expected.comma.or.rparen"));
            }
            closed = false;
        }

        list.done(builder, NodeKind::EXPRESSION_LIST);
        if !closed {
            list.set_custom_edge_token_binders(builder, None, Some(Binder::DefaultLeft));
        }
        list
    }

    /// `parseLambdaAfterParenth(builder)` (PrattExpressionParser.kt:722).
    fn parse_lambda_after_parenth(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let is_lambda: bool;
        let is_typed: bool;

        let next_token1 = builder.look_ahead(1);
        let next_token2 = builder.look_ahead(2);
        if next_token1 == Some(TokenKind::RPARENTH) && next_token2 == Some(TokenKind::ARROW) {
            is_lambda = true;
            is_typed = false;
        } else if next_token1 == Some(TokenKind::AT)
            || MODIFIER_BIT_SET.contains(next_token1)
            || PRIMITIVE_TYPE_BIT_SET.contains(next_token1)
        {
            is_lambda = true;
            is_typed = true;
        } else if next_token1 == Some(TokenKind::IDENTIFIER) {
            if next_token2 == Some(TokenKind::COMMA)
                || next_token2 == Some(TokenKind::RPARENTH) && builder.look_ahead(3) == Some(TokenKind::ARROW)
            {
                is_lambda = true;
                is_typed = false;
            } else if next_token2 == Some(TokenKind::ARROW) {
                is_lambda = false;
                is_typed = false;
            } else {
                let mut lambda = false;

                let marker = builder.mark();
                builder.advance_lexer();
                let type_info = self
                    .my_parser
                    .reference_parser()
                    .parse_type_info(builder, ReferenceParser::ELLIPSIS | ReferenceParser::WILDCARD);
                if type_info.is_some() {
                    let t = builder.token_type();
                    lambda = t == Some(TokenKind::IDENTIFIER)
                        || t == Some(TokenKind::THIS_KEYWORD)
                        || t == Some(TokenKind::RPARENTH) && builder.look_ahead(1) == Some(TokenKind::ARROW);
                }
                marker.rollback_to(builder);

                is_lambda = lambda;
                is_typed = true;
            }
        } else {
            is_lambda = false;
            is_typed = false;
        }

        if is_lambda { self.parse_lambda_expression(builder, is_typed) } else { None }
    }

    /// `parseLambdaExpression(builder, typed)` (PrattExpressionParser.kt:777).
    fn parse_lambda_expression(&self, builder: &mut SyntaxTreeBuilder<'_>, typed: bool) -> Option<Marker> {
        let start = builder.mark();

        self.my_parser.declaration_parser().parse_lambda_parameter_list(builder, typed);

        if !builder.expect(Some(TokenKind::ARROW)) {
            start.rollback_to(builder);
            return None;
        }

        let body = if builder.token_type() == Some(TokenKind::LBRACE) {
            self.my_parser.statement_parser().parse_code_block(builder)
        } else {
            self.parse_with(builder, 0)
        };

        if body.is_none() {
            builder.error(message("expected.lbrace"));
        }

        start.done(builder, NodeKind::LAMBDA_EXPRESSION);
        Some(start)
    }

    /// `getBinOpToken(builder)` (PrattExpressionParser.kt:930).
    fn get_bin_op_token(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<TokenKind> {
        let token_type = builder.token_type();
        if token_type != Some(TokenKind::GT) {
            return token_type;
        }

        if builder.raw_lookup(1) == Some(TokenKind::GT) {
            if builder.raw_lookup(2) == Some(TokenKind::GT) {
                if builder.raw_lookup(3) == Some(TokenKind::EQ) {
                    return Some(TokenKind::GTGTGTEQ);
                }
                return Some(TokenKind::GTGTGT);
            }
            if builder.raw_lookup(2) == Some(TokenKind::EQ) {
                return Some(TokenKind::GTGTEQ);
            }
            return Some(TokenKind::GTGT);
        } else if builder.raw_lookup(1) == Some(TokenKind::EQ) {
            return Some(TokenKind::GE);
        }

        token_type
    }

    /// `advanceBinOpToken(builder, type)` (PrattExpressionParser.kt:953).
    fn advance_bin_op_token(&self, builder: &mut SyntaxTreeBuilder<'_>, element_type: Option<TokenKind>) {
        let gt_token = builder.mark();

        let collapsed = match element_type {
            Some(TokenKind::GTGTGTEQ) => {
                builder.advance(4);
                TokenKind::GTGTGTEQ
            }
            Some(t @ (TokenKind::GTGTGT | TokenKind::GTGTEQ)) => {
                builder.advance(3);
                t
            }
            Some(t @ (TokenKind::GTGT | TokenKind::GE)) => {
                builder.advance(2);
                t
            }
            _ => {
                gt_token.drop(builder);
                builder.advance_lexer();
                return;
            }
        };

        gt_token.collapse(builder, collapsed);
    }

    /// `parseClassObjectAccess(builder, expr, optionalClassKeyword)` (PrattExpressionParser.kt:970).
    fn parse_class_object_access(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        expr: Marker,
        optional_class_keyword: bool,
    ) -> Option<Marker> {
        let mark = builder.mark();
        builder.advance_lexer();

        if builder.token_type() == Some(TokenKind::CLASS_KEYWORD) {
            mark.drop(builder);
            builder.advance_lexer();
        } else {
            if !optional_class_keyword {
                return None;
            }
            mark.rollback_to(builder);
            builder.error(message("class.literal.expected"));
        }

        expr.done(builder, NodeKind::CLASS_OBJECT_ACCESS_EXPRESSION);
        Some(expr)
    }

    /// `emptyExpression(builder)` (PrattExpressionParser.kt:992).
    fn empty_expression(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        empty_element(builder, NodeKind::EMPTY_EXPRESSION);
    }
}

/// `BreakPoint`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum BreakPoint {
    P1,
    P2,
    P4,
}

/// `InfixParser` and its implementations (`AssignmentParser`, `PolyExprParser`, `ConditionalExprParser`,
/// `InstanceofParser`): what parses from an infix operator on.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum InfixParser {
    Assignment,
    PolyExpr,
    ConditionalExpr,
    Instanceof,
}

impl InfixParser {
    /// `parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)`: starts to parse before the token with
    /// `binOpType`.
    pub(crate) fn parse(
        self,
        parser: &PrattExpressionParser<'_>,
        builder: &mut SyntaxTreeBuilder<'_>,
        before_lhs: Marker,
        bin_op_type: Option<TokenKind>,
        current_precedence: i32,
        mode: i32,
    ) {
        match self {
            InfixParser::Assignment => {
                assignment_parser_parse(parser, builder, before_lhs, bin_op_type, current_precedence, mode)
            }
            InfixParser::PolyExpr => {
                poly_expr_parser_parse(parser, builder, before_lhs, bin_op_type, current_precedence, mode)
            }
            InfixParser::ConditionalExpr => {
                conditional_expr_parser_parse(parser, builder, before_lhs, bin_op_type, current_precedence, mode)
            }
            InfixParser::Instanceof => {
                instanceof_parser_parse(parser, builder, before_lhs, bin_op_type, current_precedence, mode)
            }
        }
    }
}

/// `AssignmentParser.parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)` (PrattExpressionParser.kt:824).
pub fn assignment_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    bin_op_type: Option<TokenKind>,
    _current_precedence: i32,
    mode: i32,
) {
    parser.advance_bin_op_token(builder, bin_op_type);
    let right = parser.try_parse_with_precedence_at_most(builder, ASSIGNMENT_PRECEDENCE, mode);
    if right.is_none() {
        error(builder, message("expected.expression"));
    }
    done(builder, before_lhs, NodeKind::ASSIGNMENT_EXPRESSION, parser.my_parser.language_level);
}

/// `PolyExprParser.parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)` (PrattExpressionParser.kt:842).
pub fn poly_expr_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    bin_op_type: Option<TokenKind>,
    current_precedence: i32,
    mode: i32,
) {
    let mut operand_count = 1;
    loop {
        parser.advance_bin_op_token(builder, bin_op_type);
        let rhs = parser.try_parse_with_precedence_at_most(builder, current_precedence - 1, mode);
        if rhs.is_none() {
            error(builder, message("expected.expression"));
        }
        operand_count += 1;
        let next_token = parser.get_bin_op_token(builder);
        if next_token != bin_op_type {
            break;
        }
    }
    done(
        builder,
        before_lhs,
        if operand_count > 2 { NodeKind::POLYADIC_EXPRESSION } else { NodeKind::BINARY_EXPRESSION },
        parser.my_parser.language_level,
    );
}

/// `ConditionalExprParser.parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)`
/// (PrattExpressionParser.kt:871).
pub fn conditional_expr_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    _bin_op_type: Option<TokenKind>,
    _current_precedence: i32,
    mode: i32,
) {
    builder.advance_lexer(); // skipping ?

    let true_part = parser.parse_with(builder, mode);
    if true_part.is_none() {
        error(builder, message("expected.expression"));
        before_lhs.done(builder, NodeKind::CONDITIONAL_EXPRESSION);
        return;
    }

    if builder.token_type() != Some(TokenKind::COLON) {
        error(builder, message("expected.colon"));
        before_lhs.done(builder, NodeKind::CONDITIONAL_EXPRESSION);
        return;
    }
    builder.advance_lexer();

    let false_part = parser.try_parse_with_precedence_at_most(builder, CONDITIONAL_EXPR_PRECEDENCE, mode);
    if false_part.is_none() {
        error(builder, message("expected.expression"));
    }

    before_lhs.done(builder, NodeKind::CONDITIONAL_EXPRESSION);
}

/// `InstanceofParser.parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)`
/// (PrattExpressionParser.kt:905).
pub fn instanceof_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    _bin_op_type: Option<TokenKind>,
    _current_precedence: i32,
    _mode: i32,
) {
    builder.advance_lexer(); // skipping 'instanceof'

    let java_parser = parser.my_parser;
    if !java_parser.pattern_parser().is_pattern(builder) {
        let element_type = java_parser
            .reference_parser()
            .parse_type(builder, ReferenceParser::EAT_LAST_DOT | ReferenceParser::WILDCARD);
        if element_type.is_none() {
            error(builder, message("expected.type"));
        }
    } else {
        java_parser.pattern_parser().parse_primary_pattern(builder, false);
    }
    before_lhs.done(builder, NodeKind::INSTANCE_OF_EXPRESSION);
}

/// `ParserData(myPrecedence, myParser)`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct ParserData {
    pub(crate) my_precedence: i32,
    pub(crate) my_parser: InfixParser,
}

/// `INFIX_PARSERS[tokenType]`: the infix operator table `createInfixParsers()` builds.
pub(crate) fn infix_parsers(token_type: TokenKind) -> Option<ParserData> {
    use TokenKind::*;
    let (my_precedence, my_parser) = match token_type {
        EQ | ASTERISKEQ | DIVEQ | PERCEQ | PLUSEQ | MINUSEQ | LTLTEQ | GTGTEQ | GTGTGTEQ | ANDEQ | OREQ | XOREQ => {
            (ASSIGNMENT_PRECEDENCE, InfixParser::Assignment)
        }
        PLUS | MINUS => (ADDITIVE_PRECEDENCE, InfixParser::PolyExpr),
        DIV | ASTERISK | PERC => (MULTIPLICATION_PRECEDENCE, InfixParser::PolyExpr),
        LTLT | GTGT | GTGTGT => (SHIFT_PRECEDENCE, InfixParser::PolyExpr),
        LT | GT | LE | GE => (COMPARISON_AND_INSTANCEOF_PRECEDENCE, InfixParser::PolyExpr),
        INSTANCEOF_KEYWORD => (COMPARISON_AND_INSTANCEOF_PRECEDENCE, InfixParser::Instanceof),
        EQEQ | NE => (EQUALITY_PRECEDENCE, InfixParser::PolyExpr),
        OR => (BITWISE_OR_PRECEDENCE, InfixParser::PolyExpr),
        AND => (BITWISE_AND_PRECEDENCE, InfixParser::PolyExpr),
        XOR => (BITWISE_XOR_PRECEDENCE, InfixParser::PolyExpr),
        ANDAND => (LOGICAL_AND_PRECEDENCE, InfixParser::PolyExpr),
        OROR => (LOGICAL_OR_PRECEDENCE, InfixParser::PolyExpr),
        QUEST => (CONDITIONAL_EXPR_PRECEDENCE, InfixParser::ConditionalExpr),
        _ => return None,
    };
    Some(ParserData { my_precedence, my_parser })
}

/// `THIS_OR_SUPER`.
const THIS_OR_SUPER: TokenSet = TokenSet::create(&[TokenKind::THIS_KEYWORD, TokenKind::SUPER_KEYWORD]);
/// `ID_OR_SUPER`.
const ID_OR_SUPER: TokenSet = TokenSet::create(&[TokenKind::IDENTIFIER, TokenKind::SUPER_KEYWORD]);
/// `ARGS_LIST_CONTINUE`.
const ARGS_LIST_CONTINUE: TokenSet = TokenSet::create(&[
    TokenKind::IDENTIFIER,
    TokenKind::BAD_CHARACTER,
    TokenKind::COMMA,
    TokenKind::INTEGER_LITERAL,
    TokenKind::STRING_LITERAL,
]);
/// `ARGS_LIST_END`.
const ARGS_LIST_END: TokenSet = TokenSet::create(&[TokenKind::RPARENTH, TokenKind::RBRACE, TokenKind::RBRACKET]);
/// `TYPE_START`.
const TYPE_START: TokenSet =
    TokenSet::or_set(&[PRIMITIVE_TYPE_BIT_SET, TokenSet::create(&[TokenKind::IDENTIFIER, TokenKind::AT])]);
/// `POSTFIX_OPS`.
const POSTFIX_OPS: TokenSet = TokenSet::create(&[TokenKind::PLUSPLUS, TokenKind::MINUSMINUS]);
/// `PREF_ARITHMETIC_OPS`.
const PREF_ARITHMETIC_OPS: TokenSet =
    TokenSet::or_set(&[POSTFIX_OPS, TokenSet::create(&[TokenKind::PLUS, TokenKind::MINUS])]);
/// `PREFIX_OPS`.
const PREFIX_OPS: TokenSet =
    TokenSet::or_set(&[PREF_ARITHMETIC_OPS, TokenSet::create(&[TokenKind::TILDE, TokenKind::EXCL])]);

const MULTIPLICATION_PRECEDENCE: i32 = 2;
const ADDITIVE_PRECEDENCE: i32 = 3;
const SHIFT_PRECEDENCE: i32 = 4;
const COMPARISON_AND_INSTANCEOF_PRECEDENCE: i32 = 5;
const EQUALITY_PRECEDENCE: i32 = 6;
const BITWISE_AND_PRECEDENCE: i32 = 7;
const BITWISE_XOR_PRECEDENCE: i32 = 8;
const BITWISE_OR_PRECEDENCE: i32 = 9;
const LOGICAL_AND_PRECEDENCE: i32 = 10;
const LOGICAL_OR_PRECEDENCE: i32 = 11;
/// `CONDITIONAL_EXPR_PRECEDENCE`.
pub const CONDITIONAL_EXPR_PRECEDENCE: i32 = 12;
const ASSIGNMENT_PRECEDENCE: i32 = 13;
