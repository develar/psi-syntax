//! `LanguageParser_Patterns.cs`: patterns, switch expressions (lane C). Every method in C# order; see `parser/mod.rs`
//! for the conventions and `expressions.rs` for how local functions and argument order are kept.

use super::*;
use crate::syntax::types::{
    ConstantPatternSyntax, DiscardPatternSyntax, IdentifierNameSyntax, MemberAccessExpressionSyntax, QualifiedNameSyntax,
    SubpatternSyntax, TypePatternSyntax,
};

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private CSharpSyntaxNode ParseTypeOrPatternForIsOperator()` (LanguageParser_Patterns.cs:21): parses the type,
    /// or pattern, right-hand operand of an is expression. Priority is the TypeSyntax. It may return a TypeSyntax
    /// which turns out in binding to be a constant pattern such as enum 'Days.Sunday'. We handle such cases in the
    /// binder of the is operator. Note that the syntax `_` will be parsed as a type.
    pub(crate) fn parse_type_or_pattern_for_is_operator(&mut self) -> Green {
        let pattern = self.parse_pattern(Self::get_precedence(SyntaxKind::IsPatternExpression), true, false);
        match self.arena_ref().kind(pattern) {
            SyntaxKind::ConstantPattern => {
                let expression = self.arena_ref().slot(pattern, ConstantPatternSyntax::EXPRESSION);
                let mut type_ = Green::NULL;
                if self.convert_expression_to_type(expression, &mut type_) { type_ } else { pattern }
            }
            SyntaxKind::TypePattern => self.arena_ref().slot(pattern, TypePatternSyntax::TYPE),
            SyntaxKind::DiscardPattern => {
                let underscore = self.arena_ref().slot(pattern, DiscardPatternSyntax::UNDERSCORE_TOKEN);
                let identifier = self.convert_to_identifier(underscore);
                self.arena().identifier_name(identifier)
            }
            _ => pattern,
        }
    }

    /// `private bool ConvertExpressionToType(ExpressionSyntax expression, [NotNullWhen(true)] out NameSyntax? type)` (LanguageParser_Patterns.cs:33).
    /// Roslyn recurses down the member accesses of `a.b.c`; this walks them, then builds the qualified names from
    /// the innermost out as the recursion returns.
    pub(crate) fn convert_expression_to_type(&mut self, expression: Green, type_: &mut Green) -> bool {
        // the member accesses on the left spine, outermost first
        let mut accesses = Vec::new();
        let mut current = expression;
        let base = loop {
            let a = self.arena_ref();
            match a.kind(current) {
                SyntaxKind::IdentifierName | SyntaxKind::GenericName | SyntaxKind::AliasQualifiedName => break current,
                SyntaxKind::SimpleMemberAccessExpression | SyntaxKind::PointerMemberAccessExpression => {
                    let operator_token = a.slot(current, MemberAccessExpressionSyntax::OPERATOR_TOKEN);
                    if a.kind(operator_token) != SyntaxKind::DotToken {
                        *type_ = Green::NULL;
                        return false;
                    }
                    accesses.push(current);
                    current = a.slot(current, MemberAccessExpressionSyntax::EXPRESSION);
                }
                _ => {
                    *type_ = Green::NULL;
                    return false;
                }
            }
        };

        let mut left_type = base;
        while let Some(access) = accesses.pop() {
            let a = self.arena_ref();
            let dot_token = a.slot(access, MemberAccessExpressionSyntax::OPERATOR_TOKEN);
            let simple_name = a.slot(access, MemberAccessExpressionSyntax::NAME);
            left_type = self.arena().qualified_name(left_type, dot_token, simple_name);
        }
        *type_ = left_type;
        true
    }

    /// `private PatternSyntax ParsePattern(Precedence precedence, bool afterIs = false, bool inSwitchArmPattern = false)` (LanguageParser_Patterns.cs:53).
    pub(crate) fn parse_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        self.parse_disjunctive_pattern(precedence, after_is, in_switch_arm_pattern)
    }

    /// `private PatternSyntax ParseDisjunctivePattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:58).
    pub(crate) fn parse_disjunctive_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        let mut result = self.parse_conjunctive_pattern(precedence, after_is, in_switch_arm_pattern);
        while self.current_contextual_kind() == SyntaxKind::OrKeyword {
            let token = self.eat_token();
            let operator_token = self.convert_to_keyword(token);
            let right = self.parse_conjunctive_pattern(precedence, after_is, in_switch_arm_pattern);
            result = self.arena().binary_pattern(SyntaxKind::OrPattern, result, operator_token, right);
        }

        result
    }

    /// `private bool LooksLikeTypeOfPattern()` (LanguageParser_Patterns.cs:76): given tk, the type of the current
    /// token, does this look like the type of a pattern?
    pub(crate) fn looks_like_type_of_pattern(&mut self) -> bool {
        let tk = self.current_kind();
        if facts::is_predefined_type(tk) {
            return true;
        }

        if tk == SyntaxKind::IdentifierToken
            && self.current_contextual_kind() != SyntaxKind::UnderscoreToken
            && (self.current_contextual_kind() != SyntaxKind::NameOfKeyword
                || self.peek_kind(1) != SyntaxKind::OpenParenToken)
        {
            return true;
        }

        if self.looks_like_tuple_array_type() {
            return true;
        }

        // We'll parse the function pointer, but issue an error in semantic analysis
        if self.is_function_pointer_start() {
            return true;
        }

        false
    }

    /// `private PatternSyntax ParseConjunctivePattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:104).
    pub(crate) fn parse_conjunctive_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        let mut result = self.parse_negated_pattern(precedence, after_is, in_switch_arm_pattern);
        while self.current_contextual_kind() == SyntaxKind::AndKeyword {
            let token = self.eat_token();
            let operator_token = self.convert_to_keyword(token);
            let right = self.parse_negated_pattern(precedence, after_is, in_switch_arm_pattern);
            result = self.arena().binary_pattern(SyntaxKind::AndPattern, result, operator_token, right);
        }

        result
    }

    /// `private bool ScanDesignation(bool permitTuple)` (LanguageParser_Patterns.cs:119). Recursive on nested
    /// parentheses, which Roslyn leaves unguarded: the recursion depth counts each level as `ParseSubExpression` does.
    pub(crate) fn scan_designation(&mut self, permit_tuple: bool) -> bool {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.scan_designation_core(permit_tuple);
        self.recursion_depth -= 1;
        result
    }

    fn scan_designation_core(&mut self, permit_tuple: bool) -> bool {
        match self.current_kind() {
            SyntaxKind::IdentifierToken => {
                let result = self.is_true_identifier();
                self.eat_token();
                result
            }
            SyntaxKind::OpenParenToken => {
                if !permit_tuple {
                    return false;
                }

                let mut saw_comma = false;
                loop {
                    self.eat_token(); // consume the `(` or `,`
                    if !self.scan_designation(true) {
                        return false;
                    }
                    match self.current_kind() {
                        SyntaxKind::CloseParenToken => {
                            self.eat_token();
                            return saw_comma;
                        }
                        SyntaxKind::CommaToken => {
                            saw_comma = true;
                            continue;
                        }
                        _ => return false,
                    }
                }
            }
            _ => false,
        }
    }

    /// `private PatternSyntax ParseNegatedPattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:158).
    /// Every pattern is parsed through here: nested patterns (`not not ...`, `[[...]]`, `{ P: { ... } }`, `((...))`)
    /// recurse with no `ParseSubExpression` between, which Roslyn leaves unguarded, so the recursion depth counts
    /// each level here.
    pub(crate) fn parse_negated_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.parse_negated_pattern_core(precedence, after_is, in_switch_arm_pattern);
        self.recursion_depth -= 1;
        result
    }

    fn parse_negated_pattern_core(&mut self, precedence: Precedence, after_is: bool, in_switch_arm_pattern: bool) -> Green {
        if self.current_contextual_kind() == SyntaxKind::NotKeyword {
            let token = self.eat_token();
            let operator_token = self.convert_to_keyword(token);
            let pattern = self.parse_negated_pattern(precedence, after_is, in_switch_arm_pattern);
            self.arena().unary_pattern(operator_token, pattern)
        } else if self.current_kind() == SyntaxKind::EqualsEqualsToken {
            let token = self.eat_token();
            let skipped_syntax = self.add_error(token, ErrorCode::ERR_EqualityOperatorInPatternNotSupported);
            let node = self.parse_negated_pattern(precedence, after_is, in_switch_arm_pattern);
            self.add_leading_skipped_syntax(node, skipped_syntax)
        } else if self.current_kind() == SyntaxKind::ExclamationEqualsToken {
            // Handle != in patterns - suggest using 'not' instead
            let not_keyword = self.arena().missing_token(SyntaxKind::NotKeyword);
            let token = self.eat_token();
            let skipped = self.add_error(token, ErrorCode::ERR_InequalityOperatorInPatternNotSupported);
            let operator_token = self.add_trailing_skipped_syntax(not_keyword, skipped);
            let pattern = self.parse_negated_pattern(precedence, after_is, in_switch_arm_pattern);
            self.arena().unary_pattern(operator_token, pattern)
        } else {
            self.parse_primary_pattern(precedence, after_is, in_switch_arm_pattern)
        }
    }

    /// `private PatternSyntax ParsePrimaryPattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:187).
    pub(crate) fn parse_primary_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // handle common error recovery situations during typing
        let tk = self.current_kind();
        if matches!(
            tk,
            SyntaxKind::CommaToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::EqualsGreaterThanToken
        ) {
            let expression = self.parse_identifier_name(ErrorCode::ERR_MissingPattern);
            return self.arena().constant_pattern(expression);
        }

        if self.current_contextual_kind() == SyntaxKind::UnderscoreToken {
            let underscore = self.eat_contextual_token(SyntaxKind::UnderscoreToken);
            return self.arena().discard_pattern(underscore);
        }

        match self.current_kind() {
            SyntaxKind::OpenBracketToken => return self.parse_list_pattern(in_switch_arm_pattern),
            SyntaxKind::DotToken if self.is_at_dot_dot_token() => {
                let dot_dot = self.eat_dot_dot_token();
                let pattern = if self.is_possible_subpattern_element() {
                    self.parse_pattern(precedence, false, in_switch_arm_pattern)
                } else {
                    Green::NULL
                };
                return self.arena().slice_pattern(dot_dot, pattern);
            }
            SyntaxKind::LessThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken => {
                // this is a relational pattern.
                let operator_token = self.eat_token();
                let expression = self.parse_sub_expression(Precedence::RELATIONAL);
                return self.arena().relational_pattern(operator_token, expression);
            }
            _ => {}
        }

        let mut reset_point = self.get_disposable_reset_point(false);

        let mut type_ = Green::NULL;
        if self.looks_like_type_of_pattern() {
            type_ = self.parse_type(if after_is { ParseTypeMode::AfterIs } else { ParseTypeMode::DefinitePattern });
            if self.arena_ref().is_missing(type_) || !self.can_token_follow_type_in_pattern(precedence) {
                // either it is not shaped like a type, or it is a constant expression.
                self.reset_disposable(&mut reset_point);
                type_ = Green::NULL;
            }
        }

        let pattern = self.parse_pattern_continued(type_, precedence, in_switch_arm_pattern);
        if !pattern.is_null() {
            self.dispose_reset_point(reset_point);
            return pattern;
        }

        self.reset_disposable(&mut reset_point);
        let value = self.parse_sub_expression(precedence);
        let result = self.arena().constant_pattern(value);
        self.dispose_reset_point(reset_point);
        result
    }

    /// `bool CanTokenFollowTypeInPattern(Precedence precedence)` (LanguageParser_Patterns.cs:257): is the current
    /// token something that could follow a type in a pattern?
    pub(crate) fn can_token_follow_type_in_pattern(&mut self, precedence: Precedence) -> bool {
        match self.current_kind() {
            SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::IdentifierToken
            | SyntaxKind::CloseBraceToken // for efficiency, test some tokens that can follow a type pattern
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CommaToken
            | SyntaxKind::SemicolonToken => true,
            // int.MaxValue is an expression, not a type.
            SyntaxKind::DotToken => false,
            // parse as an expression for error recovery
            SyntaxKind::MinusGreaterThanToken | SyntaxKind::ExclamationToken => false,
            // If we find what looks like a continuation of an expression, it is not a type.
            kind => {
                !facts::is_binary_expression_operator_token(kind)
                    || Self::get_precedence(facts::get_binary_expression(kind)) <= precedence
            }
        }
    }

    /// `private PatternSyntax? ParsePatternContinued(TypeSyntax? type, Precedence precedence, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:284):
    /// null lets the caller fall back to parsing an expression.
    pub(crate) fn parse_pattern_continued(
        &mut self,
        type_: Green,
        precedence: Precedence,
        in_switch_arm_pattern: bool,
    ) -> Green {
        if self.arena_ref().kind(type_) == SyntaxKind::IdentifierName {
            let type_identifier_token = self.arena_ref().slot(type_, IdentifierNameSyntax::IDENTIFIER);
            if self.arena_ref().contextual_kind(type_identifier_token) == SyntaxKind::VarKeyword
                && (self.current_kind() == SyntaxKind::OpenParenToken
                    || self.is_valid_pattern_designation(in_switch_arm_pattern))
            {
                // we have a "var" pattern; "var" is not permitted to be a stand-in for a type (or a constant) in a pattern.
                let var_token = self.convert_to_keyword(type_identifier_token);
                let var_designation = self.parse_designation(true);
                return self.arena().var_pattern(var_token, var_designation);
            }
        }

        if self.current_kind() == SyntaxKind::OpenParenToken && (!type_.is_null() || !looks_like_cast(self, in_switch_arm_pattern))
        {
            // It is possible this is a parenthesized (constant) expression.
            // We normalize later.
            let mut open_paren_token = self.eat_token_kind(SyntaxKind::OpenParenToken);
            let sub_patterns = self.parse_comma_separated_syntax_list(
                &mut open_paren_token,
                SyntaxKind::CloseParenToken,
                |p| p.is_possible_subpattern_element(),
                |p| p.parse_subpattern_element(),
                |p, open, list, expected, close| p.skip_bad_pattern_list_tokens(open, list, expected, close),
                false,
                false,
                false,
            );
            let close_paren_token = self.eat_token_kind(SyntaxKind::CloseParenToken);

            let mut property_pattern_clause0 = Green::NULL;
            parse_property_pattern_clause(self, in_switch_arm_pattern, &mut property_pattern_clause0);
            let designation0 = self.try_parse_simple_designation(in_switch_arm_pattern);

            // `subPatterns.Count == 1 && subPatterns.SeparatorCount == 0`: a single element, no list
            if type_.is_null()
                && property_pattern_clause0.is_null()
                && designation0.is_null()
                && !sub_patterns.is_null()
                && !sub_patterns.is_list()
            {
                let first_sub_pattern = sub_patterns;

                if self.arena_ref().slot(first_sub_pattern, SubpatternSyntax::EXPRESSION_COLON).is_null() {
                    let subpattern = self.arena_ref().slot(first_sub_pattern, SubpatternSyntax::PATTERN);
                    if self.arena_ref().kind(subpattern) == SyntaxKind::ConstantPattern {
                        // There is an ambiguity between a positional pattern `(` pattern `)`
                        // and a constant expression pattern that happens to be parenthesized.
                        // Per 2017-11-20 LDM we treat such syntax as a parenthesized expression always.
                        let cp_expression = self.arena_ref().slot(subpattern, ConstantPatternSyntax::EXPRESSION);
                        let expression =
                            self.arena().parenthesized_expression(open_paren_token, cp_expression, close_paren_token);
                        let expression = self.parse_expression_continued(expression, precedence);
                        return self.arena().constant_pattern(expression);
                    } else {
                        return self.arena().parenthesized_pattern(open_paren_token, subpattern, close_paren_token);
                    }
                }
            }

            let a = self.arena();
            let positional_pattern_clause = a.positional_pattern_clause(open_paren_token, sub_patterns, close_paren_token);
            return a.recursive_pattern(type_, positional_pattern_clause, property_pattern_clause0, designation0);
        }

        let mut property_pattern_clause = Green::NULL;
        if parse_property_pattern_clause(self, in_switch_arm_pattern, &mut property_pattern_clause) {
            let designation = self.try_parse_simple_designation(in_switch_arm_pattern);
            return self.arena().recursive_pattern(type_, Green::NULL, property_pattern_clause, designation);
        }

        if !type_.is_null() {
            let designation = self.try_parse_simple_designation(in_switch_arm_pattern);
            if !designation.is_null() {
                return self.arena().declaration_pattern(type_, designation);
            }

            // We normally prefer an expression rather than a type in a pattern.
            let mut expression = Green::NULL;
            return if self.convert_type_to_expression(type_, &mut expression, false) {
                let expression = self.parse_expression_continued(expression, precedence);
                self.arena().constant_pattern(expression)
            } else {
                self.arena().type_pattern(type_)
            };
        }

        // let the caller fall back to parsing an expression
        Green::NULL
    }
}

