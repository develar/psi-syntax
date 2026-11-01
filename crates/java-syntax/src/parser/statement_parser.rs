//! `StatementParser.kt`: code blocks and statements.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `BraceMode`.
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum BraceMode {
    TILL_FIRST,
    TILL_LAST,
}

/// `StatementParser(myParser, parseCodeBlockDeep)`: a view of its `JavaParser`, made by `JavaParser::statement_parser`
/// (`parseCodeBlockDeep` false, as `JavaParser` makes it).
#[derive(Copy, Clone)]
pub struct StatementParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
    pub(crate) parse_code_block_deep: bool,
}

impl<'p> StatementParser<'p> {
    /// `languageLevel`.
    fn language_level(&self) -> LanguageLevel {
        self.my_parser.language_level
    }

    /// `parseCodeBlock(builder, isStatement)` with `is_statement` at its default, false (StatementParser.kt:38).
    pub fn parse_code_block(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.parse_code_block_with(builder, false)
    }

    /// `parseCodeBlock(builder, isStatement)` (StatementParser.kt:38).
    pub fn parse_code_block_with(&self, builder: &mut SyntaxTreeBuilder<'_>, is_statement: bool) -> Option<Marker> {
        // TODO(port): StatementParser.kt:38
        todo!("StatementParser.parseCodeBlock (StatementParser.kt:38)")
    }

    /// `parseCodeBlockDeep(builder, parseUntilEof)` (StatementParser.kt:48).
    pub fn parse_code_block_deep(&self, builder: &mut SyntaxTreeBuilder<'_>, parse_until_eof: bool) -> Option<Marker> {
        // TODO(port): StatementParser.kt:48
        todo!("StatementParser.parseCodeBlockDeep (StatementParser.kt:48)")
    }

