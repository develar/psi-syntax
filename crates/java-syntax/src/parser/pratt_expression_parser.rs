//! `PrattExpressionParser.kt`: expressions, by precedence climbing over the infix operator table.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::syntax_element_types::PRIMITIVE_TYPE_BIT_SET;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `PrattExpressionParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::pratt_expression_parser`.
#[derive(Copy, Clone)]
pub struct PrattExpressionParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> PrattExpressionParser<'p> {
    /// `parse(builder)` (PrattExpressionParser.kt:33).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:33
        todo!("PrattExpressionParser.parse (PrattExpressionParser.kt:33)")
    }

    /// `parse(builder, mode)` (PrattExpressionParser.kt:37).
    pub fn parse_with(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:37
        todo!("PrattExpressionParser.parse (PrattExpressionParser.kt:37)")
    }

    /// `tryParseWithPrecedenceAtMost(builder, maxPrecedence, mode)` (PrattExpressionParser.kt:45).
    pub fn try_parse_with_precedence_at_most(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        max_precedence: i32,
        mode: i32,
    ) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:45
        todo!("PrattExpressionParser.tryParseWithPrecedenceAtMost (PrattExpressionParser.kt:45)")
    }

    /// `parseUnary(builder, mode)` (PrattExpressionParser.kt:62).
    fn parse_unary(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:62
        todo!("PrattExpressionParser.parseUnary (PrattExpressionParser.kt:62)")
    }

    /// `parsePostfix(builder, mode)` (PrattExpressionParser.kt:118).
    fn parse_postfix(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:118
        todo!("PrattExpressionParser.parsePostfix (PrattExpressionParser.kt:118)")
    }

    /// `parsePrimary(builder, breakPoint, breakOffset, mode)` (PrattExpressionParser.kt:131).
    fn parse_primary(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        break_point: Option<BreakPoint>,
        break_offset: i32,
        mode: i32,
    ) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:131
        todo!("PrattExpressionParser.parsePrimary (PrattExpressionParser.kt:131)")
    }

    /// `parsePrimaryExpressionStart(builder, mode)` (PrattExpressionParser.kt:316).
    fn parse_primary_expression_start(&self, builder: &mut SyntaxTreeBuilder<'_>, mode: i32) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:316
        todo!("PrattExpressionParser.parsePrimaryExpressionStart (PrattExpressionParser.kt:316)")
    }

    /// `parseClassAccessOrMethodReference(builder)` (PrattExpressionParser.kt:449).
    fn parse_class_access_or_method_reference(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:449
        todo!("PrattExpressionParser.parseClassAccessOrMethodReference (PrattExpressionParser.kt:449)")
    }

    /// `parseClassAccessOrMethodReference(builder, expr, optionalClassKeyword)` (PrattExpressionParser.kt:463).
    fn parse_class_access_or_method_reference_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        expr: Marker,
        optional_class_keyword: bool,
    ) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:463
        todo!("PrattExpressionParser.parseClassAccessOrMethodReference (PrattExpressionParser.kt:463)")
    }

    /// `parseMethodReference(builder, start)` (PrattExpressionParser.kt:479).
    fn parse_method_reference(&self, builder: &mut SyntaxTreeBuilder<'_>, start: Marker) -> Marker {
        // TODO(port): PrattExpressionParser.kt:479
        todo!("PrattExpressionParser.parseMethodReference (PrattExpressionParser.kt:479)")
    }

    /// `parseStringTemplate(builder, start, textBlock)` (PrattExpressionParser.kt:493).
    fn parse_string_template(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        start: Option<Marker>,
        text_block: bool,
    ) -> Marker {
        // TODO(port): PrattExpressionParser.kt:493
        todo!("PrattExpressionParser.parseStringTemplate (PrattExpressionParser.kt:493)")
    }

    /// `parseNew(builder, start)` (PrattExpressionParser.kt:528).
    fn parse_new(&self, builder: &mut SyntaxTreeBuilder<'_>, start: Option<Marker>) -> Marker {
        // TODO(port): PrattExpressionParser.kt:528
        todo!("PrattExpressionParser.parseNew (PrattExpressionParser.kt:528)")
    }

    /// `parseArrayInitializer(builder)` (PrattExpressionParser.kt:614).
    fn parse_array_initializer(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): PrattExpressionParser.kt:614
        todo!("PrattExpressionParser.parseArrayInitializer (PrattExpressionParser.kt:614)")
    }

    /// `parseArrayInitializer(builder, type, elementParser, missingElementKey)` (PrattExpressionParser.kt:621).
    pub fn parse_array_initializer_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        element_type: NodeKind,
        element_parser: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> Option<Marker>,
        missing_element_key: &'static str,
    ) -> Marker {
        // TODO(port): PrattExpressionParser.kt:621
        todo!("PrattExpressionParser.parseArrayInitializer (PrattExpressionParser.kt:621)")
    }

    /// `parseArgumentList(builder)` (PrattExpressionParser.kt:668).
    pub fn parse_argument_list(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): PrattExpressionParser.kt:668
        todo!("PrattExpressionParser.parseArgumentList (PrattExpressionParser.kt:668)")
    }

    /// `parseLambdaAfterParenth(builder)` (PrattExpressionParser.kt:722).
    fn parse_lambda_after_parenth(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:722
        todo!("PrattExpressionParser.parseLambdaAfterParenth (PrattExpressionParser.kt:722)")
    }

    /// `parseLambdaExpression(builder, typed)` (PrattExpressionParser.kt:777).
    fn parse_lambda_expression(&self, builder: &mut SyntaxTreeBuilder<'_>, typed: bool) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:777
        todo!("PrattExpressionParser.parseLambdaExpression (PrattExpressionParser.kt:777)")
    }

    /// `getBinOpToken(builder)` (PrattExpressionParser.kt:930).
    fn get_bin_op_token(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<TokenKind> {
        // TODO(port): PrattExpressionParser.kt:930
        todo!("PrattExpressionParser.getBinOpToken (PrattExpressionParser.kt:930)")
    }

    /// `advanceBinOpToken(builder, type)` (PrattExpressionParser.kt:953).
    fn advance_bin_op_token(&self, builder: &mut SyntaxTreeBuilder<'_>, element_type: Option<TokenKind>) {
        // TODO(port): PrattExpressionParser.kt:953
        todo!("PrattExpressionParser.advanceBinOpToken (PrattExpressionParser.kt:953)")
    }

    /// `parseClassObjectAccess(builder, expr, optionalClassKeyword)` (PrattExpressionParser.kt:970).
    fn parse_class_object_access(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        expr: Marker,
        optional_class_keyword: bool,
    ) -> Option<Marker> {
        // TODO(port): PrattExpressionParser.kt:970
        todo!("PrattExpressionParser.parseClassObjectAccess (PrattExpressionParser.kt:970)")
    }

    /// `emptyExpression(builder)` (PrattExpressionParser.kt:992).
    fn empty_expression(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): PrattExpressionParser.kt:992
        todo!("PrattExpressionParser.emptyExpression (PrattExpressionParser.kt:992)")
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

/// `parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)` (PrattExpressionParser.kt:824).
pub fn assignment_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    bin_op_type: Option<TokenKind>,
    current_precedence: i32,
    mode: i32,
) {
    // TODO(port): PrattExpressionParser.kt:824
    todo!("PrattExpressionParser.parse (PrattExpressionParser.kt:824)")
}

/// `parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)` (PrattExpressionParser.kt:842).
pub fn poly_expr_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    bin_op_type: Option<TokenKind>,
    current_precedence: i32,
    mode: i32,
) {
    // TODO(port): PrattExpressionParser.kt:842
    todo!("PrattExpressionParser.parse (PrattExpressionParser.kt:842)")
}

/// `parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)` (PrattExpressionParser.kt:871).
pub fn conditional_expr_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    bin_op_type: Option<TokenKind>,
    current_precedence: i32,
    mode: i32,
) {
    // TODO(port): PrattExpressionParser.kt:871
    todo!("PrattExpressionParser.parse (PrattExpressionParser.kt:871)")
}

/// `parse(parser, builder, beforeLhs, binOpType, currentPrecedence, mode)` (PrattExpressionParser.kt:905).
pub fn instanceof_parser_parse(
    parser: &PrattExpressionParser<'_>,
    builder: &mut SyntaxTreeBuilder<'_>,
    before_lhs: Marker,
    bin_op_type: Option<TokenKind>,
    current_precedence: i32,
    mode: i32,
) {
    // TODO(port): PrattExpressionParser.kt:905
    todo!("PrattExpressionParser.parse (PrattExpressionParser.kt:905)")
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