/// `parsePropertyPatternClause` of `ParsePatternContinued`.
fn parse_property_pattern_clause(
    p: &mut LanguageParser<'_, '_>,
    in_switch_arm_pattern: bool,
    property_pattern_clause_result: &mut Green,
) -> bool {
    // Check for the `id {` and report that the designator has to come after the property pattern.
    let misplaced_identifier = if p.is_true_identifier()
        && p.is_valid_pattern_designation(in_switch_arm_pattern)
        && p.peek_kind(1) == SyntaxKind::OpenBraceToken
    {
        let token = p.eat_token();
        p.add_error(token, ErrorCode::ERR_DesignatorBeforePropertyPattern)
    } else {
        Green::NULL
    };

    if p.current_kind() == SyntaxKind::OpenBraceToken {
        let clause = p.parse_property_pattern_clause();
        *property_pattern_clause_result = p.add_leading_skipped_syntax(clause, misplaced_identifier);
        return true;
    }

    *property_pattern_clause_result = Green::NULL;
    false
}

/// `looksLikeCast` of `ParsePatternContinued`.
fn looks_like_cast(p: &mut LanguageParser<'_, '_>, in_switch_arm_pattern: bool) -> bool {
    let point = p.get_disposable_reset_point(true);
    let result = p.scan_cast(true, in_switch_arm_pattern);
    p.dispose_reset_point(point);
    result
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private VariableDesignationSyntax? TryParseSimpleDesignation(bool whenIsKeyword)` (LanguageParser_Patterns.cs:400).
    pub(crate) fn try_parse_simple_designation(&mut self, when_is_keyword: bool) -> Green {
        if self.is_true_identifier() && self.is_valid_pattern_designation(when_is_keyword) {
            self.parse_simple_designation()
        } else {
            Green::NULL
        }
    }

    /// `private bool IsValidPatternDesignation(bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:407).
    pub(crate) fn is_valid_pattern_designation(&mut self, in_switch_arm_pattern: bool) -> bool {
        if self.current_kind() == SyntaxKind::IdentifierToken {
            match self.current_contextual_kind() {
                SyntaxKind::WhenKeyword => {
                    // When directly in a switch arm, we *always* treat 'when' as a keyword starting the 'when
                    // clause'.  In other patterns, we allow 'when' to be a normal designator.
                    return !in_switch_arm_pattern;
                }
                SyntaxKind::AndKeyword | SyntaxKind::OrKeyword => {
                    let tk = self.peek_kind(1);
                    return match tk {
                        SyntaxKind::CloseBraceToken
                        | SyntaxKind::CloseBracketToken
                        | SyntaxKind::CloseParenToken
                        | SyntaxKind::CommaToken
                        | SyntaxKind::SemicolonToken
                        | SyntaxKind::QuestionToken
                        | SyntaxKind::ColonToken => true,
                        SyntaxKind::LessThanEqualsToken
                        | SyntaxKind::LessThanToken
                        | SyntaxKind::GreaterThanEqualsToken
                        | SyntaxKind::GreaterThanToken
                        | SyntaxKind::IdentifierToken
                        | SyntaxKind::OpenBraceToken
                        | SyntaxKind::OpenParenToken
                        | SyntaxKind::OpenBracketToken => {
                            // these all can start a pattern
                            false
                        }
                        _ => {
                            if facts::is_binary_expression(tk) {
                                return true; // `e is int and && true` is valid C# 7.0 code with `and` being a designator
                            }

                            // If the following token could start an expression, it may be a constant pattern after a combinator.
                            let point = self.get_disposable_reset_point(true);
                            self.eat_token();
                            let result = !self.can_start_expression();
                            self.dispose_reset_point(point);
                            result
                        }
                    };
                }
                // UnderscoreToken: discard is a valid pattern designation
                _ => return true,
            }
        }

        false
    }

    /// `private CSharpSyntaxNode ParseExpressionOrPatternForSwitchStatement()` (LanguageParser_Patterns.cs:459).
    pub(crate) fn parse_expression_or_pattern_for_switch_statement(&mut self) -> Green {
        let saved_state = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_EXPRESSION_OR_PATTERN_IN_CASE_LABEL_OF_SWITCH_STATEMENT;
        let pattern = self.parse_pattern(Precedence::CONDITIONAL, false, true);
        self.term_state = saved_state;
        self.convert_pattern_to_expression_if_possible(pattern, false)
    }

    /// `private CSharpSyntaxNode ConvertPatternToExpressionIfPossible(PatternSyntax pattern, bool permitTypeArguments = false)` (LanguageParser_Patterns.cs:468).
    pub(crate) fn convert_pattern_to_expression_if_possible(
        &mut self,
        pattern: Green,
        permit_type_arguments: bool,
    ) -> Green {
        match self.arena_ref().kind(pattern) {
            SyntaxKind::ConstantPattern => self.arena_ref().slot(pattern, ConstantPatternSyntax::EXPRESSION),
            SyntaxKind::TypePattern => {
                let type_ = self.arena_ref().slot(pattern, TypePatternSyntax::TYPE);
                let mut expr = Green::NULL;
                if self.convert_type_to_expression(type_, &mut expr, permit_type_arguments) { expr } else { pattern }
            }
            SyntaxKind::DiscardPattern => {
                let underscore = self.arena_ref().slot(pattern, DiscardPatternSyntax::UNDERSCORE_TOKEN);
                let identifier = self.convert_to_identifier(underscore);
                self.arena().identifier_name(identifier)
            }
            _ => pattern,
        }
    }

    /// `private bool ConvertTypeToExpression(TypeSyntax type, [NotNullWhen(true)] out ExpressionSyntax? expr, bool permitTypeArguments = false)` (LanguageParser_Patterns.cs:479).
    /// Roslyn recurses down the qualified names of `A.B.C` (each inner one with `permitTypeArguments: true`, so it
    /// always converts, its left-most name kept as it is); this walks them, then builds the member accesses from the
    /// innermost out as the recursion returns.
    pub(crate) fn convert_type_to_expression(
        &mut self,
        type_: Green,
        expr: &mut Green,
        permit_type_arguments: bool,
    ) -> bool {
        let a = self.arena_ref();
        match a.kind(type_) {
            SyntaxKind::GenericName => {
                *expr = type_;
                permit_type_arguments
            }
            SyntaxKind::IdentifierName => {
                *expr = type_;
                true
            }
            SyntaxKind::QualifiedName
                if permit_type_arguments
                    || a.kind(a.slot(type_, QualifiedNameSyntax::RIGHT)) != SyntaxKind::GenericName =>
            {
                // the qualified names on the left spine, outermost first
                let mut names = Vec::new();
                let mut current = type_;
                while a.kind(current) == SyntaxKind::QualifiedName {
                    names.push(current);
                    current = a.slot(current, QualifiedNameSyntax::LEFT);
                }
                // `ConvertTypeToExpression(left, out var leftExpr, permitTypeArguments: true) ? leftExpr : left`: the
                // left-most name itself either way
                let mut new_left = current;
                while let Some(name) = names.pop() {
                    let a = self.arena_ref();
                    let dot_token = a.slot(name, QualifiedNameSyntax::DOT_TOKEN);
                    let right = a.slot(name, QualifiedNameSyntax::RIGHT);
                    new_left =
                        self.arena().member_access_expression(SyntaxKind::SimpleMemberAccessExpression, new_left, dot_token, right);
                }
                *expr = new_left;
                true
            }
            _ => {
                *expr = Green::NULL;
                false
            }
        }
    }

    /// `private bool LooksLikeTupleArrayType()` (LanguageParser_Patterns.cs:500).
    pub(crate) fn looks_like_tuple_array_type(&mut self) -> bool {
        if self.current_kind() != SyntaxKind::OpenParenToken {
            return false;
        }

        let point = self.get_disposable_reset_point(true);
        let result = self.scan_type(true) != ScanTypeFlags::NOT_TYPE;
        self.dispose_reset_point(point);
        result
    }

    /// `private PropertyPatternClauseSyntax ParsePropertyPatternClause()` (LanguageParser_Patterns.cs:509).
    pub(crate) fn parse_property_pattern_clause(&mut self) -> Green {
        let mut open_brace_token = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        let sub_patterns = self.parse_comma_separated_syntax_list(
            &mut open_brace_token,
            SyntaxKind::CloseBraceToken,
            |p| p.is_possible_subpattern_element(),
            |p| p.parse_subpattern_element(),
            |p, open, list, expected, close| p.skip_bad_pattern_list_tokens(open, list, expected, close),
            true,
            false,
            false,
        );

        let close_brace_token = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.arena().property_pattern_clause(open_brace_token, sub_patterns, close_brace_token)
    }

    /// `private SubpatternSyntax ParseSubpatternElement()` (LanguageParser_Patterns.cs:528).
    pub(crate) fn parse_subpattern_element(&mut self) -> Green {
        let mut expr_colon = Green::NULL;

        let mut pattern = self.parse_pattern(Precedence::CONDITIONAL, false, false);
        // If there is a colon but it's not preceded by a valid expression, leave it out to parse it as a missing comma, preserving C# 9.0 behavior.
        if self.current_kind() == SyntaxKind::ColonToken {
            let expr = self.convert_pattern_to_expression_if_possible(pattern, true);
            // `is ExpressionSyntax expr`: a pattern that did not convert is no expression
            if !expr.is_null() && !is_pattern_kind(self.arena_ref().kind(expr)) {
                let colon = self.eat_token();
                expr_colon = if self.arena_ref().kind(expr) == SyntaxKind::IdentifierName {
                    self.arena().name_colon(expr, colon)
                } else {
                    self.arena().expression_colon(expr, colon)
                };

                pattern = self.parse_pattern(Precedence::CONDITIONAL, false, false);
            }
        }

        self.arena().subpattern(expr_colon, pattern)
    }

    /// `private bool IsPossibleSubpatternElement()` (LanguageParser_Patterns.cs:552): check the next token to see if
    /// it is valid as the first token of a subpattern element. Used to assist in error recovery for subpattern lists
    /// (e.g. determining which tokens to skip) to ensure we make forward progress during recovery.
    pub(crate) fn is_possible_subpattern_element(&mut self) -> bool {
        self.can_start_expression()
            || matches!(
                self.current_kind(),
                SyntaxKind::OpenBraceToken
                    | SyntaxKind::OpenBracketToken
                    | SyntaxKind::LessThanToken
                    | SyntaxKind::LessThanEqualsToken
                    | SyntaxKind::GreaterThanToken
                    | SyntaxKind::GreaterThanEqualsToken
            )
    }

    /// `private static PostSkipAction SkipBadPatternListTokens<T>( LanguageParser @this, ref SyntaxToken open, SeparatedSyntaxListBuilder<T> list, SyntaxKind expectedKind, SyntaxKind closeKind) where T : CSharpSyntaxNode` (LanguageParser_Patterns.cs:564).
    pub(crate) fn skip_bad_pattern_list_tokens(
        &mut self,
        open: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        if matches!(
            self.current_kind(),
            SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::SemicolonToken
        ) {
            return PostSkipAction::Abort;
        }

        // `:` is usually treated as incorrect separation token. This helps for error recovery in basic typing scenarios like `{ Prop:$$ Prop1: { ... } }`.
        // However, such behavior isn't much desirable when parsing pattern of a case label in a switch statement. For instance, consider the following example: `case { Prop: { }: case ...`.
        // Normally we would skip second `:` and `case` keyword after it as bad tokens and continue parsing pattern, which produces a lot of noise errors.
        // In order to avoid that and produce single error of missing `}` we exit on unexpected `:` in such cases.
        if self.term_state.contains(TerminatorState::IS_EXPRESSION_OR_PATTERN_IN_CASE_LABEL_OF_SWITCH_STATEMENT)
            && self.current_kind() == SyntaxKind::ColonToken
        {
            return PostSkipAction::Abort;
        }

        // This is pretty much the same as above, but for switch expressions and `=>` and `:` tokens.
        // The reason why we cannot use single flag for both cases is because we want `=>` to be the "exit" token only for switch expressions.
        // Consider the following example: `case (() => 0):`. Normally `=>` is treated as bad separator, so we parse this basically the same as `case ((), 1):`, which is syntactically valid.
        // However, if we treated `=>` as "exit" token, parsing wouldn't consume full case label properly and would produce a lot of noise errors.
        // We can afford `:` to be the exit token for switch expressions because error recovery is already good enough and treats `:` as bad `=>`,
        // meaning that switch expression arm `{ : 1` can be recovered to `{ } => 1` where the closing `}` is missing and instead of `=>` we have `:`.
        if self.term_state.contains(TerminatorState::IS_PATTERN_IN_SWITCH_EXPRESSION_ARM)
            && matches!(self.current_kind(), SyntaxKind::EqualsGreaterThanToken | SyntaxKind::ColonToken)
        {
            return PostSkipAction::Abort;
        }

        self.skip_bad_separated_list_tokens_with_expected_kind(
            open,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_subpattern_element(),
            |p, close_kind| p.current_kind() == close_kind || p.current_kind() == SyntaxKind::SemicolonToken,
            expected_kind,
            close_kind,
        )
    }

    /// `private SwitchExpressionSyntax ParseSwitchExpression(ExpressionSyntax governingExpression, SyntaxToken switchKeyword)` (LanguageParser_Patterns.cs:593).
    pub(crate) fn parse_switch_expression(&mut self, governing_expression: Green, switch_keyword: Green) -> Green {
        // For better error recovery when an expression is typed on a line before a switch statement,
        // the caller checks if the switch keyword is followed by an open curly brace. Only if it is
        // would we attempt to parse it as a switch expression here.
        let open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        let arms = parse_switch_expression_arms(self);
        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.arena().switch_expression(governing_expression, switch_keyword, open_brace, arms, close_brace)
    }
}