    /// `parseStatements(builder)` (StatementParser.kt:66).
    pub fn parse_statements(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): StatementParser.kt:66
        todo!("StatementParser.parseStatements (StatementParser.kt:66)")
    }

    /// `parseStatements(builder, braceMode)` (StatementParser.kt:70).
    fn parse_statements_with(&self, builder: &mut SyntaxTreeBuilder<'_>, brace_mode: Option<BraceMode>) {
        // TODO(port): StatementParser.kt:70
        todo!("StatementParser.parseStatements (StatementParser.kt:70)")
    }

    /// `parseStatement(builder)` (StatementParser.kt:93).
    pub fn parse_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): StatementParser.kt:93
        todo!("StatementParser.parseStatement (StatementParser.kt:93)")
    }

    /// `isStmtYieldToken(builder, tokenType)` (StatementParser.kt:270).
    fn is_stmt_yield_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        // TODO(port): StatementParser.kt:270
        todo!("StatementParser.isStmtYieldToken (StatementParser.kt:270)")
    }

    /// `parseIfStatement(builder)` (StatementParser.kt:295).
    fn parse_if_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:295
        todo!("StatementParser.parseIfStatement (StatementParser.kt:295)")
    }

    /// `parseWhileStatement(builder)` (StatementParser.kt:330).
    fn parse_while_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:330
        todo!("StatementParser.parseWhileStatement (StatementParser.kt:330)")
    }

    /// `isRecordPatternInForEach(builder)` (StatementParser.kt:336).
    fn is_record_pattern_in_for_each(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        // TODO(port): StatementParser.kt:336
        todo!("StatementParser.isRecordPatternInForEach (StatementParser.kt:336)")
    }

    /// `parseForStatement(builder)` (StatementParser.kt:371).
    fn parse_for_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:371
        todo!("StatementParser.parseForStatement (StatementParser.kt:371)")
    }

    /// `parseForLoopFromInitializer(builder, statement)` (StatementParser.kt:413).
    fn parse_for_loop_from_initializer(&self, builder: &mut SyntaxTreeBuilder<'_>, statement: Marker) -> Marker {
        // TODO(port): StatementParser.kt:413
        todo!("StatementParser.parseForLoopFromInitializer (StatementParser.kt:413)")
    }

    /// `parseForUpdateExpressions(builder)` (StatementParser.kt:458).
    fn parse_for_update_expressions(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): StatementParser.kt:458
        todo!("StatementParser.parseForUpdateExpressions (StatementParser.kt:458)")
    }

    /// `parseForEachFromColon(builder, statement, foreachStatement)` (StatementParser.kt:487).
    fn parse_for_each_from_colon(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        statement: Marker,
        foreach_statement: NodeKind,
    ) -> Marker {
        // TODO(port): StatementParser.kt:487
        todo!("StatementParser.parseForEachFromColon (StatementParser.kt:487)")
    }

    /// `parserForEachFromRparenth(builder, statement, forEachType)` (StatementParser.kt:501).
    fn parser_for_each_from_rparenth(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        statement: Marker,
        for_each_type: NodeKind,
    ) -> Marker {
        // TODO(port): StatementParser.kt:501
        todo!("StatementParser.parserForEachFromRparenth (StatementParser.kt:501)")
    }

    /// `parseDoWhileStatement(builder)` (StatementParser.kt:514).
    fn parse_do_while_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:514
        todo!("StatementParser.parseDoWhileStatement (StatementParser.kt:514)")
    }

    /// `parseSwitchStatement(builder)` (StatementParser.kt:533).
    fn parse_switch_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:533
        todo!("StatementParser.parseSwitchStatement (StatementParser.kt:533)")
    }

    /// `parseCaseLabel(builder)` (StatementParser.kt:541).
    pub fn parse_case_label(&self, builder: &mut SyntaxTreeBuilder<'_>) -> (Option<Marker>, Option<bool>) {
        // TODO(port): StatementParser.kt:541
        todo!("StatementParser.parseCaseLabel (StatementParser.kt:541)")
    }

    /// `parseSwitchLabelStatement(builder)` (StatementParser.kt:555).
    fn parse_switch_label_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:555
        todo!("StatementParser.parseSwitchLabelStatement (StatementParser.kt:555)")
    }

    /// `parseGuard(builder)` (StatementParser.kt:609).
    fn parse_guard(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): StatementParser.kt:609
        todo!("StatementParser.parseGuard (StatementParser.kt:609)")
    }

    /// `parseBreakStatement(builder)` (StatementParser.kt:620).
    fn parse_break_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:620
        todo!("StatementParser.parseBreakStatement (StatementParser.kt:620)")
    }

    /// `parseYieldStatement(builder)` (StatementParser.kt:629).
    fn parse_yield_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:629
        todo!("StatementParser.parseYieldStatement (StatementParser.kt:629)")
    }

    /// `parseContinueStatement(builder)` (StatementParser.kt:645).
    fn parse_continue_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:645
        todo!("StatementParser.parseContinueStatement (StatementParser.kt:645)")
    }

    /// `parseReturnStatement(builder)` (StatementParser.kt:654).
    fn parse_return_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:654
        todo!("StatementParser.parseReturnStatement (StatementParser.kt:654)")
    }

    /// `parseThrowStatement(builder)` (StatementParser.kt:663).
    fn parse_throw_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:663
        todo!("StatementParser.parseThrowStatement (StatementParser.kt:663)")
    }

    /// `parseSynchronizedStatement(builder)` (StatementParser.kt:678).
    fn parse_synchronized_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:678
        todo!("StatementParser.parseSynchronizedStatement (StatementParser.kt:678)")
    }

    /// `parseTryStatement(builder)` (StatementParser.kt:682).
    fn parse_try_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:682
        todo!("StatementParser.parseTryStatement (StatementParser.kt:682)")
    }

    /// `parseCatchBlock(builder)` (StatementParser.kt:715).
    pub fn parse_catch_block(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        // TODO(port): StatementParser.kt:715
        todo!("StatementParser.parseCatchBlock (StatementParser.kt:715)")
    }

    /// `parseAssertStatement(builder)` (StatementParser.kt:748).
    fn parse_assert_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:748
        todo!("StatementParser.parseAssertStatement (StatementParser.kt:748)")
    }

    /// `parseBlockStatement(builder)` (StatementParser.kt:766).
    fn parse_block_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): StatementParser.kt:766
        todo!("StatementParser.parseBlockStatement (StatementParser.kt:766)")
    }

    /// `parseExprInParenthWithBlock(builder, type, block)` (StatementParser.kt:773).
    pub fn parse_expr_in_parenth_with_block(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        element_type: NodeKind,
        block: bool,
    ) -> Marker {
        // TODO(port): StatementParser.kt:773
        todo!("StatementParser.parseExprInParenthWithBlock (StatementParser.kt:773)")
    }

    /// `parseExprInParenth(builder)` (StatementParser.kt:788).
    fn parse_expr_in_parenth(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        // TODO(port): StatementParser.kt:788
        todo!("StatementParser.parseExprInParenth (StatementParser.kt:788)")
    }

    /// `skipQualifiedName(builder)` (StatementParser.kt:815).
    fn skip_qualified_name(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): StatementParser.kt:815
        todo!("StatementParser.skipQualifiedName (StatementParser.kt:815)")
    }

    /// `getLastToken(builder)` (StatementParser.kt:822).
    fn get_last_token(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<TokenKind> {
        // TODO(port): StatementParser.kt:822
        todo!("StatementParser.getLastToken (StatementParser.kt:822)")
    }
}

/// `YIELD_STMT_INDICATOR_TOKENS`.
const YIELD_STMT_INDICATOR_TOKENS: TokenSet = TokenSet::create(&[
    TokenKind::PLUS,
    TokenKind::MINUS,
    TokenKind::EXCL,
    TokenKind::TILDE,
    TokenKind::SUPER_KEYWORD,
    TokenKind::THIS_KEYWORD,
    TokenKind::TRUE_KEYWORD,
    TokenKind::FALSE_KEYWORD,
    TokenKind::NULL_KEYWORD,
    TokenKind::STRING_LITERAL,
    TokenKind::INTEGER_LITERAL,
    TokenKind::DOUBLE_LITERAL,
    TokenKind::FLOAT_LITERAL,
    TokenKind::LONG_LITERAL,
    TokenKind::CHARACTER_LITERAL,
    TokenKind::TEXT_BLOCK_LITERAL,
    TokenKind::IDENTIFIER,
    TokenKind::SWITCH_KEYWORD,
    TokenKind::NEW_KEYWORD,
    TokenKind::LPARENTH, // recovery
    TokenKind::RBRACE,
    TokenKind::SEMICOLON,
    TokenKind::CASE_KEYWORD,
]);

/// `TRY_CLOSERS_SET`.
const TRY_CLOSERS_SET: TokenSet = TokenSet::create(&[TokenKind::CATCH_KEYWORD, TokenKind::FINALLY_KEYWORD]);
