//! `ExpressionParser.kt`: the facade over `PrattExpressionParser` the other parsers call.

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::node_kind::NodeKind;
use crate::parser::pratt_expression_parser::{CONDITIONAL_EXPR_PRECEDENCE, PrattExpressionParser};

/// `ExpressionParser(myNewExpressionParser)`, made by `JavaParser::expression_parser` over
/// `PrattExpressionParser(this)`.
#[derive(Copy, Clone)]
pub struct ExpressionParser<'p> {
    pub(crate) my_new_expression_parser: PrattExpressionParser<'p>,
}

impl<'p> ExpressionParser<'p> {
    /// `FORBID_LAMBDA_MASK`.
    pub const FORBID_LAMBDA_MASK: i32 = 0x1;

    /// `parse(builder)` (ExpressionParser.kt:12).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.my_new_expression_parser.parse(builder)
    }

    /// `parseArgumentList(builder)` (ExpressionParser.kt:16).
    pub fn parse_argument_list(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.my_new_expression_parser.parse_argument_list(builder)
    }

    /// `parseArrayInitializer(builder, type, elementParser, missingElementKey)` (ExpressionParser.kt:20).
    pub fn parse_array_initializer(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        element_type: NodeKind,
        element_parser: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> Option<Marker>,
        missing_element_key: &'static str,
    ) -> Marker {
        self.my_new_expression_parser.parse_array_initializer_with(
            builder,
            element_type,
            element_parser,
            missing_element_key,
        )
    }

    /// `parseConditional(builder)` (ExpressionParser.kt:29).
    pub fn parse_conditional(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.my_new_expression_parser.try_parse_with_precedence_at_most(
            builder,
            CONDITIONAL_EXPR_PRECEDENCE,
            Self::FORBID_LAMBDA_MASK,
        )
    }

    /// `parseAssignmentForbiddingLambda(builder)` (ExpressionParser.kt:33).
    pub fn parse_assignment_forbidding_lambda(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.my_new_expression_parser.parse_with(builder, Self::FORBID_LAMBDA_MASK)
    }
}