/// `parseSwitchExpressionArms` of `ParseSwitchExpression`.
fn parse_switch_expression_arms(p: &mut LanguageParser<'_, '_>) -> Green {
    let mut arms = p.pool.allocate();

    while p.current_kind() != SyntaxKind::CloseBraceToken {
        // Help out in the case where a user is converting a switch statement to a switch expression. Note:
        // `default(...)` and `default` will also be consumed as a legal syntactic patterns (though the
        // latter will fail during binding).  So if the user has `default:` we will recover fine as we
        // handle the errant colon below.
        let errant_case = if p.current_kind() == SyntaxKind::CaseKeyword {
            let token = p.eat_token();
            p.add_error(token, ErrorCode::ERR_BadCaseInSwitchArm)
        } else {
            Green::NULL
        };

        let saved_state = p.term_state;
        p.term_state = p.term_state | TerminatorState::IS_PATTERN_IN_SWITCH_EXPRESSION_ARM;
        let pattern = p.parse_pattern(Precedence::COALESCING, false, true);
        p.term_state = saved_state;

        // We use a precedence that excludes lambdas, assignments, and a conditional which could have a
        // lambda on the right, because we need the parser to leave the EqualsGreaterThanToken to be
        // consumed by the switch arm. The strange side-effect of that is that the conditional expression is
        // not permitted as a constant expression here; it would have to be parenthesized.

        let when_clause = p.parse_when_clause(Precedence::COALESCING);
        // Help out in the case where a user is converting a switch statement to a switch expression.
        // Consume the `:` as a `=>` and report an error.
        let equals_greater_than = if p.current_kind() == SyntaxKind::ColonToken {
            p.eat_token_as_kind(SyntaxKind::EqualsGreaterThanToken)
        } else {
            p.eat_token_kind(SyntaxKind::EqualsGreaterThanToken)
        };
        let expression = p.parse_expression_core();
        let mut switch_expression_case = p.arena().switch_expression_arm(pattern, when_clause, equals_greater_than, expression);

        // If we're not making progress, abort
        if errant_case.is_null()
            && p.arena_ref().full_width(switch_expression_case) == 0
            && p.current_kind() != SyntaxKind::CommaToken
        {
            break;
        }

        if !errant_case.is_null() {
            switch_expression_case = p.add_leading_skipped_syntax(switch_expression_case, errant_case);
        }

        arms.push(switch_expression_case);
        if p.current_kind() != SyntaxKind::CloseBraceToken {
            let comma_token = if p.current_kind() == SyntaxKind::SemicolonToken {
                p.eat_token_as_kind(SyntaxKind::CommaToken)
            } else {
                p.eat_token_kind(SyntaxKind::CommaToken)
            };
            arms.push(comma_token);
        }
    }

    let list = p.base.arena().list(&arms);
    p.pool.free(arms);
    list
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ListPatternSyntax ParseListPattern(bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:660).
    pub(crate) fn parse_list_pattern(&mut self, in_switch_arm_pattern: bool) -> Green {
        let mut open_bracket = self.eat_token_kind(SyntaxKind::OpenBracketToken);
        let list = self.parse_comma_separated_syntax_list(
            &mut open_bracket,
            SyntaxKind::CloseBracketToken,
            |p| p.is_possible_subpattern_element(),
            |p| p.parse_pattern(Precedence::CONDITIONAL, false, false),
            |p, open, list, expected, close| p.skip_bad_pattern_list_tokens(open, list, expected, close),
            true,
            false,
            false,
        );

        let close_bracket = self.eat_token_kind(SyntaxKind::CloseBracketToken);
        let designation = self.try_parse_simple_designation(in_switch_arm_pattern);
        self.arena().list_pattern(open_bracket, list, close_bracket, designation)
    }
}
