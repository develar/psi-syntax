//! `StatementParser.kt`: code blocks and statements.

use psi_builder::Marker;

use crate::builder::{Binder, SyntaxTreeBuilder};
use crate::bundle::message;
use crate::language_level::{JavaFeature, LanguageLevel};
use crate::node_kind::NodeKind;
use crate::parser::declaration_parser::{Context, is_non_sealed_token};
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{done, error, expect_or_error, expr_type, semicolon};
use crate::syntax_element_types::JAVA_COMMENT_OR_WHITESPACE_BIT_SET;
use crate::tokens::{TokenKind, TokenSet};

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
        if builder.token_type() != Some(TokenKind::LBRACE) {
            return None;
        }
        if is_statement && self.parse_code_block_deep {
            self.parse_code_block_deep(builder, false)
        } else {
            builder.parse_block_lazy(TokenKind::LBRACE, TokenKind::RBRACE, NodeKind::CODE_BLOCK)
        }
    }

    /// `parseCodeBlockDeep(builder, parseUntilEof)` (StatementParser.kt:48).
    pub fn parse_code_block_deep(&self, builder: &mut SyntaxTreeBuilder<'_>, parse_until_eof: bool) -> Option<Marker> {
        if builder.token_type() != Some(TokenKind::LBRACE) {
            return None;
        }

        let code_block = builder.mark();
        builder.advance_lexer();

        self.parse_statements_with(
            builder,
            Some(if parse_until_eof { BraceMode::TILL_LAST } else { BraceMode::TILL_FIRST }),
        );

        let greedy_block = !expect_or_error(builder, Some(TokenKind::RBRACE), "expected.rbrace");
        builder.token_type(); // eat spaces

        done(builder, code_block, NodeKind::CODE_BLOCK, self.my_parser.language_level);
        if greedy_block {
            code_block.set_custom_edge_token_binders(builder, None, Some(Binder::DefaultLeft));
        }
        Some(code_block)
    }

    /// `parseStatements(builder)` (StatementParser.kt:66).
    pub fn parse_statements(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        self.parse_statements_with(builder, None);
    }

    /// `parseStatements(builder, braceMode)` (StatementParser.kt:70).
    fn parse_statements_with(&self, builder: &mut SyntaxTreeBuilder<'_>, brace_mode: Option<BraceMode>) {
        while builder.token_type().is_some() {
            let statement = self.parse_statement(builder);
            if statement.is_some() {
                continue;
            }

            let token_type = builder.token_type();
            if token_type == Some(TokenKind::RBRACE)
                && (brace_mode == Some(BraceMode::TILL_FIRST)
                    || brace_mode == Some(BraceMode::TILL_LAST) && builder.look_ahead(1).is_none())
            {
                break;
            }

            let error = builder.mark();
            builder.advance_lexer();
            match token_type {
                Some(TokenKind::ELSE_KEYWORD) => error.error(builder, message("else.without.if")),
                Some(TokenKind::CATCH_KEYWORD) => error.error(builder, message("catch.without.try")),
                Some(TokenKind::FINALLY_KEYWORD) => error.error(builder, message("finally.without.try")),
                _ => error.error(builder, message("unexpected.token")),
            }
        }
    }

    /// `parseStatement(builder)` (StatementParser.kt:93).
    pub fn parse_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let token_type = builder.token_type();
        if token_type == Some(TokenKind::IF_KEYWORD) {
            return Some(self.parse_if_statement(builder));
        } else if token_type == Some(TokenKind::WHILE_KEYWORD) {
            return Some(self.parse_while_statement(builder));
        } else if token_type == Some(TokenKind::FOR_KEYWORD) {
            return Some(self.parse_for_statement(builder));
        } else if token_type == Some(TokenKind::DO_KEYWORD) {
            return Some(self.parse_do_while_statement(builder));
        } else if token_type == Some(TokenKind::SWITCH_KEYWORD) {
            return Some(self.parse_switch_statement(builder));
        } else if token_type == Some(TokenKind::CASE_KEYWORD) || token_type == Some(TokenKind::DEFAULT_KEYWORD) {
            return Some(self.parse_switch_label_statement(builder));
        } else if token_type == Some(TokenKind::BREAK_KEYWORD) {
            return Some(self.parse_break_statement(builder));
        } else if self.is_stmt_yield_token(builder, token_type) {
            return Some(self.parse_yield_statement(builder));
        } else if token_type == Some(TokenKind::CONTINUE_KEYWORD) {
            return Some(self.parse_continue_statement(builder));
        } else if token_type == Some(TokenKind::RETURN_KEYWORD) {
            return Some(self.parse_return_statement(builder));
        } else if token_type == Some(TokenKind::THROW_KEYWORD) {
            return Some(self.parse_throw_statement(builder));
        } else if token_type == Some(TokenKind::SYNCHRONIZED_KEYWORD) {
            return Some(self.parse_synchronized_statement(builder));
        } else if token_type == Some(TokenKind::TRY_KEYWORD) {
            return Some(self.parse_try_statement(builder));
        } else if token_type == Some(TokenKind::ASSERT_KEYWORD) {
            return Some(self.parse_assert_statement(builder));
        } else if token_type == Some(TokenKind::LBRACE) {
            return Some(self.parse_block_statement(builder));
        } else if token_type == Some(TokenKind::SEMICOLON) {
            let empty = builder.mark();
            builder.advance_lexer();
            done(builder, empty, NodeKind::EMPTY_STATEMENT, self.my_parser.language_level);
            return Some(empty);
        } else if token_type == Some(TokenKind::IDENTIFIER) || token_type == Some(TokenKind::AT) {
            let ref_pos = builder.mark();
            let non_sealed = is_non_sealed_token(builder, token_type, self.language_level());
            self.my_parser.declaration_parser().parse_annotations(builder);
            self.skip_qualified_name(builder);
            let current = builder.token_type();
            let next = builder.look_ahead(1);
            ref_pos.rollback_to(builder);

            if current == Some(TokenKind::LT)
                || current == Some(TokenKind::DOT) && next == Some(TokenKind::AT)
                || non_sealed
            {
                let decl_statement = builder.mark();

                if self.my_parser.declaration_parser().parse(builder, Some(Context::CODE_BLOCK)).is_some() {
                    done(builder, decl_statement, NodeKind::DECLARATION_STATEMENT, self.my_parser.language_level);
                    return Some(decl_statement);
                }

                let element_type = self.my_parser.reference_parser().parse_type_info(builder, 0);
                if current == Some(TokenKind::LT) && element_type.as_ref().is_none_or(|t| !t.is_parameterized) {
                    decl_statement.rollback_to(builder);
                } else if element_type.is_none() || builder.token_type() != Some(TokenKind::DOUBLE_COLON) {
                    error(builder, message("expected.identifier"));
                    if element_type.is_none() {
                        builder.advance_lexer();
                    }
                    done(builder, decl_statement, NodeKind::DECLARATION_STATEMENT, self.my_parser.language_level);
                    return Some(decl_statement);
                } else {
                    decl_statement.rollback_to(builder); // generic type followed by the double colon is a good candidate for being a constructor reference
                }
            }
        }

        let pos = builder.mark();
        let expr = self.my_parser.expression_parser().parse(builder);
        let mut incomplete_declaration_restricted_token_type: Option<TokenKind> = None;

        if let Some(expr) = expr {
            let mut count = 1;
            let list = expr.precede(builder);
            let statement = list.precede(builder);
            while builder.token_type() == Some(TokenKind::COMMA) {
                let comma_pos = builder.mark();
                builder.advance_lexer();
                let expr1 = self.my_parser.expression_parser().parse(builder);
                if expr1.is_none() {
                    comma_pos.rollback_to(builder);
                    break;
                }
                comma_pos.drop(builder);
                count += 1;
            }
            if count > 1 {
                pos.drop(builder);
                done(builder, list, NodeKind::EXPRESSION_LIST, self.my_parser.language_level);
                semicolon(builder);
                done(builder, statement, NodeKind::EXPRESSION_LIST_STATEMENT, self.my_parser.language_level);
                return Some(statement);
            }
            if expr_type(builder, Some(expr)) != NodeKind::REFERENCE_EXPRESSION {
                crate::builder::drop(builder, &[Some(list), Some(pos)]);
                semicolon(builder);
                done(builder, statement, NodeKind::EXPRESSION_STATEMENT, self.my_parser.language_level);
                return Some(statement);
            }
            let single_token = builder.marker_end_token_index(expr) - builder.marker_start_token_index(expr) == 1;
            pos.rollback_to(builder);
            if single_token && builder.token_type() == Some(TokenKind::IDENTIFIER) {
                let text = builder.token_text();
                let is_record = text == Some(java_keywords::RECORD.as_bytes());
                let is_var = text == Some(java_keywords::VAR.as_bytes());
                if is_record && JavaFeature::RECORDS.is_sufficient(self.language_level()) {
                    incomplete_declaration_restricted_token_type = Some(TokenKind::RECORD_KEYWORD);
                }
                if is_var && JavaFeature::LVTI.is_sufficient(self.language_level()) {
                    incomplete_declaration_restricted_token_type = Some(TokenKind::VAR_KEYWORD);
                }
            }
        } else {
            pos.drop(builder);
        }

        let decl = self.my_parser.declaration_parser().parse(builder, Some(Context::CODE_BLOCK));
        if let Some(decl) = decl {
            let statement = decl.precede(builder);
            done(builder, statement, NodeKind::DECLARATION_STATEMENT, self.my_parser.language_level);
            return Some(statement);
        }

        if builder.token_type() == Some(TokenKind::IDENTIFIER) && builder.look_ahead(1) == Some(TokenKind::COLON) {
            let statement = builder.mark();
            builder.advance(2);
            self.parse_statement(builder);
            done(builder, statement, NodeKind::LABELED_STATEMENT, self.my_parser.language_level);
            return Some(statement);
        }

        if expr.is_some() {
            let statement = builder.mark();
            let statement_type = if let Some(restricted) = incomplete_declaration_restricted_token_type {
                builder.remap_current_token(restricted);
                builder.advance_lexer();
                error(builder, message("expected.identifier"));
                NodeKind::DECLARATION_STATEMENT
            } else {
                self.my_parser.expression_parser().parse(builder);
                semicolon(builder);
                NodeKind::EXPRESSION_STATEMENT
            };
            done(builder, statement, statement_type, self.my_parser.language_level);
            return Some(statement);
        }

        None
    }

    /// `isStmtYieldToken(builder, tokenType)` (StatementParser.kt:270).
    fn is_stmt_yield_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        if !(token_type == Some(TokenKind::IDENTIFIER)
            && builder.token_text() == Some(java_keywords::YIELD.as_bytes())
            && JavaFeature::SWITCH_EXPRESSION.is_sufficient(self.language_level()))
        {
            return false;
        }
        // we prefer to parse it as yield stmt wherever possible (even in incomplete syntax)
        let maybe_yield_stmt = builder.mark();
        builder.advance_lexer();
        let token_after_yield = builder.token_type();
        if token_after_yield.is_none() || YIELD_STMT_INDICATOR_TOKENS.contains(token_after_yield) {
            maybe_yield_stmt.rollback_to(builder);
            return true;
        }
        if token_after_yield == Some(TokenKind::PLUSPLUS) || token_after_yield == Some(TokenKind::MINUSMINUS) {
            builder.advance_lexer();
            let is_yield_stmt = builder.token_type() != Some(TokenKind::SEMICOLON);
            maybe_yield_stmt.rollback_to(builder);
            return is_yield_stmt;
        }
        maybe_yield_stmt.rollback_to(builder);
        false
    }

    /// `parseIfStatement(builder)` (StatementParser.kt:295).
    fn parse_if_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let mut stack: Option<Vec<Marker>> = None;
        let mut statement: Marker;
        loop {
            // replaced recursion with iteration plus stack to avoid huge call stack for extremely large else-if chains
            statement = builder.mark();
            builder.advance_lexer();

            if self.parse_expr_in_parenth(builder) {
                if self.parse_statement(builder).is_none() {
                    error(builder, message("expected.statement"));
                } else if builder.expect(Some(TokenKind::ELSE_KEYWORD)) {
                    if builder.token_type() == Some(TokenKind::IF_KEYWORD) {
                        stack.get_or_insert_with(Vec::new).push(statement);
                        continue;
                    } else if self.parse_statement(builder).is_none() {
                        error(builder, message("expected.statement"));
                    }
                }
            }
            break;
        }
        done(builder, statement, NodeKind::IF_STATEMENT, self.my_parser.language_level);
        if let Some(stack) = stack {
            for &outer in stack.iter().rev() {
                statement = outer;
                done(builder, statement, NodeKind::IF_STATEMENT, self.my_parser.language_level);
            }
        }
        statement
    }

    /// `parseWhileStatement(builder)` (StatementParser.kt:330).
    fn parse_while_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_expr_in_parenth_with_block(builder, NodeKind::WHILE_STATEMENT, false)
    }

    /// `isRecordPatternInForEach(builder)` (StatementParser.kt:336).
    fn is_record_pattern_in_for_each(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        let Some(pattern_start) = self.my_parser.pattern_parser().pre_parse_pattern(builder) else {
            return false;
        };
        if builder.token_type() != Some(TokenKind::LPARENTH) {
            pattern_start.rollback_to(builder);
            return false;
        }
        builder.advance_lexer();

        // we must distinguish a record pattern from method call in for (foo();;)
        let mut paren_balance = 1;
        loop {
            let current = builder.token_type();
            if current.is_none() {
                pattern_start.rollback_to(builder);
                return false;
            }
            if current == Some(TokenKind::LPARENTH) {
                paren_balance += 1;
            }
            if current == Some(TokenKind::RPARENTH) {
                paren_balance -= 1;
                if paren_balance == 0 {
                    break;
                }
            }
            builder.advance_lexer();
        }
        builder.advance_lexer();
        let is_record_pattern = builder.token_type() != Some(TokenKind::SEMICOLON)
            && builder.token_type() != Some(TokenKind::DOT)
            && builder.token_type() != Some(TokenKind::COMMA);
        pattern_start.rollback_to(builder);
        is_record_pattern
    }

    /// `parseForStatement(builder)` (StatementParser.kt:371).
    fn parse_for_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();

        if !builder.expect(Some(TokenKind::LPARENTH)) {
            error(builder, message("expected.lparen"));
            done(builder, statement, NodeKind::FOR_STATEMENT, self.my_parser.language_level);
            return statement;
        }

        if self.is_record_pattern_in_for_each(builder) {
            self.my_parser.pattern_parser().parse_pattern(builder);
            if builder.token_type() == Some(TokenKind::COLON) {
                return self.parse_for_each_from_colon(builder, statement, NodeKind::FOREACH_PATTERN_STATEMENT);
            }
            error(builder, message("expected.colon"));
            // recovery: just skip everything until ')'
            loop {
                let token_type = builder.token_type();
                if token_type.is_none() {
                    done(builder, statement, NodeKind::FOREACH_PATTERN_STATEMENT, self.my_parser.language_level);
                    return statement;
                }
                if token_type == Some(TokenKind::RPARENTH) {
                    return self.parser_for_each_from_rparenth(builder, statement, NodeKind::FOREACH_PATTERN_STATEMENT);
                }
                builder.advance_lexer();
            }
        }

        let after_parenth = builder.mark();
        let param = self.my_parser.declaration_parser().parse_parameter(builder, false, false, true);
        if param.is_none()
            || expr_type(builder, param) != NodeKind::PARAMETER
            || builder.token_type() != Some(TokenKind::COLON)
        {
            after_parenth.rollback_to(builder);
            self.parse_for_loop_from_initializer(builder, statement)
        } else {
            after_parenth.drop(builder);
            self.parse_for_each_from_colon(builder, statement, NodeKind::FOREACH_STATEMENT)
        }
    }

    /// `parseForLoopFromInitializer(builder, statement)` (StatementParser.kt:413).
    fn parse_for_loop_from_initializer(&self, builder: &mut SyntaxTreeBuilder<'_>, statement: Marker) -> Marker {
        if self.parse_statement(builder).is_none() {
            error(builder, message("expected.statement"));
            if !builder.expect(Some(TokenKind::RPARENTH)) {
                done(builder, statement, NodeKind::FOR_STATEMENT, self.my_parser.language_level);
                return statement;
            }
        } else {
            let mut missing_semicolon = false;
            if self.get_last_token(builder) != Some(TokenKind::SEMICOLON) {
                missing_semicolon = !expect_or_error(builder, Some(TokenKind::SEMICOLON), "expected.semicolon");
            }

            let expr = self.my_parser.expression_parser().parse(builder);
            missing_semicolon &= expr.is_none();

            if !builder.expect(Some(TokenKind::SEMICOLON)) {
                if !missing_semicolon {
                    error(builder, message("expected.semicolon"));
                }
                if !builder.expect(Some(TokenKind::RPARENTH)) {
                    done(builder, statement, NodeKind::FOR_STATEMENT, self.my_parser.language_level);
                    return statement;
                }
            } else {
                self.parse_for_update_expressions(builder);
                if !builder.expect(Some(TokenKind::RPARENTH)) {
                    error(builder, message("expected.rparen"));
                    done(builder, statement, NodeKind::FOR_STATEMENT, self.my_parser.language_level);
                    return statement;
                }
            }
        }

        let body_statement = self.parse_statement(builder);
        if body_statement.is_none() {
            error(builder, message("expected.statement"));
        }

        done(builder, statement, NodeKind::FOR_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseForUpdateExpressions(builder)` (StatementParser.kt:458).
    fn parse_for_update_expressions(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let Some(expr) = self.my_parser.expression_parser().parse(builder) else {
            return;
        };

        let expression_statement: Marker;
        if builder.token_type() != Some(TokenKind::COMMA) {
            expression_statement = expr.precede(builder);
            done(builder, expression_statement, NodeKind::EXPRESSION_STATEMENT, self.my_parser.language_level);
        } else {
            let expression_list = expr.precede(builder);
            expression_statement = expression_list.precede(builder);

            loop {
                builder.advance_lexer();
                let next_expression = self.my_parser.expression_parser().parse(builder);
                if next_expression.is_none() {
                    error(builder, message("expected.expression"));
                }
                if builder.token_type() != Some(TokenKind::COMMA) {
                    break;
                }
            }

            done(builder, expression_list, NodeKind::EXPRESSION_LIST, self.my_parser.language_level);
            done(builder, expression_statement, NodeKind::EXPRESSION_LIST_STATEMENT, self.my_parser.language_level);
        }

        expression_statement.set_custom_edge_token_binders(builder, None, Some(Binder::DefaultRight));
    }

    /// `parseForEachFromColon(builder, statement, foreachStatement)` (StatementParser.kt:487).
    fn parse_for_each_from_colon(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        statement: Marker,
        foreach_statement: NodeKind,
    ) -> Marker {
        builder.advance_lexer();

        if self.my_parser.expression_parser().parse(builder).is_none() {
            error(builder, message("expected.expression"));
        }

        self.parser_for_each_from_rparenth(builder, statement, foreach_statement)
    }

    /// `parserForEachFromRparenth(builder, statement, forEachType)` (StatementParser.kt:501).
    fn parser_for_each_from_rparenth(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        statement: Marker,
        for_each_type: NodeKind,
    ) -> Marker {
        if expect_or_error(builder, Some(TokenKind::RPARENTH), "expected.rparen")
            && self.parse_statement(builder).is_none()
        {
            error(builder, message("expected.statement"));
        }

        done(builder, statement, for_each_type, self.my_parser.language_level);
        statement
    }

    /// `parseDoWhileStatement(builder)` (StatementParser.kt:514).
    fn parse_do_while_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();

        let body = self.parse_statement(builder);
        if body.is_none() {
            error(builder, message("expected.statement"));
        } else if !builder.expect(Some(TokenKind::WHILE_KEYWORD)) {
            error(builder, message("expected.while"));
        } else if self.parse_expr_in_parenth(builder) {
            semicolon(builder);
        }

        done(builder, statement, NodeKind::DO_WHILE_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseSwitchStatement(builder)` (StatementParser.kt:533).
    fn parse_switch_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_expr_in_parenth_with_block(builder, NodeKind::SWITCH_STATEMENT, true)
    }

    /// `parseCaseLabel(builder)` (StatementParser.kt:541): the marker and whether it contains an expression.
    pub fn parse_case_label(&self, builder: &mut SyntaxTreeBuilder<'_>) -> (Option<Marker>, Option<bool>) {
        if builder.token_type() == Some(TokenKind::DEFAULT_KEYWORD) {
            let default_element = builder.mark();
            builder.advance_lexer();
            done(builder, default_element, NodeKind::DEFAULT_CASE_LABEL_ELEMENT, self.my_parser.language_level);
            return (Some(default_element), Some(false));
        }
        if self.my_parser.pattern_parser().is_pattern(builder) {
            let pattern = self.my_parser.pattern_parser().parse_pattern(builder);
            return (Some(pattern), Some(false));
        }
        (self.my_parser.expression_parser().parse_assignment_forbidding_lambda(builder), Some(true))
    }

    /// `parseSwitchLabelStatement(builder)` (StatementParser.kt:555).
    fn parse_switch_label_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        let is_case = builder.token_type() == Some(TokenKind::CASE_KEYWORD);
        builder.advance_lexer();

        if is_case {
            let patterns_allowed = JavaFeature::PATTERNS_IN_SWITCH.is_sufficient(self.language_level());
            let list = builder.mark();
            loop {
                let marker_and_is_expression = self.parse_case_label(builder);
                let case_label = marker_and_is_expression.0;
                if case_label.is_none() {
                    error(
                        builder,
                        message(if patterns_allowed { "expected.case.label.element" } else { "expected.expression" }),
                    );
                }
                if !builder.expect(Some(TokenKind::COMMA)) {
                    break;
                }
            }
            done(builder, list, NodeKind::CASE_LABEL_ELEMENT_LIST, self.my_parser.language_level);
            self.parse_guard(builder);
        }

        if builder.expect(Some(TokenKind::ARROW)) {
            if builder.token_type() == Some(TokenKind::LBRACE) {
                let body = builder.mark();
                self.parse_code_block_with(builder, true);
                body.done(builder, NodeKind::BLOCK_STATEMENT);
                if builder.token_type() == Some(TokenKind::SEMICOLON) {
                    let mark = builder.mark();
                    while builder.token_type() == Some(TokenKind::SEMICOLON) {
                        builder.advance_lexer();
                    }
                    mark.error(builder, message("expected.switch.label"));
                }
            } else if builder.token_type() == Some(TokenKind::THROW_KEYWORD) {
                self.parse_throw_statement(builder);
            } else if let Some(expr) = self.my_parser.expression_parser().parse(builder) {
                let body = expr.precede(builder);
                semicolon(builder);
                body.done(builder, NodeKind::EXPRESSION_STATEMENT);
            } else {
                error(builder, message("expected.switch.rule"));
                builder.expect(Some(TokenKind::SEMICOLON));
            }
            done(builder, statement, NodeKind::SWITCH_LABELED_RULE, self.my_parser.language_level);
        } else {
            expect_or_error(builder, Some(TokenKind::COLON), "expected.colon.or.arrow");
            done(builder, statement, NodeKind::SWITCH_LABEL_STATEMENT, self.my_parser.language_level);
        }

        statement
    }

    /// `parseGuard(builder)` (StatementParser.kt:609).
    fn parse_guard(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        if builder.token_type() == Some(TokenKind::IDENTIFIER)
            && builder.token_text() == Some(java_keywords::WHEN.as_bytes())
        {
            builder.remap_current_token(TokenKind::WHEN_KEYWORD);
            builder.advance_lexer();
            let guarding_expression = self.my_parser.expression_parser().parse_assignment_forbidding_lambda(builder);
            if guarding_expression.is_none() {
                error(builder, message("expected.expression"));
            }
        }
    }

    /// `parseBreakStatement(builder)` (StatementParser.kt:620).
    fn parse_break_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();
        builder.expect(Some(TokenKind::IDENTIFIER));
        semicolon(builder);
        done(builder, statement, NodeKind::BREAK_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseYieldStatement(builder)` (StatementParser.kt:629).
    fn parse_yield_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.remap_current_token(TokenKind::YIELD_KEYWORD);
        builder.advance_lexer();

        if self.my_parser.expression_parser().parse(builder).is_none() {
            error(builder, message("expected.expression"));
        } else {
            semicolon(builder);
        }

        done(builder, statement, NodeKind::YIELD_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseContinueStatement(builder)` (StatementParser.kt:645).
    fn parse_continue_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();
        builder.expect(Some(TokenKind::IDENTIFIER));
        semicolon(builder);
        done(builder, statement, NodeKind::CONTINUE_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseReturnStatement(builder)` (StatementParser.kt:654).
    fn parse_return_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();
        self.my_parser.expression_parser().parse(builder);
        semicolon(builder);
        done(builder, statement, NodeKind::RETURN_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseThrowStatement(builder)` (StatementParser.kt:663).
    fn parse_throw_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();

        if self.my_parser.expression_parser().parse(builder).is_none() {
            error(builder, message("expected.expression"));
        } else {
            semicolon(builder);
        }

        done(builder, statement, NodeKind::THROW_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseSynchronizedStatement(builder)` (StatementParser.kt:678).
    fn parse_synchronized_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_expr_in_parenth_with_block(builder, NodeKind::SYNCHRONIZED_STATEMENT, true)
    }

    /// `parseTryStatement(builder)` (StatementParser.kt:682).
    fn parse_try_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();

        let has_resource_list = builder.token_type() == Some(TokenKind::LPARENTH);
        if has_resource_list {
            self.my_parser.declaration_parser().parse_resource_list(builder);
        }

        let try_block = self.parse_code_block_with(builder, true);
        if try_block.is_none() {
            error(builder, message("expected.lbrace"));
        } else if !has_resource_list && !TRY_CLOSERS_SET.contains(builder.token_type()) {
            error(builder, message("expected.catch.or.finally"));
        } else {
            while builder.token_type() == Some(TokenKind::CATCH_KEYWORD) {
                if !self.parse_catch_block(builder) {
                    break;
                }
            }

            if builder.expect(Some(TokenKind::FINALLY_KEYWORD)) {
                let finally_block = self.parse_code_block_with(builder, true);
                if finally_block.is_none() {
                    error(builder, message("expected.lbrace"));
                }
            }
        }

        done(builder, statement, NodeKind::TRY_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseCatchBlock(builder)` (StatementParser.kt:715). The Kotlin `require`s the `catch` every caller is at; here
    /// a debug assertion.
    pub fn parse_catch_block(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        let token_type = builder.token_type();
        debug_assert_eq!(token_type, Some(TokenKind::CATCH_KEYWORD));
        let section = builder.mark();
        builder.advance_lexer();

        if !builder.expect(Some(TokenKind::LPARENTH)) {
            error(builder, message("expected.lparen"));
            done(builder, section, NodeKind::CATCH_SECTION, self.my_parser.language_level);
            return false;
        }

        let param = self.my_parser.declaration_parser().parse_parameter(builder, false, true, false);
        if param.is_none() {
            error(builder, message("expected.parameter"));
        }

        if !builder.expect(Some(TokenKind::RPARENTH)) {
            error(builder, message("expected.rparen"));
            done(builder, section, NodeKind::CATCH_SECTION, self.my_parser.language_level);
            return false;
        }

        let body = self.parse_code_block_with(builder, true);
        if body.is_none() {
            error(builder, message("expected.lbrace"));
            done(builder, section, NodeKind::CATCH_SECTION, self.my_parser.language_level);
            return false;
        }

        done(builder, section, NodeKind::CATCH_SECTION, self.my_parser.language_level);
        true
    }

    /// `parseAssertStatement(builder)` (StatementParser.kt:748).
    fn parse_assert_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();

        if self.my_parser.expression_parser().parse(builder).is_none() {
            error(builder, message("expected.boolean.expression"));
        } else if builder.expect(Some(TokenKind::COLON)) && self.my_parser.expression_parser().parse(builder).is_none()
        {
            error(builder, message("expected.expression"));
        } else {
            semicolon(builder);
        }

        done(builder, statement, NodeKind::ASSERT_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseBlockStatement(builder)` (StatementParser.kt:766).
    fn parse_block_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        self.parse_code_block_with(builder, true);
        done(builder, statement, NodeKind::BLOCK_STATEMENT, self.my_parser.language_level);
        statement
    }

    /// `parseExprInParenthWithBlock(builder, type, block)` (StatementParser.kt:773).
    pub fn parse_expr_in_parenth_with_block(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        element_type: NodeKind,
        block: bool,
    ) -> Marker {
        let statement = builder.mark();
        builder.advance_lexer();

        if self.parse_expr_in_parenth(builder) {
            let body = if block { self.parse_code_block_with(builder, true) } else { self.parse_statement(builder) };
            if body.is_none() {
                error(builder, message(if block { "expected.lbrace" } else { "expected.statement" }));
            }
        }

        done(builder, statement, element_type, self.my_parser.language_level);
        statement
    }

    /// `parseExprInParenth(builder)` (StatementParser.kt:788).
    fn parse_expr_in_parenth(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        if !builder.expect(Some(TokenKind::LPARENTH)) {
            error(builder, message("expected.lparen"));
            return false;
        }

        let before_expr = builder.mark();
        let expr = self.my_parser.expression_parser().parse(builder);
        if expr.is_none() || builder.token_type() == Some(TokenKind::SEMICOLON) {
            before_expr.rollback_to(builder);
            error(builder, message("expected.expression"));
            if builder.token_type() != Some(TokenKind::RPARENTH) {
                return false;
            }
        } else {
            before_expr.drop(builder);
            if builder.token_type() != Some(TokenKind::RPARENTH) {
                error(builder, message("expected.rparen"));
                return false;
            }
        }

        builder.advance_lexer();
        true
    }

    /// `skipQualifiedName(builder)` (StatementParser.kt:815).
    fn skip_qualified_name(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        if !builder.expect(Some(TokenKind::IDENTIFIER)) {
            return;
        }
        while builder.token_type() == Some(TokenKind::DOT) && builder.look_ahead(1) == Some(TokenKind::IDENTIFIER) {
            builder.advance(2);
        }
    }

    /// `getLastToken(builder)` (StatementParser.kt:822).
    fn get_last_token(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<TokenKind> {
        let mut offset = -1;
        loop {
            let token = builder.raw_lookup(offset);
            if !JAVA_COMMENT_OR_WHITESPACE_BIT_SET.contains(token) {
                return token;
            }
            offset -= 1;
        }
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
