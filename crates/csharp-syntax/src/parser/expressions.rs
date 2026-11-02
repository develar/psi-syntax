//! `LanguageParser.cs` 11123-14419: expressions and their precedence, primary expressions, arguments, casts,
//! lambdas, object and collection creation, initializers, query expressions (lane C). Every method in C# order; see
//! `parser/mod.rs` for the conventions. A C# local function is a module-private function after its method, named as
//! it is; the C# arguments of a call are evaluated into locals first, left to right, as C# evaluates them.

use super::*;
use crate::green::GreenArena;
use crate::syntax::types::{
    AssignmentExpressionSyntax, AttributeListSyntax, ConditionalExpressionSyntax, ParenthesizedLambdaExpressionSyntax,
};

/// `ParserSyntaxContextResetter` (LanguageParser.cs:4344): the syntax factory context set for a scope and restored
/// by `dispose` where the C# `using` scope ends. `None` keeps the current value.
struct ParserSyntaxContextResetter {
    is_in_async: bool,
    is_in_query: bool,
    is_in_field_keyword_context: bool,
    force_conditional_access_expression: bool,
}

impl ParserSyntaxContextResetter {
    fn new(
        parser: &mut LanguageParser<'_, '_>,
        is_in_async_context: Option<bool>,
        is_in_query_context: Option<bool>,
        is_in_field_keyword_context: Option<bool>,
        force_conditional_access_expression: Option<bool>,
    ) -> ParserSyntaxContextResetter {
        let previous = ParserSyntaxContextResetter {
            is_in_async: parser.is_in_async,
            is_in_query: parser.is_in_query,
            is_in_field_keyword_context: parser.is_in_field_keyword_context,
            force_conditional_access_expression: parser.force_conditional_access_expression,
        };
        parser.is_in_async = is_in_async_context.unwrap_or(parser.is_in_async);
        parser.is_in_query = is_in_query_context.unwrap_or(parser.is_in_query);
        parser.is_in_field_keyword_context =
            is_in_field_keyword_context.unwrap_or(parser.is_in_field_keyword_context);
        parser.force_conditional_access_expression =
            force_conditional_access_expression.unwrap_or(parser.force_conditional_access_expression);
        previous
    }

    fn dispose(self, parser: &mut LanguageParser<'_, '_>) {
        parser.is_in_async = self.is_in_async;
        parser.is_in_query = self.is_in_query;
        parser.is_in_field_keyword_context = self.is_in_field_keyword_context;
        parser.force_conditional_access_expression = self.force_conditional_access_expression;
    }
}

/// The elements of a green list (`SyntaxList<T>`): none for null, the element itself when it is no list.
fn list_count(a: &GreenArena, list: Green) -> usize {
    if list.is_null() {
        0
    } else if list.is_list() {
        a.slot_count(list)
    } else {
        1
    }
}

/// `list[index]` of a green list (see `list_count`).
fn list_item(a: &GreenArena, list: Green, index: usize) -> Green {
    if list.is_list() {
        a.slot(list, index)
    } else if index == 0 {
        list
    } else {
        Green::NULL
    }
}

/// `SyntaxList<SyntaxToken>.Any(kind)`.
fn list_any(a: &GreenArena, list: Green, kind: SyntaxKind) -> bool {
    (0..list_count(a, list)).any(|i| a.kind(list_item(a, list, i)) == kind)
}

/// `_pool.ToListAndFree(builder)` (and `ToTokenListAndFree`).
fn to_list_and_free(p: &mut LanguageParser<'_, '_>, builder: SyntaxListBuilder) -> Green {
    let list = p.base.arena().list(&builder);
    p.pool.free(builder);
    list
}

/// A pattern's kind (`node is PatternSyntax`).
pub(super) fn is_pattern_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::DiscardPattern
            | SyntaxKind::DeclarationPattern
            | SyntaxKind::VarPattern
            | SyntaxKind::RecursivePattern
            | SyntaxKind::ConstantPattern
            | SyntaxKind::ParenthesizedPattern
            | SyntaxKind::RelationalPattern
            | SyntaxKind::TypePattern
            | SyntaxKind::OrPattern
            | SyntaxKind::AndPattern
            | SyntaxKind::NotPattern
            | SyntaxKind::ListPattern
            | SyntaxKind::SlicePattern
    )
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `public ExpressionSyntax ParseExpression()` (LanguageParser.cs:11123).
    pub(crate) fn parse_expression(&mut self) -> Green {
        self.parse_with_stack_guard(|this| this.parse_expression_core(), |this| this.create_missing_identifier_name())
    }

    /// `private ExpressionSyntax ParseExpressionCore()` (LanguageParser.cs:11130).
    pub(crate) fn parse_expression_core(&mut self) -> Green {
        self.parse_sub_expression(Precedence::EXPRESSION)
    }

    /// `private bool CanStartExpression()` (LanguageParser.cs:11138): is the current token one that could start an
    /// expression?
    pub(crate) fn can_start_expression(&mut self) -> bool {
        self.is_possible_expression_with_allow_assignment_expressions(false, false)
    }

    /// `private bool IsPossibleExpression()` (LanguageParser.cs:11146): is the current token one that could be in an
    /// expression?
    pub(crate) fn is_possible_expression(&mut self) -> bool {
        self.is_possible_expression_with_allow_assignment_expressions(true, true)
    }

    /// `private bool IsPossibleExpression(bool allowBinaryExpressions, bool allowAssignmentExpressions)` (LanguageParser.cs:11151).
    pub(crate) fn is_possible_expression_with_allow_assignment_expressions(
        &mut self,
        allow_binary_expressions: bool,
        allow_assignment_expressions: bool,
    ) -> bool {
        let tk = self.current_kind();
        match tk {
            SyntaxKind::TypeOfKeyword
            | SyntaxKind::DefaultKeyword
            | SyntaxKind::SizeOfKeyword
            | SyntaxKind::MakeRefKeyword
            | SyntaxKind::RefTypeKeyword
            | SyntaxKind::CheckedKeyword
            | SyntaxKind::UncheckedKeyword
            | SyntaxKind::UnsafeKeyword
            | SyntaxKind::RefValueKeyword
            | SyntaxKind::ArgListKeyword
            | SyntaxKind::BaseKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::ThisKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::OpenParenToken
            | SyntaxKind::NumericLiteralToken
            | SyntaxKind::StringLiteralToken
            | SyntaxKind::Utf8StringLiteralToken
            | SyntaxKind::SingleLineRawStringLiteralToken
            | SyntaxKind::Utf8SingleLineRawStringLiteralToken
            | SyntaxKind::MultiLineRawStringLiteralToken
            | SyntaxKind::Utf8MultiLineRawStringLiteralToken
            | SyntaxKind::InterpolatedStringToken
            | SyntaxKind::InterpolatedStringStartToken
            | SyntaxKind::InterpolatedVerbatimStringStartToken
            | SyntaxKind::InterpolatedSingleLineRawStringStartToken
            | SyntaxKind::InterpolatedMultiLineRawStringStartToken
            | SyntaxKind::CharacterLiteralToken
            | SyntaxKind::NewKeyword
            | SyntaxKind::DelegateKeyword
            | SyntaxKind::ColonColonToken // bad aliased name
            | SyntaxKind::ThrowKeyword
            | SyntaxKind::StackAllocKeyword
            | SyntaxKind::RefKeyword
            | SyntaxKind::OpenBracketToken // attributes on a lambda, or a collection expression.
            => true,
            SyntaxKind::DotToken if self.is_at_dot_dot_token() => true,
            SyntaxKind::StaticKeyword => {
                self.is_possible_anonymous_method_expression() || self.is_possible_lambda_expression(Precedence::EXPRESSION)
            }
            SyntaxKind::IdentifierToken => {
                // Specifically allow the from contextual keyword, because it can always be the start of an
                // expression (whether it is used as an identifier or a keyword).
                self.is_true_identifier() || self.current_contextual_kind() == SyntaxKind::FromKeyword
            }
            _ => {
                Self::is_predefined_type(tk)
                    || facts::is_any_unary_expression(tk)
                    || (allow_binary_expressions && facts::is_binary_expression(tk))
                    || (allow_assignment_expressions && facts::is_assignment_expression_operator_token(tk))
            }
        }
    }

    /// `private static bool IsInvalidSubExpression(SyntaxKind kind)` (LanguageParser.cs:11209).
    pub(crate) fn is_invalid_sub_expression(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::BreakKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::ForEachKeyword
                | SyntaxKind::GotoKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::LockKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::WhileKeyword
        )
    }

    /// `internal static bool IsRightAssociative(SyntaxKind op)` (LanguageParser.cs:11237).
    pub(crate) fn is_right_associative(op: SyntaxKind) -> bool {
        matches!(
            op,
            SyntaxKind::SimpleAssignmentExpression
                | SyntaxKind::AddAssignmentExpression
                | SyntaxKind::SubtractAssignmentExpression
                | SyntaxKind::MultiplyAssignmentExpression
                | SyntaxKind::DivideAssignmentExpression
                | SyntaxKind::ModuloAssignmentExpression
                | SyntaxKind::AndAssignmentExpression
                | SyntaxKind::ExclusiveOrAssignmentExpression
                | SyntaxKind::OrAssignmentExpression
                | SyntaxKind::LeftShiftAssignmentExpression
                | SyntaxKind::RightShiftAssignmentExpression
                | SyntaxKind::UnsignedRightShiftAssignmentExpression
                | SyntaxKind::CoalesceAssignmentExpression
                | SyntaxKind::CoalesceExpression
        )
    }
}

/// `Precedence` (LanguageParser.cs:11261).
/// (`enum Precedence : uint`: `Assignment` and `Lambda` alias `Expression`, so it is a number with named values.)
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) struct Precedence(pub(crate) u32);

impl Precedence {
    pub(crate) const EXPRESSION: Precedence = Precedence(0);
    pub(crate) const ASSIGNMENT: Precedence = Precedence(0);
    pub(crate) const LAMBDA: Precedence = Precedence(0);
    pub(crate) const CONDITIONAL: Precedence = Precedence(1);
    pub(crate) const COALESCING: Precedence = Precedence(2);
    pub(crate) const CONDITIONAL_OR: Precedence = Precedence(3);
    pub(crate) const CONDITIONAL_AND: Precedence = Precedence(4);
    pub(crate) const LOGICAL_OR: Precedence = Precedence(5);
    pub(crate) const LOGICAL_XOR: Precedence = Precedence(6);
    pub(crate) const LOGICAL_AND: Precedence = Precedence(7);
    pub(crate) const EQUALITY: Precedence = Precedence(8);
    pub(crate) const RELATIONAL: Precedence = Precedence(9);
    pub(crate) const SHIFT: Precedence = Precedence(10);
    pub(crate) const ADDITIVE: Precedence = Precedence(11);
    pub(crate) const MULTIPLICATIVE: Precedence = Precedence(12);
    pub(crate) const SWITCH: Precedence = Precedence(13);
    pub(crate) const RANGE: Precedence = Precedence(14);
    pub(crate) const UNARY: Precedence = Precedence(15);
    pub(crate) const CAST: Precedence = Precedence(16);
    pub(crate) const POINTER_INDIRECTION: Precedence = Precedence(17);
    pub(crate) const ADDRESS_OF: Precedence = Precedence(18);
    pub(crate) const PRIMARY: Precedence = Precedence(19);
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private static Precedence GetPrecedence(SyntaxKind op)` (LanguageParser.cs:11287). Roslyn throws on a kind it
    /// does not list, which no expression the parser makes has; here such a kind (a null node) is `Primary`.
    pub(crate) fn get_precedence(op: SyntaxKind) -> Precedence {
        match op {
            SyntaxKind::QueryExpression => Precedence::EXPRESSION,
            SyntaxKind::ParenthesizedLambdaExpression
            | SyntaxKind::SimpleLambdaExpression
            | SyntaxKind::AnonymousMethodExpression => Precedence::LAMBDA,
            SyntaxKind::SimpleAssignmentExpression
            | SyntaxKind::AddAssignmentExpression
            | SyntaxKind::SubtractAssignmentExpression
            | SyntaxKind::MultiplyAssignmentExpression
            | SyntaxKind::DivideAssignmentExpression
            | SyntaxKind::ModuloAssignmentExpression
            | SyntaxKind::AndAssignmentExpression
            | SyntaxKind::ExclusiveOrAssignmentExpression
            | SyntaxKind::OrAssignmentExpression
            | SyntaxKind::LeftShiftAssignmentExpression
            | SyntaxKind::RightShiftAssignmentExpression
            | SyntaxKind::UnsignedRightShiftAssignmentExpression
            | SyntaxKind::CoalesceAssignmentExpression => Precedence::ASSIGNMENT,
            SyntaxKind::CoalesceExpression | SyntaxKind::ThrowExpression => Precedence::COALESCING,
            SyntaxKind::LogicalOrExpression => Precedence::CONDITIONAL_OR,
            SyntaxKind::LogicalAndExpression => Precedence::CONDITIONAL_AND,
            SyntaxKind::BitwiseOrExpression => Precedence::LOGICAL_OR,
            SyntaxKind::ExclusiveOrExpression => Precedence::LOGICAL_XOR,
            SyntaxKind::BitwiseAndExpression => Precedence::LOGICAL_AND,
            SyntaxKind::EqualsExpression | SyntaxKind::NotEqualsExpression => Precedence::EQUALITY,
            SyntaxKind::LessThanExpression
            | SyntaxKind::LessThanOrEqualExpression
            | SyntaxKind::GreaterThanExpression
            | SyntaxKind::GreaterThanOrEqualExpression
            | SyntaxKind::IsExpression
            | SyntaxKind::AsExpression
            | SyntaxKind::IsPatternExpression => Precedence::RELATIONAL,
            SyntaxKind::SwitchExpression | SyntaxKind::WithExpression => Precedence::SWITCH,
            SyntaxKind::LeftShiftExpression
            | SyntaxKind::RightShiftExpression
            | SyntaxKind::UnsignedRightShiftExpression => Precedence::SHIFT,
            SyntaxKind::AddExpression | SyntaxKind::SubtractExpression => Precedence::ADDITIVE,
            SyntaxKind::MultiplyExpression | SyntaxKind::DivideExpression | SyntaxKind::ModuloExpression => {
                Precedence::MULTIPLICATIVE
            }
            SyntaxKind::UnaryPlusExpression
            | SyntaxKind::UnaryMinusExpression
            | SyntaxKind::BitwiseNotExpression
            | SyntaxKind::LogicalNotExpression
            | SyntaxKind::PreIncrementExpression
            | SyntaxKind::PreDecrementExpression
            | SyntaxKind::TypeOfExpression
            | SyntaxKind::SizeOfExpression
            | SyntaxKind::CheckedExpression
            | SyntaxKind::UncheckedExpression
            | SyntaxKind::UnsafeExpression
            | SyntaxKind::MakeRefExpression
            | SyntaxKind::RefValueExpression
            | SyntaxKind::RefTypeExpression
            | SyntaxKind::AwaitExpression
            | SyntaxKind::IndexExpression => Precedence::UNARY,
            SyntaxKind::CastExpression => Precedence::CAST,
            SyntaxKind::PointerIndirectionExpression => Precedence::POINTER_INDIRECTION,
            SyntaxKind::AddressOfExpression => Precedence::ADDRESS_OF,
            SyntaxKind::RangeExpression => Precedence::RANGE,
            SyntaxKind::ConditionalExpression => Precedence::EXPRESSION,
            // AliasQualifiedName, AnonymousObjectCreationExpression, ArgListExpression, ..., TupleExpression
            _ => Precedence::PRIMARY,
        }
    }

    /// `private static bool IsExpectedPrefixUnaryOperator(SyntaxKind kind)` (LanguageParser.cs:11420).
    pub(crate) fn is_expected_prefix_unary_operator(kind: SyntaxKind) -> bool {
        facts::is_prefix_unary_expression(kind) && !matches!(kind, SyntaxKind::RefKeyword | SyntaxKind::OutKeyword)
    }

    /// `private static bool IsExpectedBinaryOperator(SyntaxKind kind)` (LanguageParser.cs:11425).
    pub(crate) fn is_expected_binary_operator(kind: SyntaxKind) -> bool {
        facts::is_binary_expression(kind)
    }

    /// `private static bool IsExpectedAssignmentOperator(SyntaxKind kind)` (LanguageParser.cs:11430).
    pub(crate) fn is_expected_assignment_operator(kind: SyntaxKind) -> bool {
        facts::is_assignment_expression_operator_token(kind)
    }

    /// `private bool IsPossibleAwaitExpressionStatement()` (LanguageParser.cs:11435).
    pub(crate) fn is_possible_await_expression_statement(&mut self) -> bool {
        (self.is_script() || self.is_in_async) && self.current_contextual_kind() == SyntaxKind::AwaitKeyword
    }

    /// `private bool IsAwaitExpression()` (LanguageParser.cs:11440).
    pub(crate) fn is_await_expression(&mut self) -> bool {
        if self.current_contextual_kind() == SyntaxKind::AwaitKeyword {
            if self.is_in_async {
                // If we see an await in an async function, parse it as an unop.
                return true;
            }

            // If we see an await followed by a token that cannot follow an identifier, parse await as a unop.
            // BindAwait() catches the cases where await successfully parses as a unop but is not in an async
            // function, and reports an appropriate ERR_BadAwaitWithoutAsync* error.
            let next = self.peek_token(1);
            match self.arena_ref().kind(next) {
                SyntaxKind::IdentifierToken => {
                    return self.arena_ref().contextual_kind(next) != SyntaxKind::WithKeyword;
                }
                // Keywords
                SyntaxKind::NewKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::BaseKeyword
                | SyntaxKind::DelegateKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::CheckedKeyword
                | SyntaxKind::UncheckedKeyword
                | SyntaxKind::DefaultKeyword
                // Literals
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::StringLiteralToken
                | SyntaxKind::SingleLineRawStringLiteralToken
                | SyntaxKind::Utf8SingleLineRawStringLiteralToken
                | SyntaxKind::MultiLineRawStringLiteralToken
                | SyntaxKind::Utf8MultiLineRawStringLiteralToken
                | SyntaxKind::InterpolatedStringToken
                | SyntaxKind::Utf8StringLiteralToken
                | SyntaxKind::InterpolatedStringStartToken
                | SyntaxKind::InterpolatedVerbatimStringStartToken
                | SyntaxKind::InterpolatedSingleLineRawStringStartToken
                | SyntaxKind::InterpolatedMultiLineRawStringStartToken
                | SyntaxKind::NumericLiteralToken
                | SyntaxKind::NullKeyword
                | SyntaxKind::CharacterLiteralToken => return true,
                _ => {}
            }
        }

        false
    }

    /// `private ExpressionSyntax ParseSubExpression(Precedence precedence)` (LanguageParser.cs:11496): parse a
    /// subexpression of the enclosing operator of the given precedence.
    pub(crate) fn parse_sub_expression(&mut self, precedence: Precedence) -> Green {
        self.recursion_depth += 1;

        self.ensure_sufficient_execution_stack();

        let result = self.parse_sub_expression_core(precedence);
        self.recursion_depth -= 1;
        result
    }

    /// `private ExpressionSyntax ParseSubExpressionCore(Precedence precedence)` (LanguageParser.cs:11511).
    pub(crate) fn parse_sub_expression_core(&mut self, precedence: Precedence) -> Green {
        // all of these are tokens that start statements and are invalid
        // to start a expression with. if we see one, then we must have
        // something like:
        //
        // return
        // if (...
        // parse out a missing name node for the expression, and keep on going
        let tk = self.current_kind();
        if Self::is_invalid_sub_expression(tk) {
            let missing = self.create_missing_identifier_name();
            return self.add_error_args(missing, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::Str(facts::get_text(tk))]);
        }

        let unary_or_primary = parse_unary_or_primary_expression(self, tk, precedence);
        self.parse_expression_continued(unary_or_primary, precedence)
    }
}

/// `parseUnaryOrPrimaryExpression` of `ParseSubExpressionCore`: a unary expression, or something with higher
/// precedence (cast, addressof, primary).
fn parse_unary_or_primary_expression(p: &mut LanguageParser<'_, '_>, tk: SyntaxKind, precedence: Precedence) -> Green {
    // Parse a left operand -- possibly preceded by a unary operator.
    if LanguageParser::is_expected_prefix_unary_operator(tk) {
        let op_kind = facts::get_prefix_unary_expression(tk);
        let operator_token = p.eat_token();
        let operand = p.parse_sub_expression(LanguageParser::get_precedence(op_kind));
        return p.arena().prefix_unary_expression(op_kind, operator_token, operand);
    }

    // Check *explicitly* for `..` starting an expression.  This *is* the initial term we want to parse out.
    // If we have `expr..` though we don't do that here.  Instead, we'll parse out 'expr', and the `..`
    // portion will be handled in ParseExpressionContinued.
    if p.is_at_dot_dot_token() {
        let dot_dot = p.eat_dot_dot_token();
        let right = if p.can_start_expression() { p.parse_sub_expression(Precedence::RANGE) } else { Green::NULL };
        return p.arena().range_expression(Green::NULL, dot_dot, right);
    }

    if p.is_await_expression() {
        let await_keyword = p.eat_contextual_token(SyntaxKind::AwaitKeyword);
        let operand = p.parse_sub_expression(LanguageParser::get_precedence(SyntaxKind::AwaitExpression));
        return p.arena().await_expression(await_keyword, operand);
    }

    if p.is_query_expression(false, false) {
        return p.parse_query_expression(precedence);
    }

    if p.current_contextual_kind() == SyntaxKind::FromKeyword && p.is_in_query {
        // If this "from" token wasn't the start of a query then it's not really an expression.
        // Consume it so that we don't try to parse it again as the next argument in an
        // argument list.
        let missing = p.create_missing_identifier_name();
        let from = p.eat_token();
        let text = p.current_token();
        let skipped = p.add_error_args(from, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::TokenText(text)]);
        return p.add_trailing_skipped_syntax(missing, skipped);
    }

    if tk == SyntaxKind::ThrowKeyword {
        let result = p.parse_throw_expression();
        // we parse a throw expression even at the wrong precedence for better recovery
        return if precedence <= Precedence::COALESCING {
            result
        } else {
            p.add_error_args(result, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::Str(facts::get_text(tk))])
        };
    }

    if p.is_possible_deconstruction_left(precedence) {
        return p.parse_declaration_expression(ParseTypeMode::Normal, false);
    }

    // Not a unary operator - get a primary expression.
    p.parse_primary_expression(precedence)
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ExpressionSyntax ParseExpressionContinued(ExpressionSyntax unaryOrPrimaryExpression, Precedence precedence)` (LanguageParser.cs:11596):
    /// takes in an initial unary expression or primary expression, and then consumes what follows as long as its
    /// precedence is either lower than the `precedence` we're parsing currently, or equal to that precedence if we
    /// have something right-associative (`IsRightAssociative`).
    pub(crate) fn parse_expression_continued(
        &mut self,
        unary_or_primary_expression: Green,
        precedence: Precedence,
    ) -> Green {
        let mut current_expression = unary_or_primary_expression;

        // Keep on expanding the left operand as long as what we see fits the precedence we're under.
        loop {
            let expanded = try_expand_expression(self, current_expression, precedence);
            if expanded.is_null() {
                break;
            }
            current_expression = expanded;
        }

        // Finally, consume a conditional expression if precedence allows it.

        // https://github.com/dotnet/csharpstandard/blob/standard-v6/standard/expressions.md#1115-conditional-operator:
        //
        // conditional_expression
        //     : null_coalescing_expression
        //     | null_coalescing_expression '?' expression ':' expression
        //     ;
        //
        // 1. Only take the conditional part of the expression if we're at or below its precedence.
        // 2. When parsing the branches of the expression, parse at the highest precedence again ('expression').
        //    This allows for things like assignments/lambdas in the branches of the conditional.
        if self.current_kind() == SyntaxKind::QuestionToken && precedence <= Precedence::CONDITIONAL {
            return consume_conditional_expression(self, current_expression);
        }

        current_expression
    }
}

/// `tryExpandExpression` of `ParseExpressionContinued`: null when nothing expands `left_operand`.
fn try_expand_expression(p: &mut LanguageParser<'_, '_>, left_operand: Green, precedence: Precedence) -> Green {
    // Look for operators that can follow what we've seen so far, and which are acceptable at this
    // precedence level.  Examples include binary operator, assignment operators, range operators `..`, as
    // well as `switch` and `with` clauses.

    let (operator_token_kind, operator_expression_kind) = p.get_expression_operator_token_kind_and_expression_kind();

    if operator_token_kind == SyntaxKind::None {
        return Green::NULL;
    }

    let new_precedence = LanguageParser::get_precedence(operator_expression_kind);

    // Check the precedence to see if we should "take" this operator.  A lower precedence means what's
    // coming isn't a child of us, but rather we will be a child of it.  So we bail out and let any higher
    // up expression parsing consume it with us as the left side.
    if new_precedence < precedence {
        return Green::NULL;
    }

    // Same precedence, but not right-associative -- deal with this "later"
    if new_precedence == precedence && !LanguageParser::is_right_associative(operator_expression_kind) {
        return Green::NULL;
    }

    // Now consume the operator (including consuming multiple tokens in the case of merged operator tokens)
    let mut operator_token = p.eat_expression_operator_token(operator_token_kind);

    let left_kind = p.arena_ref().kind(left_operand);
    if new_precedence > LanguageParser::get_precedence(left_kind) {
        // Normally, a left operand with a looser precedence will consume all right operands that
        // have a tighter precedence.  For example, in the expression `a + b * c`, the `* c` part
        // will be consumed as part of the right operand of the addition.  However, there are a
        // few circumstances in which a tighter precedence is not consumed: that occurs when the
        // left hand operator does not have an expression as its right operand.  This occurs for
        // the is-type operator and the is-pattern operator.  Source text such as
        // `a is {} + b` should produce a syntax error, as parsing the `+` with an `is`
        // expression as its left operand would be a precedence inversion.  Similarly, it occurs
        // with an anonymous method expression or a lambda expression with a block body.  No
        // further parsing will find a way to fix things up, so we accept the operator but issue
        // a diagnostic.
        let code = if left_kind == SyntaxKind::IsPatternExpression {
            ErrorCode::ERR_UnexpectedToken
        } else {
            ErrorCode::WRN_PrecedenceInversion
        };
        operator_token = p.add_error_args(operator_token, code, &[DiagArg::TokenText(operator_token)]);
    }

    if operator_expression_kind == SyntaxKind::AsExpression {
        let right = p.parse_type(ParseTypeMode::AsExpression);
        return p.arena().binary_expression(operator_expression_kind, left_operand, operator_token, right);
    }

    if operator_expression_kind == SyntaxKind::IsExpression {
        return p.parse_is_expression(left_operand, operator_token);
    }

    if operator_expression_kind == SyntaxKind::SwitchExpression {
        return p.parse_switch_expression(left_operand, operator_token);
    }

    if operator_expression_kind == SyntaxKind::WithExpression {
        return p.parse_with_expression(left_operand, operator_token);
    }

    if operator_expression_kind == SyntaxKind::RangeExpression {
        let right = if p.can_start_expression() { p.parse_sub_expression(Precedence::RANGE) } else { Green::NULL };
        return p.arena().range_expression(left_operand, operator_token, right);
    }

    let operator_kind = p.arena_ref().kind(operator_token);
    if LanguageParser::is_expected_assignment_operator(operator_kind) {
        return p.parse_assignment_expression(operator_expression_kind, left_operand, operator_token);
    }

    if LanguageParser::is_expected_binary_operator(operator_kind) {
        let right = p.parse_sub_expression(new_precedence);
        return p.arena().binary_expression(operator_expression_kind, left_operand, operator_token, right);
    }

    // `throw ExceptionUtilities.Unreachable()`: every operator kind above is a binary or an assignment one.
    Green::NULL
}

/// `consumeConditionalExpression` of `ParseExpressionContinued`.
fn consume_conditional_expression(p: &mut LanguageParser<'_, '_>, left_operand: Green) -> Green {
    // Complex ambiguity with `?` and collection-expressions.  Specifically: b?[c]:d
    //
    // On its own, we want that to be a conditional expression with a collection expression in it.  However, for
    // back compat, we need to make sure that `a ? b?[c] : d` sees the inner `b?[c]` as a
    // conditional-access-expression.  So, if after consuming the portion after the initial `?` if we do not
    // have the `:` we need, and we can see a `?[` in that portion of the parse, then we retry consuming the
    // when-true portion, but this time forcing the prior way of handling `?[`.
    let question_token = p.eat_token();

    let mut after_question_token = p.get_disposable_reset_point(false);
    let mut when_true = p.parse_possible_ref_expression();

    if p.current_kind() != SyntaxKind::ColonToken
        && !p.force_conditional_access_expression
        && contains_ternary_collection_to_reinterpret(p, when_true)
    {
        // Keep track of where we are right now in case the new parse doesn't make things better.
        let mut original_after_when_true = p.get_disposable_reset_point(false);

        // Go back to right after the `?`
        p.reset_disposable(&mut after_question_token);

        // try reparsing with `?[` as a conditional access, not a ternary+collection
        let resetter = ParserSyntaxContextResetter::new(p, None, None, None, Some(true));
        // reparse the when-true portion
        let new_when_true = p.parse_possible_ref_expression();
        resetter.dispose(p);

        if p.current_kind() == SyntaxKind::ColonToken {
            // if we now are at a colon, this was preferred parse.
            when_true = new_when_true;
        } else {
            // retrying the parse didn't help.  Use the original interpretation.
            p.reset_disposable(&mut original_after_when_true);
        }
        p.dispose_reset_point(original_after_when_true);
    }

    let result = if p.current_kind() == SyntaxKind::EndOfFileToken && p.lexer.interpolation_followed_by_colon() {
        // We have an interpolated string with an interpolation that contains a conditional expression.
        // Unfortunately, the precedence demands that the colon is considered to signal the start of the
        // format string. Without this code, the compiler would complain about a missing colon, and point
        // to the colon that is present, which would be confusing. We aim to give a better error message.
        let a = p.arena();
        let colon = a.missing_token(SyntaxKind::ColonToken);
        let identifier = a.missing_token(SyntaxKind::IdentifierToken);
        let when_false = a.identifier_name(identifier);
        let conditional_expression = a.conditional_expression(left_operand, question_token, when_true, colon, when_false);
        p.add_error(conditional_expression, ErrorCode::ERR_ConditionalInInterpolation)
    } else {
        let colon = p.eat_token_kind(SyntaxKind::ColonToken);
        let when_false = p.parse_possible_ref_expression();
        p.arena().conditional_expression(left_operand, question_token, when_true, colon, when_false)
    };
    p.dispose_reset_point(after_question_token);
    result
}

/// `containsTernaryCollectionToReinterpret` of `ParseExpressionContinued`.
fn contains_ternary_collection_to_reinterpret(p: &mut LanguageParser<'_, '_>, expression: Green) -> bool {
    let a = p.arena_ref();
    let mut stack = vec![expression];
    let mut children = Vec::new();

    while let Some(current) = stack.pop() {
        if a.kind(current) == SyntaxKind::ConditionalExpression {
            let when_true = a.slot(current, ConditionalExpressionSyntax::WHEN_TRUE);
            if a.kind(a.first_token(when_true)) == SyntaxKind::OpenBracketToken {
                return true;
            }
        }

        // Note: we could consider not recursing into anonymous-methods/lambdas (since we reset the
        // ForceConditionalAccessExpression flag when we go into that).  However, that adds a bit of
        // fragile coupling between these different code blocks that i'd prefer to avoid.  In practice
        // the extra cost here will almost never occur, so the simplicity is worth it.
        if !current.is_token() {
            a.child_nodes_and_tokens(current, &mut children);
            stack.extend_from_slice(&children);
        }
    }

    false
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TUPLE GetExpressionOperatorTokenKindAndExpressionKind()` (LanguageParser.cs:11797): the operator token
    /// kind and the expression kind it makes.
    pub(crate) fn get_expression_operator_token_kind_and_expression_kind(&mut self) -> (SyntaxKind, SyntaxKind) {
        // If the set of expression continuations is updated here, please review ParseStatementAttributeDeclarations
        // to see if it may need a similar look-ahead check to determine if something is a collection expression versus
        // an attribute.

        let token1 = self.current_token();
        let mut token1_kind = self.arena_ref().contextual_kind(token1);

        // Merge two consecutive dots into a DotDotToken
        if self.is_at_dot_dot_token() {
            return (SyntaxKind::DotDotToken, SyntaxKind::RangeExpression);
        }

        // check for >>, >>=, >>> or >>>=
        //
        // In all those cases, update token1Kind to be the merged token kind.  It will then be handled by the code below.
        if token1_kind == SyntaxKind::GreaterThanToken {
            let token2 = self.peek_token(1);
            let token2_kind = self.arena_ref().kind(token2);
            if matches!(token2_kind, SyntaxKind::GreaterThanToken | SyntaxKind::GreaterThanEqualsToken)
                && self.no_trivia_between(token1, token2)
            // check to see if they really are adjacent
            {
                if token2_kind == SyntaxKind::GreaterThanToken {
                    let token3 = self.peek_token(2);
                    let token3_kind = self.arena_ref().kind(token3);
                    if matches!(token3_kind, SyntaxKind::GreaterThanToken | SyntaxKind::GreaterThanEqualsToken)
                        && self.no_trivia_between(token2, token3)
                    // check to see if they really are adjacent
                    {
                        // >>>  or  >>>=
                        token1_kind = if token3_kind == SyntaxKind::GreaterThanToken {
                            SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                        } else {
                            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                        };
                    } else {
                        // >>
                        token1_kind = SyntaxKind::GreaterThanGreaterThanToken;
                    }
                } else {
                    // >>=
                    token1_kind = SyntaxKind::GreaterThanGreaterThanEqualsToken;
                }
            }
        }

        if Self::is_expected_binary_operator(token1_kind) {
            return (token1_kind, facts::get_binary_expression(token1_kind));
        }

        if Self::is_expected_assignment_operator(token1_kind) {
            return (token1_kind, facts::get_assignment_expression(token1_kind));
        }

        if token1_kind == SyntaxKind::SwitchKeyword && self.peek_kind(1) == SyntaxKind::OpenBraceToken {
            return (token1_kind, SyntaxKind::SwitchExpression);
        }

        if token1_kind == SyntaxKind::WithKeyword && self.peek_kind(1) == SyntaxKind::OpenBraceToken {
            return (token1_kind, SyntaxKind::WithExpression);
        }

        // Something that doesn't expand the current expression we're looking at.  Bail out and see if we
        // can end with a conditional expression.
        (SyntaxKind::None, SyntaxKind::None)
    }

    /// `private SyntaxToken EatExpressionOperatorToken(SyntaxKind operatorTokenKind)` (LanguageParser.cs:11857).
    pub(crate) fn eat_expression_operator_token(&mut self, operator_token_kind: SyntaxKind) -> Green {
        // Combine tokens into a single token if needed

        if operator_token_kind == SyntaxKind::DotDotToken {
            return self.eat_dot_dot_token();
        }

        if matches!(
            operator_token_kind,
            SyntaxKind::GreaterThanGreaterThanToken | SyntaxKind::GreaterThanGreaterThanEqualsToken
        ) {
            // >> and >>=
            // Two tokens need to be consumed here.

            let token1 = self.eat_token();
            let token2 = self.eat_token();

            let a = self.arena();
            let leading = a.leading_trivia(token1);
            let trailing = a.trailing_trivia(token2);
            a.token_with_trivia(leading, operator_token_kind, trailing)
        } else if matches!(
            operator_token_kind,
            SyntaxKind::GreaterThanGreaterThanGreaterThanToken | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
        ) {
            // >>> and >>>=
            // Three tokens need to be consumed here.

            let token1 = self.eat_token();
            let _ = self.eat_token();
            let token3 = self.eat_token();

            let a = self.arena();
            let leading = a.leading_trivia(token1);
            let trailing = a.trailing_trivia(token3);
            a.token_with_trivia(leading, operator_token_kind, trailing)
        } else {
            // Normal operator.  Eat as a single token, converting contextual words cases (like 'with') to a keyword.
            self.eat_contextual_token(operator_token_kind)
        }
    }

    /// `private AssignmentExpressionSyntax ParseAssignmentExpression(SyntaxKind operatorExpressionKind, ExpressionSyntax leftOperand, SyntaxToken operatorToken)` (LanguageParser.cs:11898).
    pub(crate) fn parse_assignment_expression(
        &mut self,
        operator_expression_kind: SyntaxKind,
        left_operand: Green,
        operator_token: Green,
    ) -> Green {
        let rhs = if operator_expression_kind == SyntaxKind::SimpleAssignmentExpression
            && self.current_kind() == SyntaxKind::RefKeyword
            // check for lambda expression with explicit ref return type: `ref int () => { ... }`
            && !self.is_possible_lambda_expression(Precedence::ASSIGNMENT)
        {
            let ref_keyword = self.eat_token();
            let expression = self.parse_expression_core();
            self.arena().ref_expression(ref_keyword, expression)
        } else {
            self.parse_sub_expression(Precedence::ASSIGNMENT)
        };

        self.arena().assignment_expression(operator_expression_kind, left_operand, operator_token, rhs)
    }

    /// `public bool IsAtDotDotToken()` (LanguageParser.cs:11923): check if we're currently at a .. sequence that can
    /// then be parsed out as a `DotDotToken`.
    pub(crate) fn is_at_dot_dot_token(&mut self) -> bool {
        if self.current_kind() != SyntaxKind::DotToken {
            return false;
        }

        let next_token = self.peek_token(1);
        let current = self.current_token();
        self.arena_ref().kind(next_token) == SyntaxKind::DotToken && self.no_trivia_between(current, next_token)
    }

    /// `public static bool IsAtDotDotToken(SyntaxToken token1, SyntaxToken token2)` (LanguageParser.cs:11932).
    pub(crate) fn is_at_dot_dot_token_with_token2(&mut self, token1: Green, token2: Green) -> bool {
        self.arena_ref().kind(token1) == SyntaxKind::DotToken
            && self.arena_ref().kind(token2) == SyntaxKind::DotToken
            && self.no_trivia_between(token1, token2)
    }

    /// `public SyntaxToken EatDotDotToken()` (LanguageParser.cs:11940): consume the next two tokens as a
    /// `DotDotToken`.  Note: if three dot tokens are in a row, an error will be placed on the `..` token to say that
    /// is illegal, and single DotDot token will be returned.
    pub(crate) fn eat_dot_dot_token(&mut self) -> Green {
        let token1 = self.eat_token();
        let token2 = self.eat_token();

        let mut dot_dot_token = {
            let a = self.arena();
            let leading = a.leading_trivia(token1);
            let trailing = a.trailing_trivia(token2);
            a.token_with_trivia(leading, SyntaxKind::DotDotToken, trailing)
        };
        let token3 = self.current_token();
        if self.arena_ref().kind(token3) == SyntaxKind::DotToken && self.no_trivia_between(token2, token3) {
            // At least three dots directly in a row.  Definitely mark that this is always illegal.  We do not allow
            // `...` at all in case we want to use that syntax in the future.
            let offset = self.arena_ref().leading_trivia_width(dot_dot_token) as i32;
            dot_dot_token = self.add_error_at(dot_dot_token, offset, 0, ErrorCode::ERR_TripleDotNotAllowed, &[]);

            // If we have exactly 3 dots in a row, then make the third dot into skipped trivia on the `..` as this
            // is likely just a mistyped range/slice and we'll recover better if we don't try to process the 3rd dot
            // as a member access or anything like that.
            //
            // If we have 4 dots in a row (`....`), then don't skip any of them.  We'll let the caller handle the
            // next two dots as a range/slice/whatever.
            let token4 = self.peek_token(1);
            if self.arena_ref().kind(token4) != SyntaxKind::DotToken || !self.no_trivia_between(token3, token4) {
                let eaten = self.eat_token();
                dot_dot_token = self.add_skipped_syntax(dot_dot_token, eaten, true);
            }
        }

        dot_dot_token
    }

    /// `private DeclarationExpressionSyntax ParseDeclarationExpression(ParseTypeMode mode, bool isScoped)` (LanguageParser.cs:11976).
    pub(crate) fn parse_declaration_expression(&mut self, mode: ParseTypeMode, is_scoped: bool) -> Green {
        let scoped_keyword =
            if is_scoped { self.eat_contextual_token(SyntaxKind::ScopedKeyword) } else { Green::NULL };

        let type_ = self.parse_type(mode);
        let type_ = if scoped_keyword.is_null() { type_ } else { self.arena().scoped_type(scoped_keyword, type_) };
        let designation = self.parse_designation(false);
        self.arena().declaration_expression(type_, designation)
    }

    /// `private ExpressionSyntax ParseThrowExpression()` (LanguageParser.cs:11988).
    pub(crate) fn parse_throw_expression(&mut self) -> Green {
        let throw_keyword = self.eat_token_kind(SyntaxKind::ThrowKeyword);
        let expression = self.parse_sub_expression(Precedence::COALESCING);
        self.arena().throw_expression(throw_keyword, expression)
    }

    /// `private ExpressionSyntax ParseIsExpression(ExpressionSyntax leftOperand, SyntaxToken opToken)` (LanguageParser.cs:11995).
    pub(crate) fn parse_is_expression(&mut self, left_operand: Green, op_token: Green) -> Green {
        let node = self.parse_type_or_pattern_for_is_operator();
        if is_pattern_kind(self.arena_ref().kind(node)) {
            self.arena().is_pattern_expression(left_operand, op_token, node)
        } else {
            self.arena().binary_expression(SyntaxKind::IsExpression, left_operand, op_token, node)
        }
    }

    /// `private ExpressionSyntax ParsePrimaryExpression(Precedence precedence)` (LanguageParser.cs:12006).
    pub(crate) fn parse_primary_expression(&mut self, precedence: Precedence) -> Green {
        // Primary expressions:
        // x.y, f(x), a[i], x?.y, x?[y], x++, x--, x!, new, typeof, checked, unchecked, default, nameof, delegate, sizeof, stackalloc, x->y
        //
        // Note that postfix operators (like ++) are still primary expressions, even though their prefix equivalents (`++x`) are unary.

        let expr = parse_primary_expression_without_postfix(self, precedence);
        parse_post_fix_expression(self, expr)
    }
}

/// `parsePrimaryExpressionWithoutPostfix` of `ParsePrimaryExpression`.
fn parse_primary_expression_without_postfix(p: &mut LanguageParser<'_, '_>, precedence: Precedence) -> Green {
    let tk = p.current_kind();
    match tk {
        SyntaxKind::TypeOfKeyword => p.parse_type_of_expression(),
        SyntaxKind::DefaultKeyword => p.parse_default_expression(),
        SyntaxKind::SizeOfKeyword => p.parse_size_of_expression(),
        SyntaxKind::MakeRefKeyword => p.parse_make_ref_expression(),
        SyntaxKind::RefTypeKeyword => p.parse_ref_type_expression(),
        SyntaxKind::CheckedKeyword | SyntaxKind::UncheckedKeyword => p.parse_checked_or_unchecked_expression(),
        SyntaxKind::UnsafeKeyword => p.parse_unsafe_expression(),
        SyntaxKind::RefValueKeyword => p.parse_ref_value_expression(),
        SyntaxKind::ColonColonToken => {
            // misplaced ::
            // Calling ParseAliasQualifiedName will cause us to create a missing identifier node that then
            // properly consumes the :: and the reset of the alias name afterwards.
            p.parse_alias_qualified_name(NameOptions::IN_EXPRESSION)
        }
        SyntaxKind::EqualsGreaterThanToken => p.parse_lambda_expression(),
        SyntaxKind::StaticKeyword => {
            if p.is_possible_anonymous_method_expression() {
                p.parse_anonymous_method_expression()
            } else if p.is_possible_lambda_expression(precedence) {
                p.parse_lambda_expression()
            } else {
                let missing = p.create_missing_identifier_name();
                let text = p.current_token();
                p.add_error_args(missing, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::TokenText(text)])
            }
        }
        SyntaxKind::IdentifierToken => {
            if p.is_true_identifier() {
                if p.is_possible_anonymous_method_expression() {
                    return p.parse_anonymous_method_expression();
                }
                if p.is_possible_lambda_expression(precedence) {
                    let lambda = p.try_parse_lambda_expression();
                    if !lambda.is_null() {
                        return lambda;
                    }
                }
                if p.is_possible_deconstruction_left(precedence) {
                    p.parse_declaration_expression(ParseTypeMode::Normal, false)
                } else if p.is_current_token_field_in_keyword_context()
                    && p.peek_kind(1) != SyntaxKind::ColonColonToken
                {
                    let field = p.eat_contextual_token(SyntaxKind::FieldKeyword);
                    p.arena().field_expression(field)
                } else {
                    p.parse_alias_qualified_name(NameOptions::IN_EXPRESSION)
                }
            } else {
                let missing = p.create_missing_identifier_name();
                let text = p.current_token();
                p.add_error_args(missing, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::TokenText(text)])
            }
        }
        SyntaxKind::OpenBracketToken => {
            if p.is_possible_lambda_expression(precedence) {
                p.parse_lambda_expression()
            } else {
                p.parse_collection_expression()
            }
        }
        SyntaxKind::ThisKeyword => {
            let token = p.eat_token();
            p.arena().this_expression(token)
        }
        SyntaxKind::BaseKeyword => p.parse_base_expression(),

        SyntaxKind::ArgListKeyword
        | SyntaxKind::FalseKeyword
        | SyntaxKind::TrueKeyword
        | SyntaxKind::NullKeyword
        | SyntaxKind::NumericLiteralToken
        | SyntaxKind::StringLiteralToken
        | SyntaxKind::Utf8StringLiteralToken
        | SyntaxKind::CharacterLiteralToken => {
            let token = p.eat_token();
            p.arena().literal_expression(facts::get_literal_expression(tk), token)
        }
        SyntaxKind::InterpolatedStringStartToken
        | SyntaxKind::InterpolatedVerbatimStringStartToken
        | SyntaxKind::InterpolatedSingleLineRawStringStartToken
        | SyntaxKind::InterpolatedMultiLineRawStringStartToken => {
            // `throw new NotImplementedException()`: this should not occur because these tokens are produced and
            // parsed immediately (the lexer makes an `InterpolatedStringToken` in the parser's modes).
            let missing = p.create_missing_identifier_name();
            let text = p.current_token();
            p.add_error_args(missing, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::TokenText(text)])
        }
        SyntaxKind::InterpolatedStringToken => p.parse_interpolated_string_token(),
        SyntaxKind::SingleLineRawStringLiteralToken
        | SyntaxKind::Utf8SingleLineRawStringLiteralToken
        | SyntaxKind::MultiLineRawStringLiteralToken
        | SyntaxKind::Utf8MultiLineRawStringLiteralToken => p.parse_raw_string_token(),
        SyntaxKind::OpenParenToken => {
            if p.is_possible_lambda_expression(precedence) {
                let lambda = p.try_parse_lambda_expression();
                if !lambda.is_null() {
                    return lambda;
                }
            }
            p.parse_cast_or_paren_expression_or_tuple()
        }
        SyntaxKind::NewKeyword => p.parse_new_expression(),
        SyntaxKind::StackAllocKeyword => p.parse_stack_alloc_expression(),
        SyntaxKind::DelegateKeyword => {
            // check for lambda expression with explicit function pointer return type
            if p.is_possible_lambda_expression(precedence) {
                p.parse_lambda_expression()
            } else {
                p.parse_anonymous_method_expression()
            }
        }
        SyntaxKind::RefKeyword => {
            // check for lambda expression with explicit ref return type: `ref int () => { ... }`
            if p.is_possible_lambda_expression(precedence) {
                return p.parse_lambda_expression();
            }
            // ref is not expected to appear in this position.
            let ref_keyword = p.eat_token();
            let expression = p.parse_expression_core();
            let ref_expression = p.arena().ref_expression(ref_keyword, expression);
            p.add_error_args(ref_expression, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::Str(facts::get_text(tk))])
        }
        _ => {
            if LanguageParser::is_predefined_type(tk) {
                if p.is_possible_lambda_expression(precedence) {
                    return p.parse_lambda_expression();
                }

                // check for intrinsic type followed by '.'
                let keyword = p.eat_token();
                let mut expr = p.arena().predefined_type(keyword);

                if p.current_kind() != SyntaxKind::DotToken || tk == SyntaxKind::VoidKeyword {
                    expr = p.add_error_args(expr, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::Str(facts::get_text(tk))]);
                }

                expr
            } else {
                let mut expr = p.create_missing_identifier_name();

                if tk == SyntaxKind::EndOfFileToken {
                    expr = p.add_error(expr, ErrorCode::ERR_ExpressionExpected);
                } else if facts::is_binary_expression(tk) || facts::is_assignment_expression_operator_token(tk) {
                    // We got into the expression parsing path because we saw an error operator (see the
                    // default case in IsPossibleExpression), knowing we'd create a missing expr which would
                    // then allow the binary/assignment expr parsing to proceed.  In this case, we want to
                    // report the invalid expr, but place it next to the operator, not whatever might have
                    // come arbitrarily far before us.
                    let current = p.current_token();
                    let offset = p.arena_ref().leading_trivia_width(current) as i32;
                    let width = p.arena_ref().width(current) as i32;
                    let error =
                        p.make_error(offset, width, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::Str(facts::get_text(tk))]);
                    return p.with_additional_diagnostics(expr, &[error]);
                } else {
                    expr = p.add_error_args(expr, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::Str(facts::get_text(tk))]);
                }

                expr
            }
        }
    }
}

/// `parsePostFixExpression` of `ParsePrimaryExpression`.
fn parse_post_fix_expression(p: &mut LanguageParser<'_, '_>, expr: Green) -> Green {
    let mut expr = expr;
    loop {
        // If the set of postfix expressions is updated here, please review ParseStatementAttributeDeclarations
        // to see if it may need a similar look-ahead check to determine if something is a collection expression
        // versus an attribute.

        match p.current_kind() {
            SyntaxKind::OpenParenToken => {
                let argument_list = p.parse_parenthesized_argument_list();
                expr = p.arena().invocation_expression(expr, argument_list);
            }
            SyntaxKind::OpenBracketToken => {
                let argument_list = p.parse_bracketed_argument_list();
                expr = p.arena().element_access_expression(expr, argument_list);
            }
            SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken => {
                let kind = facts::get_postfix_unary_expression(p.current_kind());
                let operator_token = p.eat_token();
                expr = p.arena().postfix_unary_expression(kind, expr, operator_token);
            }
            SyntaxKind::ColonColonToken => {
                if p.peek_kind(1) == SyntaxKind::IdentifierToken {
                    // replace :: with missing dot and annotate with skipped text "::" and error
                    let colon_colon = p.eat_token();
                    let colon_colon = p.add_error(colon_colon, ErrorCode::ERR_UnexpectedAliasedName);
                    let dot = p.convert_to_missing_with_trailing_trivia(colon_colon, SyntaxKind::DotToken);
                    let name = p.parse_simple_name(NameOptions::IN_EXPRESSION);
                    expr = p.arena().member_access_expression(SyntaxKind::SimpleMemberAccessExpression, expr, dot, name);
                } else {
                    // just some random trailing :: ?
                    let skipped = p.eat_token_even_with_incorrect_kind(SyntaxKind::DotToken);
                    expr = p.add_trailing_skipped_syntax(expr, skipped);
                }
            }
            SyntaxKind::MinusGreaterThanToken => {
                let operator_token = p.eat_token();
                let name = p.parse_simple_name(NameOptions::IN_EXPRESSION);
                expr =
                    p.arena().member_access_expression(SyntaxKind::PointerMemberAccessExpression, expr, operator_token, name);
            }
            SyntaxKind::DotToken if !p.is_at_dot_dot_token() => {
                // if we have the error situation:
                //
                //      expr.
                //      X Y
                //
                // Then we don't want to parse this out as "Expr.X"
                //
                // It's far more likely the member access expression is simply incomplete and
                // there is a new declaration on the next line.
                let current = p.current_token();
                let trailing = p.arena_ref().trailing_trivia(current);
                let trailing_has_end_of_line = {
                    let a = p.arena_ref();
                    a.trivia_items(trailing).iter().any(|&t| a.trivia_kind(t) == SyntaxKind::EndOfLineTrivia)
                };
                if trailing_has_end_of_line
                    && p.peek_kind(1) == SyntaxKind::IdentifierToken
                    && {
                        let t = p.peek_token(2);
                        p.arena_ref().contextual_kind(t) == SyntaxKind::IdentifierToken
                    }
                {
                    let dot = p.eat_token();
                    let missing = p.create_missing_identifier_name();
                    let name = p.add_error(missing, ErrorCode::ERR_IdentifierExpected);
                    return p.arena().member_access_expression(SyntaxKind::SimpleMemberAccessExpression, expr, dot, name);
                }

                let dot = p.eat_token();
                let name = p.parse_simple_name(NameOptions::IN_EXPRESSION);
                expr = p.arena().member_access_expression(SyntaxKind::SimpleMemberAccessExpression, expr, dot, name);
            }
            SyntaxKind::QuestionToken => {
                let mut conditional_access = Green::NULL;
                if p.try_parse_conditional_access_expression(expr, &mut conditional_access) {
                    expr = conditional_access;
                    continue;
                }

                return expr;
            }
            SyntaxKind::ExclamationToken => {
                let operator_token = p.eat_token();
                expr = p.arena().postfix_unary_expression(SyntaxKind::SuppressNullableWarningExpression, expr, operator_token);
            }
            _ => return expr,
        }
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ExpressionSyntax ParseBaseExpression()` (LanguageParser.cs:12281).
    pub(crate) fn parse_base_expression(&mut self) -> Green {
        let token = self.eat_token();
        self.arena().base_expression(token)
    }

    /// `private bool IsPossibleDeconstructionLeft(Precedence precedence)` (LanguageParser.cs:12295): whether the
    /// precedence is at most Assignment, the current token is the identifier var or a predefined type followed by
    /// `(`, that `(` begins a valid parenthesized designation, and the token following that designation is `=`.
    pub(crate) fn is_possible_deconstruction_left(&mut self, precedence: Precedence) -> bool {
        if precedence > Precedence::ASSIGNMENT
            || !(self.current_contextual_kind() == SyntaxKind::VarKeyword
                || Self::is_predefined_type(self.current_kind()))
        {
            return false;
        }

        let point = self.get_disposable_reset_point(true);

        self.eat_token(); // `var`
        let result = self.current_kind() == SyntaxKind::OpenParenToken
            && self.scan_designator()
            && self.current_kind() == SyntaxKind::EqualsToken;
        self.dispose_reset_point(point);
        result
    }

    /// `private bool ScanDesignator()` (LanguageParser.cs:12310). Recursive on nested parentheses, which Roslyn
    /// leaves unguarded: the recursion depth counts each level as `ParseSubExpression` does.
    pub(crate) fn scan_designator(&mut self) -> bool {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.scan_designator_core();
        self.recursion_depth -= 1;
        result
    }

    fn scan_designator_core(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::IdentifierToken => {
                if !self.is_true_identifier() {
                    return false;
                }

                self.eat_token(); // eat the identifier
                true
            }
            SyntaxKind::OpenParenToken => loop {
                self.eat_token(); // eat the open paren or comma
                if !self.scan_designator() {
                    return false;
                }

                match self.current_kind() {
                    SyntaxKind::CommaToken => continue,
                    SyntaxKind::CloseParenToken => {
                        self.eat_token(); // eat the close paren
                        return true;
                    }
                    _ => return false,
                }
            },
            _ => false,
        }
    }

    /// `private bool IsPossibleAnonymousMethodExpression()` (LanguageParser.cs:12347).
    pub(crate) fn is_possible_anonymous_method_expression(&mut self) -> bool {
        // Skip past any static/async keywords.
        let mut token_index = 0;
        loop {
            let token = self.peek_token(token_index);
            let a = self.arena_ref();
            if a.kind(token) == SyntaxKind::StaticKeyword || a.contextual_kind(token) == SyntaxKind::AsyncKeyword {
                token_index += 1;
            } else {
                break;
            }
        }

        self.peek_kind(token_index) == SyntaxKind::DelegateKeyword
            && self.peek_kind(token_index + 1) != SyntaxKind::AsteriskToken
    }

    /// `private bool TryParseConditionalAccessExpression( ExpressionSyntax primaryExpression, [NotNullWhen(true)] out ConditionalAccessExpressionSyntax? conditionalAccessExpression)` (LanguageParser.cs:12368):
    /// called when we could be at a `?` that could start a `ConditionalAccessExpressionSyntax` or a
    /// `ConditionalExpressionSyntax`.  Returns true if this succeeds at parsing the former.
    pub(crate) fn try_parse_conditional_access_expression(
        &mut self,
        primary_expression: Green,
        conditional_access_expression: &mut Green,
    ) -> bool {
        // From
        // https://github.com/dotnet/csharpstandard/blob/standard-v7/standard/expressions.md#1288-null-conditional-member-access
        // https://github.com/dotnet/csharpstandard/blob/standard-v7/standard/expressions.md#12812-null-conditional-element-access

        // null_conditional_member_access
        //      : primary_expression '?' '.' identifier type_argument_list? dependent_access*
        //      ;
        //
        // null_conditional_element_access
        //      : primary_no_array_creation_expression '?' '[' argument_list ']' dependent_access*
        //      ;
        //
        // dependent_access
        //      : '.' identifier type_argument_list?    // member access
        //      | '[' argument_list ']'                 // element access
        //      | '(' argument_list ? ')'               // invocation
        //      ;

        // We get in here after parsing out the initial primary expression and seeing a `?` follow.

        let (question_token, binding_expression) = try_eat_question_and_binding_expression(self);
        if question_token.is_null() || binding_expression.is_null() {
            *conditional_access_expression = Green::NULL;
            return false;
        }

        // Not in Roslyn: `parseWhenNotNull` of `a?.b?.c…` ends in this method again for each `?`, a recursion Roslyn
        // does not guard (100 000 of them overflowed a 1 MB stack). The loop keeps each pending `primary ?` on
        // `pending_conditional_accesses` and builds the nodes from the innermost out, as the recursion returns.
        let base = self.pending_conditional_accesses.len();
        self.pending_conditional_accesses.push((primary_expression, question_token));
        let mut binding_expression = binding_expression;
        let mut when_not_null = loop {
            match parse_when_not_null(self, binding_expression) {
                WhenNotNull::End(expression) => break expression,
                WhenNotNull::ConditionalAccess { primary_expression, question_token, binding } => {
                    self.pending_conditional_accesses.push((primary_expression, question_token));
                    binding_expression = binding;
                }
            }
        };
        while self.pending_conditional_accesses.len() > base {
            let Some((primary, question)) = self.pending_conditional_accesses.pop() else { break };
            when_not_null = self.arena().conditional_access_expression(primary, question, when_not_null);
        }
        *conditional_access_expression = when_not_null;
        true
    }
}

/// `tryEatQuestionAndBindingExpression` of `TryParseConditionalAccessExpression`.
fn try_eat_question_and_binding_expression(p: &mut LanguageParser<'_, '_>) -> (Green, Green) {
    if p.current_kind() == SyntaxKind::QuestionToken {
        let next_token = p.peek_token(1);
        let next_token_kind = p.arena_ref().kind(next_token);

        // ?.   is always the start of of a consequence expression.
        //
        // ?..  is a ternary with a range expression as it's 'whenTrue' clause.
        if next_token_kind == SyntaxKind::DotToken {
            let token2 = p.peek_token(2);
            if !p.is_at_dot_dot_token_with_token2(next_token, token2) {
                let question_token = p.eat_token();
                let operator_token = p.eat_token();
                let name = p.parse_simple_name(NameOptions::IN_EXPRESSION);
                return (question_token, p.arena().member_binding_expression(operator_token, name));
            }
        }

        if is_start_of_element_binding_expression(p, next_token_kind) {
            let question_token = p.eat_token();
            let argument_list = p.parse_bracketed_argument_list();
            return (question_token, p.arena().element_binding_expression(argument_list));
        }
    }

    // Anything else is either not a `?` at all, or is just a `?` that starts a conditional expression (not
    // a conditional access expression).
    (Green::NULL, Green::NULL)
}

/// `isStartOfElementBindingExpression` of `TryParseConditionalAccessExpression`.
fn is_start_of_element_binding_expression(p: &mut LanguageParser<'_, '_>, next_token_kind: SyntaxKind) -> bool {
    if next_token_kind != SyntaxKind::OpenBracketToken {
        return false;
    }

    // could simply be `x?[0]`, or could be `x ? [0] : [1]`.

    // Caller only wants us to parse ?[ how it was originally parsed before collection expressions.
    if p.force_conditional_access_expression {
        return true;
    }

    let point = p.get_disposable_reset_point(true);

    // Move past the '?'. Parse what comes next the same way that conditional expressions are parsed.
    p.eat_token();
    p.parse_possible_ref_expression();

    // If we see a colon, then do not parse this as a conditional-access-expression, pop up to the caller
    // and have it reparse this as a conditional-expression instead.
    let result = p.current_kind() != SyntaxKind::ColonToken;
    p.dispose_reset_point(point);
    result
}

/// `parseWhenNotNull` of `TryParseConditionalAccessExpression`.
/// What `parseWhenNotNull` ends with: its expression, or `TryParseConditionalAccessExpression` of it, whose `?` and
/// binding it has eaten (`try_parse_conditional_access_expression` continues with them).
enum WhenNotNull {
    End(Green),
    ConditionalAccess { primary_expression: Green, question_token: Green, binding: Green },
}

fn parse_when_not_null(p: &mut LanguageParser<'_, '_>, expr: Green) -> WhenNotNull {
    let mut expr = expr;
    loop {
        // We should consume suppression '!'s which are in the middle of the 'whenNotNull', but not at the end.
        // For example, 'a?.b!.c' should be a cond-access whose RHS is '.b!.c',
        // while 'a?.b!' should be a suppression-expr containing a cond-access 'a?.b'.
        let mut before_suppressions_reset_point = p.get_disposable_reset_point(false);
        let expression_before_suppressions = expr;

        while p.current_kind() == SyntaxKind::ExclamationToken {
            let operator_token = p.eat_token();
            expr = p.arena().postfix_unary_expression(SyntaxKind::SuppressNullableWarningExpression, expr, operator_token);
        }

        // Expand to consume the `dependent_access*` continuations.
        let expanded_expression = try_parse_dependent_access(p, expr);
        if !expanded_expression.is_null() {
            expr = expanded_expression;
            p.dispose_reset_point(before_suppressions_reset_point);
            continue;
        }

        // A trailing cond-access or assignment is effectively the "end" of the current cond-access node.
        // Due to right-associativity, everything that follows will be included in the child node.
        // e.g. 'a?.b?.c' parses as '(a) ? (.b?.c)'
        // e.g. 'a?.b = c?.d = e?.f' parses as 'a?.b = (c?.d = e?.f)'

        // a?.b?.c
        // a?.b!?.c
        let (question_token, binding) = try_eat_question_and_binding_expression(p);
        if !question_token.is_null() && !binding.is_null() {
            p.dispose_reset_point(before_suppressions_reset_point);
            return WhenNotNull::ConditionalAccess { primary_expression: expr, question_token, binding };
        }

        // a?.b = c
        // a?.b! = c
        let (operator_token_kind, operator_expression_kind) = p.get_expression_operator_token_kind_and_expression_kind();
        if LanguageParser::is_expected_assignment_operator(operator_token_kind) {
            let operator_token = p.eat_expression_operator_token(operator_token_kind);
            let result = p.parse_assignment_expression(operator_expression_kind, expr, operator_token);
            p.dispose_reset_point(before_suppressions_reset_point);
            return WhenNotNull::End(result);
        }

        // End of the cond-access.
        // Any '!' suppressions which followed this are a parent of the cond-access, not a child of it.
        p.reset_disposable(&mut before_suppressions_reset_point);
        p.dispose_reset_point(before_suppressions_reset_point);
        return WhenNotNull::End(expression_before_suppressions);
    }
}

/// `tryParseDependentAccess` of `TryParseConditionalAccessExpression`: null when no dependent access follows.
fn try_parse_dependent_access(p: &mut LanguageParser<'_, '_>, expr: Green) -> Green {
    match p.current_kind() {
        SyntaxKind::OpenParenToken => {
            let argument_list = p.parse_parenthesized_argument_list();
            p.arena().invocation_expression(expr, argument_list)
        }
        SyntaxKind::OpenBracketToken => {
            let argument_list = p.parse_bracketed_argument_list();
            p.arena().element_access_expression(expr, argument_list)
        }
        SyntaxKind::DotToken => {
            let operator_token = p.eat_token();
            let name = p.parse_simple_name(NameOptions::IN_EXPRESSION);
            p.arena().member_access_expression(SyntaxKind::SimpleMemberAccessExpression, expr, operator_token, name)
        }
        _ => Green::NULL,
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `internal ArgumentListSyntax ParseParenthesizedArgumentList()` (LanguageParser.cs:12507).
    pub(crate) fn parse_parenthesized_argument_list(&mut self) -> Green {
        let (mut open_token, mut arguments, mut close_token) = (Green::NULL, Green::NULL, Green::NULL);
        self.parse_argument_list(
            &mut open_token,
            &mut arguments,
            &mut close_token,
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
        );
        self.arena().argument_list(open_token, arguments, close_token)
    }

    /// `internal BracketedArgumentListSyntax ParseBracketedArgumentList()` (LanguageParser.cs:12523).
    pub(crate) fn parse_bracketed_argument_list(&mut self) -> Green {
        let (mut open_token, mut arguments, mut close_token) = (Green::NULL, Green::NULL, Green::NULL);
        self.parse_argument_list(
            &mut open_token,
            &mut arguments,
            &mut close_token,
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
        );
        self.arena().bracketed_argument_list(open_token, arguments, close_token)
    }

    /// `private void ParseArgumentList( out SyntaxToken openToken, out SeparatedSyntaxList<ArgumentSyntax> arguments, out SyntaxToken closeToken, SyntaxKind openKind, SyntaxKind closeKind)` (LanguageParser.cs:12539).
    pub(crate) fn parse_argument_list(
        &mut self,
        open_token: &mut Green,
        arguments: &mut Green,
        close_token: &mut Green,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) {
        let is_indexer = open_kind == SyntaxKind::OpenBracketToken;

        // convert `[` into `(` or vice versa for error recovery
        *open_token = if matches!(self.current_kind(), SyntaxKind::OpenParenToken | SyntaxKind::OpenBracketToken) {
            self.eat_token_as_kind(open_kind)
        } else {
            self.eat_token_kind(open_kind)
        };

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_ARGUMENT_LIST;

        if self.current_kind() != close_kind && self.current_kind() != SyntaxKind::SemicolonToken {
            if is_indexer {
                // An indexer always expects at least one value.
                *arguments = self.parse_comma_separated_syntax_list(
                    open_token,
                    SyntaxKind::CloseBracketToken,
                    |p| p.is_possible_argument_expression(),
                    |p| p.parse_argument_expression(true),
                    skip_bad_argument_list_tokens,
                    false,
                    false,
                    false,
                );
            } else {
                *arguments = self.parse_comma_separated_syntax_list(
                    open_token,
                    SyntaxKind::CloseParenToken,
                    |p| p.is_possible_argument_expression(),
                    |p| p.parse_argument_expression(false),
                    skip_bad_argument_list_tokens,
                    false,
                    false,
                    false,
                );
            }
        } else if is_indexer && self.current_kind() == close_kind {
            // An indexer always expects at least one value. And so we need to give an error
            // for the case where we see only "[]". ParseArgumentExpression gives it.
            let mut list = self.pool.allocate();
            let argument = self.parse_argument_expression(is_indexer);
            list.push(argument);
            *arguments = to_list_and_free(self, list);
        } else {
            *arguments = Green::NULL;
        }

        self.term_state = save_term;

        // convert `]` into `)` or vice versa for error recovery
        *close_token = if matches!(self.current_kind(), SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken) {
            self.eat_token_as_kind(close_kind)
        } else {
            self.eat_token_kind(close_kind)
        };
    }
}

/// `skipBadArgumentListTokens` of `ParseArgumentList`.
fn skip_bad_argument_list_tokens(
    p: &mut LanguageParser<'_, '_>,
    open: &mut Green,
    list: &mut SyntaxListBuilder,
    expected_kind: SyntaxKind,
    close_kind: SyntaxKind,
) -> PostSkipAction {
    if matches!(p.current_kind(), SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken | SyntaxKind::SemicolonToken)
    {
        return PostSkipAction::Abort;
    }

    p.skip_bad_separated_list_tokens_with_expected_kind(
        open,
        list,
        |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_argument_expression(),
        |p, close_kind| p.current_kind() == close_kind || p.current_kind() == SyntaxKind::SemicolonToken,
        expected_kind,
        close_kind,
    )
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsEndOfArgumentList()` (LanguageParser.cs:12622).
    pub(crate) fn is_end_of_argument_list(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken)
    }

    /// `private bool IsPossibleArgumentExpression()` (LanguageParser.cs:12627).
    pub(crate) fn is_possible_argument_expression(&mut self) -> bool {
        Self::is_valid_argument_ref_kind_keyword(self.current_kind()) || self.is_possible_expression()
    }

    /// `private static bool IsValidArgumentRefKindKeyword(SyntaxKind kind)` (LanguageParser.cs:12632).
    pub(crate) fn is_valid_argument_ref_kind_keyword(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::RefKeyword | SyntaxKind::OutKeyword | SyntaxKind::InKeyword)
    }

    /// `private ArgumentSyntax ParseArgumentExpression(bool isIndexer)` (LanguageParser.cs:12645).
    pub(crate) fn parse_argument_expression(&mut self, is_indexer: bool) -> Green {
        let name_colon =
            if self.current_kind() == SyntaxKind::IdentifierToken && self.peek_kind(1) == SyntaxKind::ColonToken {
                let name = self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected);
                let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                self.arena().name_colon(name, colon)
            } else {
                Green::NULL
            };

        let mut ref_kind_keyword = Green::NULL;
        if Self::is_valid_argument_ref_kind_keyword(self.current_kind())
            // check for lambda expression with explicit ref return type: `ref int () => { ... }`
            && !(self.current_kind() == SyntaxKind::RefKeyword && self.is_possible_lambda_expression(Precedence::EXPRESSION))
        {
            ref_kind_keyword = self.eat_token();
        }

        let expression = if is_indexer && matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::CloseBracketToken)
        {
            self.parse_identifier_name(ErrorCode::ERR_ValueExpected)
        } else if self.current_kind() == SyntaxKind::CommaToken {
            self.parse_identifier_name(ErrorCode::ERR_MissingArgument)
        } else {
            // According to Language Specification, section 7.6.7 Element access
            //      The argument-list of an element-access is not allowed to contain ref or out arguments.
            // However, we actually do support ref indexing of indexed properties in COM interop
            // scenarios, and when indexing an object of static type "dynamic". So we enforce
            // that the ref/out of the argument must match the parameter when binding the argument list.

            if !ref_kind_keyword.is_null() && self.arena_ref().kind(ref_kind_keyword) == SyntaxKind::OutKeyword {
                self.parse_expression_or_declaration(ParseTypeMode::Normal, false)
            } else {
                self.parse_sub_expression(Precedence::EXPRESSION)
            }
        };

        self.arena().argument(name_colon, ref_kind_keyword, expression)
    }

    /// `private TypeOfExpressionSyntax ParseTypeOfExpression()` (LanguageParser.cs:12688).
    pub(crate) fn parse_type_of_expression(&mut self) -> Green {
        let keyword = self.eat_token();
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let type_ = self.parse_type_or_void();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().type_of_expression(keyword, open_paren, type_, close_paren)
    }

    /// `private ExpressionSyntax ParseDefaultExpression()` (LanguageParser.cs:12697).
    pub(crate) fn parse_default_expression(&mut self) -> Green {
        let keyword = self.eat_token();
        if self.current_kind() == SyntaxKind::OpenParenToken {
            let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
            let type_ = self.parse_type(ParseTypeMode::Normal);
            let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
            self.arena().default_expression(keyword, open_paren, type_, close_paren)
        } else {
            self.arena().literal_expression(SyntaxKind::DefaultLiteralExpression, keyword)
        }
    }

    /// `private SizeOfExpressionSyntax ParseSizeOfExpression()` (LanguageParser.cs:12714).
    pub(crate) fn parse_size_of_expression(&mut self) -> Green {
        let keyword = self.eat_token();
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let type_ = self.parse_type(ParseTypeMode::Normal);
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().size_of_expression(keyword, open_paren, type_, close_paren)
    }

    /// `private MakeRefExpressionSyntax ParseMakeRefExpression()` (LanguageParser.cs:12723).
    pub(crate) fn parse_make_ref_expression(&mut self) -> Green {
        let keyword = self.eat_token();
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_for_parenthesized_construct();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().make_ref_expression(keyword, open_paren, expression, close_paren)
    }

    /// `private RefTypeExpressionSyntax ParseRefTypeExpression()` (LanguageParser.cs:12732).
    pub(crate) fn parse_ref_type_expression(&mut self) -> Green {
        let keyword = self.eat_token();
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_for_parenthesized_construct();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().ref_type_expression(keyword, open_paren, expression, close_paren)
    }

    /// `private CheckedExpressionSyntax ParseCheckedOrUncheckedExpression()` (LanguageParser.cs:12741).
    pub(crate) fn parse_checked_or_unchecked_expression(&mut self) -> Green {
        let checked_or_unchecked = self.eat_token();
        let kind = if self.arena_ref().kind(checked_or_unchecked) == SyntaxKind::CheckedKeyword {
            SyntaxKind::CheckedExpression
        } else {
            SyntaxKind::UncheckedExpression
        };

        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_for_parenthesized_construct();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().checked_expression(kind, checked_or_unchecked, open_paren, expression, close_paren)
    }

    /// `private UnsafeExpressionSyntax ParseUnsafeExpression()` (LanguageParser.cs:12755).
    pub(crate) fn parse_unsafe_expression(&mut self) -> Green {
        let keyword = self.eat_token_kind(SyntaxKind::UnsafeKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_for_parenthesized_construct();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().unsafe_expression(keyword, open_paren, expression, close_paren)
    }

    /// `private RefValueExpressionSyntax ParseRefValueExpression()` (LanguageParser.cs:12764).
    pub(crate) fn parse_ref_value_expression(&mut self) -> Green {
        let keyword = self.eat_token_kind(SyntaxKind::RefValueKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let expression = self.parse_sub_expression(Precedence::EXPRESSION);
        let comma = self.eat_token_kind(SyntaxKind::CommaToken);
        let type_ = self.parse_type(ParseTypeMode::Normal);
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().ref_value_expression(keyword, open_paren, expression, comma, type_, close_paren)
    }

    /// `private bool ScanParenthesizedLambda(Precedence precedence)` (LanguageParser.cs:12775).
    pub(crate) fn scan_parenthesized_lambda(&mut self, precedence: Precedence) -> bool {
        self.scan_implicitly_typed_lambda_or_simple_explicitly_typed_parenthesized_lambda(precedence)
            || self.scan_explicitly_typed_lambda(precedence)
    }

    /// `private bool ScanImplicitlyTypedLambdaOrSimpleExplicitlyTypedParenthesizedLambda(Precedence precedence)` (LanguageParser.cs:12785):
    /// scans implicitly typed lambdas (like `(a, b) =>`) as well as basic explicitly typed lambdas (like
    /// `(A a, B b) =>`).  More complex scanning of parenthesized lambdas happens in `ScanExplicitlyTypedLambda`.
    pub(crate) fn scan_implicitly_typed_lambda_or_simple_explicitly_typed_parenthesized_lambda(
        &mut self,
        precedence: Precedence,
    ) -> bool {
        if precedence > Precedence::LAMBDA {
            return false;
        }

        //  (   ) =>
        //  ( x ) =>           or       ( ref x ) =>
        //  ( x , ... ) =>     or       ( ref x , ...) =>
        let mut index = 1;

        loop {
            let token = self.peek_token(index);
            index += 1;

            // Keep skipping modifiers, commas, and identifiers to consume the rest of the lambda arguments. Note:
            // this *will* grab explicitly typed lambdas like `(A b) =>`.  However, that's ok.  The only caller of
            // this is ScanParenthesizedLambda, which just wants to know if it's on some form of lambda.
            if self.is_true_identifier_with_token(token)
                || self.arena_ref().kind(token) == SyntaxKind::CommaToken
                || self.is_parameter_modifier_including_scoped(token)
            {
                continue;
            }

            return self.arena_ref().kind(token) == SyntaxKind::CloseParenToken
                && self.peek_kind(index) == SyntaxKind::EqualsGreaterThanToken;
        }
    }

    /// `private bool ScanExplicitlyTypedLambda(Precedence precedence)` (LanguageParser.cs:12816).
    pub(crate) fn scan_explicitly_typed_lambda(&mut self, precedence: Precedence) -> bool {
        if precedence > Precedence::LAMBDA {
            return false;
        }

        let point = self.get_disposable_reset_point(true);
        let result = scan_explicitly_typed_lambda_worker(self);
        self.dispose_reset_point(point);
        result
    }
}

/// The body of `ScanExplicitlyTypedLambda` inside its `using` scope.
fn scan_explicitly_typed_lambda_worker(p: &mut LanguageParser<'_, '_>) -> bool {
    // Do we have the following, where the attributes, modifier, and type are
    // optional? If so then parse it as a lambda.
    //   (attributes modifier T x [, ...]) =>
    //
    // It's not sufficient to assume this is a lambda expression if we see a
    // modifier such as `(ref x,` because the caller of this method may have
    // scanned past a preceding identifier, and `F (ref x,` might be a call to
    // method F rather than a lambda expression with return type F.
    // Instead, we need to scan to `=>`.

    loop {
        // Advance past the open paren or comma.
        p.eat_token();

        p.parse_attribute_declarations(true);

        let current = p.current_token();
        if p.is_parameter_modifier_including_scoped(current) {
            let mut modifiers = p.pool.allocate();
            p.parse_parameter_modifiers(&mut modifiers, false, true);
            p.pool.free(modifiers);
        }

        if p.should_parse_lambda_parameter_type() && p.scan_type(false) == ScanTypeFlags::NOT_TYPE {
            return false;
        }

        // eat the parameter name.
        let _identifier = if p.is_true_identifier() { p.eat_token() } else { p.create_missing_identifier_token() };

        let equals_token = p.try_eat_token(SyntaxKind::EqualsToken);

        // If we have an `=` then parse out a default value.  Note: this is not legal, but this allows us to
        // to be resilient to the user writing this so we don't go completely off the rails.
        if !equals_token.is_null() {
            // Note: we don't do this if we have `=[`.  Realistically, this is never going to be a lambda
            // expression as a `[` can only start an attribute declaration or collection expression, neither of
            // which can be a default arg.  Checking for this helps us from going off the rails in pathological
            // cases with lots of nested tokens that look like the could be anything.
            if p.current_kind() == SyntaxKind::OpenBracketToken {
                return false;
            }

            p.parse_expression_core();
        }

        match p.current_kind() {
            SyntaxKind::CommaToken => continue,
            SyntaxKind::CloseParenToken => return p.peek_kind(1) == SyntaxKind::EqualsGreaterThanToken,
            _ => return false,
        }
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ExpressionSyntax ParseCastOrParenExpressionOrTuple()` (LanguageParser.cs:12892).
    pub(crate) fn parse_cast_or_paren_expression_or_tuple(&mut self) -> Green {
        let mut reset_point = self.get_disposable_reset_point(false);
        let result = parse_cast_or_paren_expression_or_tuple_worker(self, &mut reset_point);
        self.dispose_reset_point(reset_point);
        result
    }
}

/// The body of `ParseCastOrParenExpressionOrTuple` inside its `using` scope.
fn parse_cast_or_paren_expression_or_tuple_worker(
    p: &mut LanguageParser<'_, '_>,
    reset_point: &mut DisposableResetPoint,
) -> Green {
    // We have a decision to make -- is this a cast, or is it a parenthesized
    // expression?  Because look-ahead is cheap with our token stream, we check
    // to see if this "looks like" a cast (without constructing any parse trees)
    // to help us make the decision.
    if p.scan_cast(false, false) && !p.is_current_token_query_keyword_in_query() {
        // Looks like a cast, so parse it as one.
        p.reset_disposable(reset_point);
        let open_paren = p.eat_token_kind(SyntaxKind::OpenParenToken);
        let type_ = p.parse_type(ParseTypeMode::Normal);
        let close_paren = p.eat_token_kind(SyntaxKind::CloseParenToken);
        let expression = p.parse_sub_expression(Precedence::CAST);
        return p.arena().cast_expression(open_paren, type_, close_paren, expression);
    }

    // Doesn't look like a cast, so parse this as a parenthesized expression or tuple.
    p.reset_disposable(reset_point);
    let open_paren = p.eat_token_kind(SyntaxKind::OpenParenToken);
    let expression = p.parse_expression_or_declaration(ParseTypeMode::FirstElementOfPossibleTupleLiteral, true);

    //  ( <expr>,    must be a tuple
    if p.current_kind() == SyntaxKind::CommaToken {
        let first_arg = p.arena().argument(Green::NULL, Green::NULL, expression);
        return p.parse_tuple_expression_tail(open_paren, first_arg);
    }

    // ( name:
    if p.arena_ref().kind(expression) == SyntaxKind::IdentifierName && p.current_kind() == SyntaxKind::ColonToken {
        let colon = p.eat_token();
        let name_colon = p.arena().name_colon(expression, colon);
        let element = p.parse_expression_or_declaration(ParseTypeMode::FirstElementOfPossibleTupleLiteral, true);
        let first_arg = p.arena().argument(name_colon, Green::NULL, element);
        return p.parse_tuple_expression_tail(open_paren, first_arg);
    }

    let expression = p.parse_errant_expression_when_no_close_paren_token(expression);
    let close_paren = p.eat_token_kind(SyntaxKind::CloseParenToken);
    p.arena().parenthesized_expression(open_paren, expression, close_paren)
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TupleExpressionSyntax ParseTupleExpressionTail(SyntaxToken openParen, ArgumentSyntax firstArg)` (LanguageParser.cs:12946).
    pub(crate) fn parse_tuple_expression_tail(&mut self, open_paren: Green, first_arg: Green) -> Green {
        let mut list = self.pool.allocate();
        list.push(first_arg);

        while self.current_kind() == SyntaxKind::CommaToken {
            let comma = self.eat_token_kind(SyntaxKind::CommaToken);
            list.push(comma);

            let expression = self.parse_expression_or_declaration(ParseTypeMode::AfterTupleComma, true);
            let argument = if self.arena_ref().kind(expression) != SyntaxKind::IdentifierName
                || self.current_kind() != SyntaxKind::ColonToken
            {
                self.arena().argument(Green::NULL, Green::NULL, expression)
            } else {
                let colon = self.eat_token();
                let name_colon = self.arena().name_colon(expression, colon);
                let element = self.parse_expression_or_declaration(ParseTypeMode::AfterTupleComma, true);
                self.arena().argument(name_colon, Green::NULL, element)
            };

            list.push(argument);
        }

        if list.len() < 2 {
            let comma = self.arena().missing_token(SyntaxKind::CommaToken);
            list.push(comma);
            let missing = self.create_missing_identifier_name();
            let missing = self.add_error(missing, ErrorCode::ERR_TupleTooFewElements);
            let argument = self.arena().argument(Green::NULL, Green::NULL, missing);
            list.push(argument);
        }

        let arguments = to_list_and_free(self, list);
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().tuple_expression(open_paren, arguments, close_paren)
    }

    /// `private bool ScanCast(bool forPattern, bool inSwitchArmPattern)` (LanguageParser.cs:12980).
    pub(crate) fn scan_cast(&mut self, for_pattern: bool, in_switch_arm_pattern: bool) -> bool {
        if self.current_kind() != SyntaxKind::OpenParenToken {
            return false;
        }

        self.eat_token();

        let type_ = self.scan_type(for_pattern);
        if type_ == ScanTypeFlags::NOT_TYPE {
            return false;
        }

        if self.current_kind() != SyntaxKind::CloseParenToken {
            return false;
        }

        self.eat_token();

        if for_pattern && self.current_kind() == SyntaxKind::IdentifierToken {
            // In a pattern we might have a cast of a constant, or the start of a legal pattern form.
            //
            // For example: `(A.B) and ...` should be treated not as a 'cast' of some variable 'and', but instead as
            // a conjunctive pattern.
            if is_binary_pattern(self) {
                return false;
            }

            // Similarly `(A.B) when` directly in a switch arm should be treated as the start of a `when` clause not
            // a cast of a `when` variable when in a switch expression arm.  This matches the exact checking logic
            // in IsValidPatternDesignation.
            if in_switch_arm_pattern && self.current_contextual_kind() == SyntaxKind::WhenKeyword {
                return false;
            }

            return true;
        }

        // If we have any of the following, we know it must be a cast:
        // 1) (Goo*)bar;
        // 2) (Goo?)bar;
        // 3) "(int)bar" or "(int[])bar"
        // 4) (G::Goo)bar
        if type_ == ScanTypeFlags::POINTER_OR_MULTIPLICATION
            || type_ == ScanTypeFlags::NULLABLE_TYPE
            || type_ == ScanTypeFlags::MUST_BE_TYPE
            || type_ == ScanTypeFlags::ALIAS_QUALIFIED_NAME
        {
            // The thing between parens is unambiguously a type.
            // In a pattern, we need more lookahead to confirm it is a cast and not
            // a parenthesized type pattern.  In this case the tokens that
            // have both unary and binary operator forms may appear in their unary form
            // following a cast.
            return !for_pattern
                || match self.current_kind() {
                    SyntaxKind::PlusToken | SyntaxKind::MinusToken | SyntaxKind::AmpersandToken | SyntaxKind::AsteriskToken => {
                        true
                    }

                    // `(X)..` must be a cast of a range expression, not a member access of some arbitrary expression.
                    SyntaxKind::DotToken if self.is_at_dot_dot_token() => true,

                    tk => Self::can_follow_cast(tk),
                };
        }

        if type_ == ScanTypeFlags::GENERIC_TYPE_OR_METHOD || type_ == ScanTypeFlags::TUPLE_TYPE {
            // If we have `(X<Y>)[...` then we know this must be a cast of a collection expression, not an index
            // into some expr. As most collections are generic, the common case is not ambiguous.
            //
            // Things are still ambiguous if you have `(X)[...` and for back compat we still parse that as
            // indexing into an expression.  The user can still write `(X)([...` in this case though to get cast
            // parsing. As non-generic casts are the rare case for collection expressions, this gives a good
            // balance of back compat and user ease for the normal case.
            return self.current_kind() == SyntaxKind::OpenBracketToken || Self::can_follow_cast(self.current_kind());
        }

        // GenericTypeOrExpression, NonGenericTypeOrExpression (`throw ExceptionUtilities.UnexpectedValue(type)` for
        // any other: ScanType returns no other)

        // if we have `(A)[]` then treat that always as a cast of an empty collection expression.  `[]` is not
        // legal on the RHS in any other circumstances for a parenthesized expr.
        if self.current_kind() == SyntaxKind::OpenBracketToken && self.peek_kind(1) == SyntaxKind::CloseBracketToken {
            return true;
        }

        // check for ambiguous type or expression followed by disambiguating token.  i.e.
        //
        // "(A)b" is a cast.  But "(A)+b" is not a cast.
        Self::can_follow_cast(self.current_kind())
    }
}

/// `isBinaryPattern` of `ScanCast`.
fn is_binary_pattern(p: &mut LanguageParser<'_, '_>) -> bool {
    if !is_binary_pattern_keyword(p) {
        return false;
    }

    let mut last_token_is_binary_operator = true;

    p.eat_token();
    while is_binary_pattern_keyword(p) {
        // If we see a subsequent binary pattern token, it can't be an operator.
        // Later, it will be parsed as an identifier.
        last_token_is_binary_operator = !last_token_is_binary_operator;
        p.eat_token();
    }

    // In case a combinator token is used as a constant, we explicitly check that a pattern is NOT followed.
    // Such as `(e is (int)or or >= 0)` versus `(e is (int) or or)`
    last_token_is_binary_operator == p.is_possible_subpattern_element()
}

/// `isBinaryPatternKeyword` of `ScanCast`.
fn is_binary_pattern_keyword(p: &mut LanguageParser<'_, '_>) -> bool {
    matches!(p.current_contextual_kind(), SyntaxKind::OrKeyword | SyntaxKind::AndKeyword)
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsPossibleLambdaExpression(Precedence precedence)` (LanguageParser.cs:13118): tokens that match
    /// `attribute-list* ('async' | 'static')* type? ('(' | identifier) ...` are considered a possible lambda
    /// expression. For better error recovery 'static =>' is also considered a possible lambda expression.
    pub(crate) fn is_possible_lambda_expression(&mut self, precedence: Precedence) -> bool {
        if precedence > Precedence::LAMBDA {
            return false;
        }

        // x =>
        //
        // Def a lambda.
        if self.peek_kind(1) == SyntaxKind::EqualsGreaterThanToken {
            return true;
        }

        let point = self.get_disposable_reset_point(true);
        let result = is_possible_lambda_expression_worker(self, precedence);
        self.dispose_reset_point(point);
        result
    }
}

/// The body of `IsPossibleLambdaExpression` inside its `using` scope.
fn is_possible_lambda_expression_worker(p: &mut LanguageParser<'_, '_>, precedence: Precedence) -> bool {
    // A lambda could be starting with attributes, attempt to skip past them and check after that point.
    if p.current_kind() == SyntaxKind::OpenBracketToken {
        // Subtle case to deal with.  Consider:
        //
        // [X, () => {}       vs:
        // [X] () => {}
        //
        // The former is a collection expression, the latter an attributed-lambda.  However, we will likely
        // successfully parse out `[X,` as an incomplete attribute, and thus think the former is the latter. So,
        // to ensure proper parsing of the collection expressions, bail out if the attribute is not complete.
        let attribute_declarations = p.parse_attribute_declarations(true);
        let a = p.arena_ref();
        let count = list_count(a, attribute_declarations);
        if count > 0 {
            let last = list_item(a, attribute_declarations, count - 1);
            if a.is_missing(a.slot(last, AttributeListSyntax::CLOSE_BRACKET_TOKEN)) {
                return false;
            }
        }
    }

    let seen_static = if p.current_kind() == SyntaxKind::StaticKeyword {
        p.eat_token();
        true
    } else if p.current_contextual_kind() == SyntaxKind::AsyncKeyword && p.peek_kind(1) == SyntaxKind::StaticKeyword {
        p.eat_token();
        p.eat_token();
        true
    } else {
        false
    };

    if seen_static {
        if p.current_kind() == SyntaxKind::EqualsGreaterThanToken {
            // 1. `static =>`
            // 2. `async static =>`

            // This is an error case, but we have enough code in front of us to be certain
            // the user was trying to write a static lambda.
            return true;
        }

        if p.current_kind() == SyntaxKind::OpenParenToken {
            // 1. `static (...
            // 2. `async static (...
            return true;
        }
    }

    if p.current_kind() == SyntaxKind::IdentifierToken && p.peek_kind(1) == SyntaxKind::EqualsGreaterThanToken {
        // 1. `a => ...`
        // 1. `static a => ...`
        // 2. `async static a => ...`
        return true;
    }

    // Have checked all the static forms.  And have checked for the basic `a => a` form.
    // At this point we have must be on 'async' or an explicit return type for this to still be a lambda.
    if p.current_contextual_kind() == SyntaxKind::AsyncKeyword && p.is_anonymous_function_async_modifier() {
        p.eat_token();
    }

    {
        let mut nested_reset_point = p.get_disposable_reset_point(false);
        let st = p.scan_type(false);
        if st == ScanTypeFlags::NOT_TYPE || p.current_kind() != SyntaxKind::OpenParenToken {
            p.reset_disposable(&mut nested_reset_point);
        }
        p.dispose_reset_point(nested_reset_point);
    }

    // However, just because we're on `async` doesn't mean we're a lambda.  We might have
    // something lambda-like like:
    //
    //      async a => ...  // or
    //      async (a) => ...
    //
    // Or we could have something that isn't a lambda like:
    //
    //      async ();

    // 'async <identifier> => ...' looks like an async simple lambda
    if p.current_kind() == SyntaxKind::IdentifierToken && p.peek_kind(1) == SyntaxKind::EqualsGreaterThanToken {
        // async a => ...
        return true;
    }

    // Non-simple async lambda must be of the form 'async (...'
    if p.current_kind() != SyntaxKind::OpenParenToken {
        return false;
    }

    // Check whether looks like implicitly or explicitly typed lambda
    p.scan_parenthesized_lambda(precedence)
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private static bool CanFollowCast(SyntaxKind kind)` (LanguageParser.cs:13245).
    pub(crate) fn can_follow_cast(kind: SyntaxKind) -> bool {
        !matches!(
            kind,
            SyntaxKind::AsKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::SemicolonToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CommaToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::PlusEqualsToken
                | SyntaxKind::MinusEqualsToken
                | SyntaxKind::AsteriskEqualsToken
                | SyntaxKind::SlashEqualsToken
                | SyntaxKind::PercentEqualsToken
                | SyntaxKind::AmpersandEqualsToken
                | SyntaxKind::CaretEqualsToken
                | SyntaxKind::BarEqualsToken
                | SyntaxKind::LessThanLessThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::ColonToken
                | SyntaxKind::BarBarToken
                | SyntaxKind::AmpersandAmpersandToken
                | SyntaxKind::BarToken
                | SyntaxKind::CaretToken
                | SyntaxKind::AmpersandToken
                | SyntaxKind::EqualsEqualsToken
                | SyntaxKind::ExclamationEqualsToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::LessThanEqualsToken
                | SyntaxKind::GreaterThanToken
                | SyntaxKind::GreaterThanEqualsToken
                | SyntaxKind::QuestionQuestionEqualsToken
                | SyntaxKind::LessThanLessThanToken
                | SyntaxKind::GreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                | SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::SlashToken
                | SyntaxKind::PercentToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::DotToken
                | SyntaxKind::MinusGreaterThanToken
                | SyntaxKind::QuestionQuestionToken
                | SyntaxKind::EndOfFileToken
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::EqualsGreaterThanToken
        )
    }

    /// `private ExpressionSyntax ParseNewExpression()` (LanguageParser.cs:13306).
    pub(crate) fn parse_new_expression(&mut self) -> Green {
        if self.is_anonymous_type() {
            self.parse_anonymous_type_expression()
        } else if self.is_implicitly_typed_array() {
            self.parse_implicitly_typed_array_creation()
        } else {
            // assume object creation as default case
            self.parse_array_or_object_creation_expression()
        }
    }

    /// `private CollectionExpressionSyntax ParseCollectionExpression()` (LanguageParser.cs:13325).
    pub(crate) fn parse_collection_expression(&mut self) -> Green {
        let mut open_bracket = self.eat_token_kind(SyntaxKind::OpenBracketToken);
        let list = self.parse_comma_separated_syntax_list(
            &mut open_bracket,
            SyntaxKind::CloseBracketToken,
            |p| p.is_possible_collection_element(),
            |p| p.parse_collection_element(),
            skip_bad_collection_element_tokens,
            true,
            false,
            false,
        );

        let close_bracket = self.eat_token_kind(SyntaxKind::CloseBracketToken);
        self.arena().collection_expression(open_bracket, list, close_bracket)
    }
}

/// `skipBadCollectionElementTokens` of `ParseCollectionExpression`.
fn skip_bad_collection_element_tokens(
    p: &mut LanguageParser<'_, '_>,
    open_bracket: &mut Green,
    list: &mut SyntaxListBuilder,
    expected_kind: SyntaxKind,
    close_kind: SyntaxKind,
) -> PostSkipAction {
    p.skip_bad_separated_list_tokens_with_expected_kind(
        open_bracket,
        list,
        |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_collection_element(),
        |p, close_kind| p.current_kind() == close_kind,
        expected_kind,
        close_kind,
    )
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsPossibleCollectionElement()` (LanguageParser.cs:13354).
    pub(crate) fn is_possible_collection_element(&mut self) -> bool {
        self.is_possible_expression()
    }

    /// `private CollectionElementSyntax ParseCollectionElement()` (LanguageParser.cs:13359).
    pub(crate) fn parse_collection_element(&mut self) -> Green {
        // Even though `with(` could start a legal expression (like `with(x) + y`), spec mandates that if we see
        // `with(` at the start of a collection element, we only parse it as a with-element.
        if self.current_contextual_kind() == SyntaxKind::WithKeyword && self.peek_kind(1) == SyntaxKind::OpenParenToken {
            let with_keyword = self.eat_contextual_token(SyntaxKind::WithKeyword);
            let argument_list = self.parse_parenthesized_argument_list();
            return self.arena().with_element(with_keyword, argument_list);
        }

        // Like above, even though `..` could start a legal expression (like `..` (a naked-range)), the spec
        // mandates that if we see `..` at the start of a collection element, we only parse it as a spread-element.
        if self.is_at_dot_dot_token() {
            let operator_token = self.eat_dot_dot_token();
            let expression = self.parse_expression_core();
            return self.arena().spread_element(operator_token, expression);
        }

        let expression = self.parse_expression_core();
        self.arena().expression_element(expression)
    }

    /// `private bool IsAnonymousType()` (LanguageParser.cs:13379).
    pub(crate) fn is_anonymous_type(&mut self) -> bool {
        self.current_kind() == SyntaxKind::NewKeyword && self.peek_kind(1) == SyntaxKind::OpenBraceToken
    }

    /// `private AnonymousObjectCreationExpressionSyntax ParseAnonymousTypeExpression()` (LanguageParser.cs:13384).
    pub(crate) fn parse_anonymous_type_expression(&mut self) -> Green {
        let new = self.eat_token_kind(SyntaxKind::NewKeyword);

        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        let expressions = self.parse_comma_separated_syntax_list(
            &mut open_brace,
            SyntaxKind::CloseBraceToken,
            |p| p.is_possible_expression(),
            |p| p.parse_anonymous_type_member_initializer(),
            |p, open, list, expected, close| p.skip_bad_initializer_list_tokens(open, list, expected, close),
            true,
            false,
            false,
        );

        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.arena().anonymous_object_creation_expression(new, open_brace, expressions, close_brace)
    }

    /// `private AnonymousObjectMemberDeclaratorSyntax ParseAnonymousTypeMemberInitializer()` (LanguageParser.cs:13409).
    pub(crate) fn parse_anonymous_type_member_initializer(&mut self) -> Green {
        let name_equals = if self.is_named_assignment() { self.parse_name_equals() } else { Green::NULL };
        let expression = self.parse_expression_core();
        self.arena().anonymous_object_member_declarator(name_equals, expression)
    }

    /// `private bool IsInitializerMember()` (LanguageParser.cs:13416).
    pub(crate) fn is_initializer_member(&mut self) -> bool {
        self.is_complex_element_initializer()
            || self.is_named_assignment()
            || self.is_dictionary_initializer()
            || self.is_possible_expression()
    }

    /// `private bool IsComplexElementInitializer()` (LanguageParser.cs:13424).
    pub(crate) fn is_complex_element_initializer(&mut self) -> bool {
        self.current_kind() == SyntaxKind::OpenBraceToken
    }

    /// `private bool IsNamedAssignment()` (LanguageParser.cs:13429).
    pub(crate) fn is_named_assignment(&mut self) -> bool {
        self.is_true_identifier() && self.peek_kind(1) == SyntaxKind::EqualsToken
    }

    /// `private bool IsNamedMemberInitializer()` (LanguageParser.cs:13434).
    pub(crate) fn is_named_member_initializer(&mut self) -> bool {
        self.is_true_identifier() && matches!(self.peek_kind(1), SyntaxKind::EqualsToken | SyntaxKind::ColonToken)
    }

    /// `private bool IsDictionaryInitializer()` (LanguageParser.cs:13439).
    pub(crate) fn is_dictionary_initializer(&mut self) -> bool {
        self.current_kind() == SyntaxKind::OpenBracketToken
    }

    /// `private ExpressionSyntax ParseArrayOrObjectCreationExpression()` (LanguageParser.cs:13444).
    pub(crate) fn parse_array_or_object_creation_expression(&mut self) -> Green {
        let new = self.eat_token_kind(SyntaxKind::NewKeyword);

        let mut type_ = Green::NULL;
        let mut initializer = Green::NULL;

        if !self.is_implicit_object_creation() {
            type_ = self.parse_type(ParseTypeMode::NewExpression);
            if self.arena_ref().kind(type_) == SyntaxKind::ArrayType {
                // Check for an initializer.
                if self.current_kind() == SyntaxKind::OpenBraceToken {
                    initializer = self.parse_array_initializer();
                }

                return self.arena().array_creation_expression(new, type_, initializer);
            }
        }

        let mut argument_list = Green::NULL;
        if self.current_kind() == SyntaxKind::OpenParenToken {
            argument_list = self.parse_parenthesized_argument_list();
        }

        if self.current_kind() == SyntaxKind::OpenBraceToken {
            initializer = self.parse_object_or_collection_initializer();
        }

        // we need one or the other.  also, don't bother reporting this if we already complained about the new type.
        if argument_list.is_null() && initializer.is_null() {
            // `type?.ContainsDiagnostics == false`: false for no type
            let report_error = !type_.is_null() && !self.arena_ref().contains_diagnostics(type_);
            let open_paren = self.eat_token_code(SyntaxKind::OpenParenToken, ErrorCode::ERR_BadNewExpr, report_error);
            let close_paren = self.arena().missing_token(SyntaxKind::CloseParenToken);
            argument_list = self.arena().argument_list(open_paren, Green::NULL, close_paren);
        }

        if type_.is_null() {
            self.arena().implicit_object_creation_expression(new, argument_list, initializer)
        } else {
            self.arena().object_creation_expression(new, type_, argument_list, initializer)
        }
    }

    /// `private bool IsImplicitObjectCreation()` (LanguageParser.cs:13491).
    pub(crate) fn is_implicit_object_creation(&mut self) -> bool {
        // The caller is expected to have consumed the new keyword.
        if self.current_kind() != SyntaxKind::OpenParenToken {
            return false;
        }

        let point = self.get_disposable_reset_point(true);

        self.eat_token(); // open paren
        let mut last_token_of_type = Green::NULL;
        let scan_type_flags = self.scan_tuple_type(&mut last_token_of_type);
        let result = if scan_type_flags != ScanTypeFlags::NOT_TYPE {
            !matches!(
                self.current_kind(),
                SyntaxKind::QuestionToken // e.g. `new(a, b)?()`
                    | SyntaxKind::OpenBracketToken // e.g. `new(a, b)[]`
                    | SyntaxKind::OpenParenToken // e.g. `new(a, b)()` for better error recovery
            )
        } else {
            true
        };
        self.dispose_reset_point(point);
        result
    }

    /// `private WithExpressionSyntax ParseWithExpression(ExpressionSyntax receiverExpression, SyntaxToken withKeyword)` (LanguageParser.cs:13519).
    pub(crate) fn parse_with_expression(&mut self, receiver_expression: Green, with_keyword: Green) -> Green {
        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        let list = self.parse_comma_separated_syntax_list(
            &mut open_brace,
            SyntaxKind::CloseBraceToken,
            |p| p.is_possible_expression(),
            |p| p.parse_expression_core(),
            |p, open, list, expected, close| p.skip_bad_initializer_list_tokens(open, list, expected, close),
            true,
            false,
            false,
        );

        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        let a = self.arena();
        let initializer = a.initializer_expression(SyntaxKind::WithInitializerExpression, open_brace, list, close_brace);
        a.with_expression(receiver_expression, with_keyword, initializer)
    }

    /// `private InitializerExpressionSyntax ParseObjectOrCollectionInitializer()` (LanguageParser.cs:13544). Nested
    /// initializers of named members recurse with no `ParseSubExpression` between, which Roslyn leaves unguarded: the
    /// recursion depth counts each level.
    pub(crate) fn parse_object_or_collection_initializer(&mut self) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();

        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);

        let initializers = self.parse_comma_separated_syntax_list(
            &mut open_brace,
            SyntaxKind::CloseBraceToken,
            |p| p.is_initializer_member(),
            |p| p.parse_object_or_collection_initializer_member(),
            |p, open, list, expected, close| p.skip_bad_initializer_list_tokens(open, list, expected, close),
            true,
            false,
            true,
        );

        let kind = if is_object_initializer(self.arena_ref(), initializers) {
            SyntaxKind::ObjectInitializerExpression
        } else {
            SyntaxKind::CollectionInitializerExpression
        };

        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        let result = self.arena().initializer_expression(kind, open_brace, initializers, close_brace);
        self.recursion_depth -= 1;
        result
    }
}

/// `isObjectInitializer` of `ParseObjectOrCollectionInitializer`.
fn is_object_initializer(a: &GreenArena, initializers: Green) -> bool {
    // Empty initializer list must be parsed as an object initializer.
    if initializers.is_null() {
        return true;
    }

    // We have at least one initializer expression. If at least one initializer expression is a named
    // assignment, this is an object initializer. Otherwise, this is a collection initializer.
    let count = list_count(a, initializers);
    (0..count).step_by(2).any(|i| {
        let initializer = list_item(a, initializers, i);
        a.kind(initializer) == SyntaxKind::SimpleAssignmentExpression
            && matches!(
                a.kind(a.slot(initializer, AssignmentExpressionSyntax::LEFT)),
                SyntaxKind::IdentifierName | SyntaxKind::ImplicitElementAccess
            )
    })
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ExpressionSyntax ParseObjectOrCollectionInitializerMember()` (LanguageParser.cs:13590).
    pub(crate) fn parse_object_or_collection_initializer_member(&mut self) -> Green {
        if self.is_complex_element_initializer() {
            // { ... }
            self.parse_complex_element_initializer()
        } else if self.is_dictionary_initializer() {
            // [...] = { ... }
            // [...] = ref <expr>
            // [...] = <expr>
            self.parse_dictionary_initializer()
        } else if self.is_named_member_initializer() {
            // Name = { ... }
            // Name = ref <expr>
            // Name =  <expr>
            self.parse_object_initializer_named_assignment()
        } else {
            // <expr>
            // ref <expr>
            self.parse_possible_ref_expression()
        }
    }

    /// `private static PostSkipAction SkipBadInitializerListTokens<T>( LanguageParser @this, ref SyntaxToken startToken, SeparatedSyntaxListBuilder<T> list, SyntaxKind expectedKind, SyntaxKind closeKind) where T : CSharpSyntaxNode` (LanguageParser.cs:13619).
    pub(crate) fn skip_bad_initializer_list_tokens(
        &mut self,
        start_token: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        self.skip_bad_separated_list_tokens_with_expected_kind(
            start_token,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_expression(),
            |p, close_kind| p.current_kind() == close_kind,
            expected_kind,
            close_kind,
        )
    }

    /// `private AssignmentExpressionSyntax ParseObjectInitializerNamedAssignment()` (LanguageParser.cs:13629).
    pub(crate) fn parse_object_initializer_named_assignment(&mut self) -> Green {
        let left = self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected);
        let operator_token = if self.current_kind() == SyntaxKind::ColonToken {
            self.eat_token_as_kind(SyntaxKind::EqualsToken)
        } else {
            self.eat_token_kind(SyntaxKind::EqualsToken)
        };
        let right = if self.current_kind() == SyntaxKind::OpenBraceToken {
            self.parse_object_or_collection_initializer()
        } else {
            self.parse_possible_ref_expression()
        };
        self.arena().assignment_expression(SyntaxKind::SimpleAssignmentExpression, left, operator_token, right)
    }

    /// `private AssignmentExpressionSyntax ParseDictionaryInitializer()` (LanguageParser.cs:13642).
    pub(crate) fn parse_dictionary_initializer(&mut self) -> Green {
        let argument_list = self.parse_bracketed_argument_list();
        let left = self.arena().implicit_element_access(argument_list);
        let operator_token = self.eat_token_kind(SyntaxKind::EqualsToken);
        let right = if self.current_kind() == SyntaxKind::OpenBraceToken {
            self.parse_object_or_collection_initializer()
        } else {
            self.parse_possible_ref_expression()
        };
        self.arena().assignment_expression(SyntaxKind::SimpleAssignmentExpression, left, operator_token, right)
    }

    /// `private InitializerExpressionSyntax ParseComplexElementInitializer()` (LanguageParser.cs:13653).
    pub(crate) fn parse_complex_element_initializer(&mut self) -> Green {
        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);

        let initializers = self.parse_comma_separated_syntax_list(
            &mut open_brace,
            SyntaxKind::CloseBraceToken,
            |p| p.is_possible_expression(),
            |p| p.parse_expression_core(),
            |p, open, list, expected, close| p.skip_bad_initializer_list_tokens(open, list, expected, close),
            false,
            false,
            false,
        );

        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.arena().initializer_expression(
            SyntaxKind::ComplexElementInitializerExpression,
            open_brace,
            initializers,
            close_brace,
        )
    }

    /// `private bool IsImplicitlyTypedArray()` (LanguageParser.cs:13674).
    pub(crate) fn is_implicitly_typed_array(&mut self) -> bool {
        self.peek_kind(1) == SyntaxKind::OpenBracketToken
    }

    /// `private ImplicitArrayCreationExpressionSyntax ParseImplicitlyTypedArrayCreation()` (LanguageParser.cs:13680).
    pub(crate) fn parse_implicitly_typed_array_creation(&mut self) -> Green {
        let new = self.eat_token_kind(SyntaxKind::NewKeyword);
        let mut open_bracket = self.eat_token_kind(SyntaxKind::OpenBracketToken);

        let mut commas = self.pool.allocate();

        let mut last_token_position = -1;
        while self.is_making_progress(&mut last_token_position, true) {
            if self.is_possible_expression() {
                let expression = self.parse_expression_core();
                let size = self.add_error(expression, ErrorCode::ERR_InvalidArray);
                if commas.is_empty() {
                    open_bracket = self.add_trailing_skipped_syntax(open_bracket, size);
                } else {
                    self.add_trailing_skipped_syntax_to_list(&mut commas, size);
                }
            }

            if self.current_kind() == SyntaxKind::CommaToken {
                let comma = self.eat_token();
                commas.push(comma);
                continue;
            }

            break;
        }

        let commas = to_list_and_free(self, commas);
        let close_bracket = self.eat_token_kind(SyntaxKind::CloseBracketToken);
        let initializer = self.parse_array_initializer();
        self.arena().implicit_array_creation_expression(new, open_bracket, commas, close_bracket, initializer)
    }

    /// `private InitializerExpressionSyntax ParseArrayInitializer()` (LanguageParser.cs:13720). Nested array
    /// initializers (through `ParseVariableInitializer`) recurse with no `ParseSubExpression` between, which Roslyn
    /// leaves unguarded: the recursion depth counts each level.
    pub(crate) fn parse_array_initializer(&mut self) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();

        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        let list = self.parse_comma_separated_syntax_list(
            &mut open_brace,
            SyntaxKind::CloseBraceToken,
            |p| p.is_possible_variable_initializer(),
            |p| p.parse_variable_initializer(),
            skip_bad_array_initializer_tokens,
            true,
            false,
            false,
        );

        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        let result =
            self.arena().initializer_expression(SyntaxKind::ArrayInitializerExpression, open_brace, list, close_brace);
        self.recursion_depth -= 1;
        result
    }
}

/// `skipBadArrayInitializerTokens` of `ParseArrayInitializer`.
fn skip_bad_array_initializer_tokens(
    p: &mut LanguageParser<'_, '_>,
    open_brace: &mut Green,
    list: &mut SyntaxListBuilder,
    expected_kind: SyntaxKind,
    close_kind: SyntaxKind,
) -> PostSkipAction {
    p.skip_bad_separated_list_tokens_with_expected_kind(
        open_brace,
        list,
        |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_variable_initializer(),
        |p, close_kind| p.current_kind() == close_kind,
        expected_kind,
        close_kind,
    )
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ExpressionSyntax ParseStackAllocExpression()` (LanguageParser.cs:13749).
    pub(crate) fn parse_stack_alloc_expression(&mut self) -> Green {
        if self.is_implicitly_typed_array() {
            self.parse_implicitly_typed_stack_alloc_expression()
        } else {
            self.parse_regular_stack_alloc_expression()
        }
    }

    /// `private ExpressionSyntax ParseImplicitlyTypedStackAllocExpression()` (LanguageParser.cs:13756).
    pub(crate) fn parse_implicitly_typed_stack_alloc_expression(&mut self) -> Green {
        let stackalloc = self.eat_token_kind(SyntaxKind::StackAllocKeyword);
        let mut open_bracket = self.eat_token_kind(SyntaxKind::OpenBracketToken);

        let mut last_token_position = -1;
        while self.is_making_progress(&mut last_token_position, true) {
            if self.is_possible_expression() {
                let expression = self.parse_expression_core();
                let size = self.add_error(expression, ErrorCode::ERR_InvalidStackAllocArray);
                open_bracket = self.add_trailing_skipped_syntax(open_bracket, size);
            }

            if self.current_kind() == SyntaxKind::CommaToken {
                let comma = self.eat_token();
                let comma = self.add_error(comma, ErrorCode::ERR_InvalidStackAllocArray);
                open_bracket = self.add_trailing_skipped_syntax(open_bracket, comma);
                continue;
            }

            break;
        }

        let close_bracket = self.eat_token_kind(SyntaxKind::CloseBracketToken);
        let initializer = self.parse_array_initializer();
        self.arena().implicit_stack_alloc_array_creation_expression(stackalloc, open_bracket, close_bracket, initializer)
    }

    /// `private ExpressionSyntax ParseRegularStackAllocExpression()` (LanguageParser.cs:13787).
    pub(crate) fn parse_regular_stack_alloc_expression(&mut self) -> Green {
        let stackalloc = self.eat_token_kind(SyntaxKind::StackAllocKeyword);
        let type_ = self.parse_type(ParseTypeMode::Normal);
        let initializer =
            if self.current_kind() == SyntaxKind::OpenBraceToken { self.parse_array_initializer() } else { Green::NULL };
        self.arena().stack_alloc_array_creation_expression(stackalloc, type_, initializer)
    }

    /// `private AnonymousMethodExpressionSyntax ParseAnonymousMethodExpression()` (LanguageParser.cs:13795).
    pub(crate) fn parse_anonymous_method_expression(&mut self) -> Green {
        let modifiers = self.parse_anonymous_function_modifiers();
        let is_in_async = self.is_in_async || list_any(self.arena_ref(), modifiers, SyntaxKind::AsyncKeyword);
        let resetter = ParserSyntaxContextResetter::new(self, Some(is_in_async), None, None, Some(false));

        let result = parse_anonymous_method_expression_worker(self, modifiers);
        resetter.dispose(self);
        result
    }
}

/// `parseAnonymousMethodExpressionWorker` of `ParseAnonymousMethodExpression`.
fn parse_anonymous_method_expression_worker(p: &mut LanguageParser<'_, '_>, modifiers: Green) -> Green {
    let delegate = p.eat_token_kind(SyntaxKind::DelegateKeyword);

    let mut parameter_list = Green::NULL;
    if p.current_kind() == SyntaxKind::OpenParenToken {
        parameter_list = p.parse_parenthesized_parameter_list(false);
    }

    // In mismatched braces cases (missing a }) it is possible for delegate declarations to be
    // parsed as delegate statement expressions.  When this situation occurs all subsequent
    // delegate declarations will also be parsed as delegate statement expressions.  In a file with
    // a sufficient number of delegates, common in generated code, it will put considerable
    // stack pressure on the parser.
    //
    // To help avoid this problem we don't recursively descend into a delegate expression unless
    // { } are actually present.  This keeps the stack pressure lower in bad code scenarios.
    if p.current_kind() != SyntaxKind::OpenBraceToken {
        // There's a special error code for a missing token after an accessor keyword
        let open_brace = p.eat_token_kind(SyntaxKind::OpenBraceToken);
        let a = p.arena();
        let close_brace = a.missing_token(SyntaxKind::CloseBraceToken);
        let block = a.block(Green::NULL, open_brace, Green::NULL, close_brace);
        return a.anonymous_method_expression(modifiers, delegate, parameter_list, block, Green::NULL);
    }

    let block = p.parse_block(Green::NULL);
    p.arena().anonymous_method_expression(modifiers, delegate, parameter_list, block, Green::NULL)
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private SyntaxList<SyntaxToken> ParseAnonymousFunctionModifiers()` (LanguageParser.cs:13847).
    pub(crate) fn parse_anonymous_function_modifiers(&mut self) -> Green {
        let mut modifiers = self.pool.allocate();

        loop {
            if self.current_kind() == SyntaxKind::StaticKeyword {
                let token = self.eat_token_kind(SyntaxKind::StaticKeyword);
                modifiers.push(token);
                continue;
            }

            if self.current_contextual_kind() == SyntaxKind::AsyncKeyword && self.is_anonymous_function_async_modifier() {
                let token = self.eat_contextual_token(SyntaxKind::AsyncKeyword);
                modifiers.push(token);
                continue;
            }

            break;
        }

        to_list_and_free(self, modifiers)
    }

    /// `private bool IsAnonymousFunctionAsyncModifier()` (LanguageParser.cs:13872).
    pub(crate) fn is_anonymous_function_async_modifier(&mut self) -> bool {
        match self.peek_kind(1) {
            SyntaxKind::OpenParenToken
            | SyntaxKind::IdentifierToken
            | SyntaxKind::StaticKeyword
            | SyntaxKind::RefKeyword
            | SyntaxKind::DelegateKeyword => true,
            kind => Self::is_predefined_type(kind),
        }
    }

    /// `private LambdaExpressionSyntax TryParseLambdaExpression()` (LanguageParser.cs:13894): parse expected lambda
    /// expression but assume `x ? () => y :` is a conditional expression rather than a lambda expression with an
    /// explicit return type and return null in that case only.
    pub(crate) fn try_parse_lambda_expression(&mut self) -> Green {
        let mut reset_point = self.get_disposable_reset_point(false);
        let mut result = self.parse_lambda_expression();

        if self.current_kind() == SyntaxKind::ColonToken {
            let a = self.arena_ref();
            if a.kind(result) == SyntaxKind::ParenthesizedLambdaExpression
                && a.kind(a.slot(result, ParenthesizedLambdaExpressionSyntax::RETURN_TYPE)) == SyntaxKind::NullableType
            {
                self.reset_disposable(&mut reset_point);
                result = Green::NULL;
            }
        }

        self.dispose_reset_point(reset_point);
        result
    }

    /// `private LambdaExpressionSyntax ParseLambdaExpression()` (LanguageParser.cs:13909).
    pub(crate) fn parse_lambda_expression(&mut self) -> Green {
        let attributes = self.parse_attribute_declarations(true);

        let modifiers = self.parse_anonymous_function_modifiers();

        let is_in_async = self.is_in_async || list_any(self.arena_ref(), modifiers, SyntaxKind::AsyncKeyword);
        let resetter = ParserSyntaxContextResetter::new(self, Some(is_in_async), None, None, Some(false));

        let result = parse_lambda_expression_worker(self, attributes, modifiers);
        resetter.dispose(self);
        result
    }
}

/// `parseLambdaExpressionWorker` of `ParseLambdaExpression`.
fn parse_lambda_expression_worker(p: &mut LanguageParser<'_, '_>, attributes: Green, modifiers: Green) -> Green {
    let mut return_type;
    {
        let mut reset_point = p.get_disposable_reset_point(false);
        return_type = p.parse_return_type();
        if p.current_kind() != SyntaxKind::OpenParenToken {
            p.reset_disposable(&mut reset_point);
            return_type = Green::NULL;
        }
        p.dispose_reset_point(reset_point);
    }

    if p.current_kind() == SyntaxKind::OpenParenToken {
        let param_list = p.parse_lambda_parameter_list();
        let arrow = p.eat_token_kind(SyntaxKind::EqualsGreaterThanToken);
        let (block, expression) = p.parse_lambda_body();

        p.arena().parenthesized_lambda_expression(attributes, modifiers, return_type, param_list, arrow, block, expression)
    } else {
        // Unparenthesized lambda case
        // x => ...
        let identifier = if p.current_kind() != SyntaxKind::IdentifierToken
            && p.peek_kind(1) == SyntaxKind::EqualsGreaterThanToken
        {
            p.eat_token_as_kind(SyntaxKind::IdentifierToken)
        } else {
            p.parse_identifier_token(ErrorCode::ERR_IdentifierExpected)
        };

        // Case x=>, x =>
        let arrow = p.eat_token_kind(SyntaxKind::EqualsGreaterThanToken);

        let parameter = p.arena().parameter(Green::NULL, Green::NULL, Green::NULL, identifier, Green::NULL);
        let (block, expression) = p.parse_lambda_body();
        p.arena().simple_lambda_expression(attributes, modifiers, parameter, arrow, block, expression)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TUPLE ParseLambdaBody()` (LanguageParser.cs:13963): the block or the expression.
    pub(crate) fn parse_lambda_body(&mut self) -> (Green, Green) {
        if self.current_kind() == SyntaxKind::OpenBraceToken {
            (self.parse_block(Green::NULL), Green::NULL)
        } else {
            (Green::NULL, self.parse_possible_ref_expression())
        }
    }

    /// `private ParameterListSyntax ParseLambdaParameterList()` (LanguageParser.cs:13968).
    pub(crate) fn parse_lambda_parameter_list(&mut self) -> Green {
        let mut open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_PARAMETER_LIST;

        let nodes = self.parse_comma_separated_syntax_list(
            &mut open_paren,
            SyntaxKind::CloseParenToken,
            |p| p.is_possible_lambda_parameter(),
            |p| p.parse_lambda_parameter(),
            skip_bad_lambda_parameter_list_tokens,
            false,
            false,
            false,
        );

        self.term_state = save_term;

        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.arena().parameter_list(open_paren, nodes, close_paren)
    }
}

/// `skipBadLambdaParameterListTokens` of `ParseLambdaParameterList`.
fn skip_bad_lambda_parameter_list_tokens(
    p: &mut LanguageParser<'_, '_>,
    open_paren: &mut Green,
    list: &mut SyntaxListBuilder,
    expected_kind: SyntaxKind,
    close_kind: SyntaxKind,
) -> PostSkipAction {
    p.skip_bad_separated_list_tokens_with_expected_kind(
        open_paren,
        list,
        |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_lambda_parameter(),
        |p, close_kind| p.current_kind() == close_kind,
        expected_kind,
        close_kind,
    )
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsPossibleLambdaParameter()` (LanguageParser.cs:14001).
    pub(crate) fn is_possible_lambda_parameter(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::ParamsKeyword
            | SyntaxKind::ReadOnlyKeyword
            | SyntaxKind::RefKeyword
            | SyntaxKind::OutKeyword
            | SyntaxKind::InKeyword
            | SyntaxKind::OpenParenToken // tuple
            | SyntaxKind::OpenBracketToken // attribute
            => true,

            SyntaxKind::IdentifierToken => self.is_true_identifier(),

            SyntaxKind::DelegateKeyword => self.is_function_pointer_start(),

            kind => Self::is_predefined_type(kind),
        }
    }

    /// `private ParameterSyntax ParseLambdaParameter()` (LanguageParser.cs:14025).
    pub(crate) fn parse_lambda_parameter(&mut self) -> Green {
        let attributes = self.parse_attribute_declarations(false);

        // Params are actually illegal in a lambda, but we'll allow it for error recovery purposes and
        // give the "params unexpected" error at semantic analysis time.
        let mut modifiers = self.pool.allocate();
        let current = self.current_token();
        if self.is_parameter_modifier_including_scoped(current) {
            self.parse_parameter_modifiers(&mut modifiers, false, true);
        }

        let param_type =
            if self.should_parse_lambda_parameter_type() { self.parse_type(ParseTypeMode::Parameter) } else { Green::NULL };

        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);

        // Parse default value if any
        let equals_token = self.try_eat_token(SyntaxKind::EqualsToken);

        let modifiers = to_list_and_free(self, modifiers);
        let default = if !equals_token.is_null() {
            let value = self.parse_expression_core();
            self.arena().equals_value_clause(equals_token, value)
        } else {
            Green::NULL
        };
        self.arena().parameter(attributes, modifiers, param_type, identifier, default)
    }

    /// `private bool ShouldParseLambdaParameterType()` (LanguageParser.cs:14056).
    pub(crate) fn should_parse_lambda_parameter_type(&mut self) -> bool {
        // If we have "int/string/etc." always parse out a type.
        if Self::is_predefined_type(self.current_kind()) {
            return true;
        }

        // if we have a tuple type in a lambda.
        if self.current_kind() == SyntaxKind::OpenParenToken {
            return true;
        }

        if self.is_function_pointer_start() {
            return true;
        }

        let current = self.current_token();
        if self.is_true_identifier_with_token(current) {
            // Don't parse out a type if we see:
            //
            //      (a,
            //      (a)
            //      (a =>
            //      (a {
            //      (a =
            //
            // In all other cases, parse out a type.
            if !matches!(
                self.peek_kind(1),
                SyntaxKind::CommaToken
                    | SyntaxKind::CloseParenToken
                    | SyntaxKind::EqualsGreaterThanToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::EqualsToken
            ) {
                return true;
            }
        }

        false
    }

    /// `private bool IsCurrentTokenQueryContextualKeyword` (LanguageParser.cs:14101).
    pub(crate) fn is_current_token_query_contextual_keyword(&mut self) -> bool {
        let token = self.current_token();
        self.is_token_query_contextual_keyword(token)
    }

    /// `private static bool IsTokenQueryContextualKeyword(SyntaxToken token)` (LanguageParser.cs:14104).
    pub(crate) fn is_token_query_contextual_keyword(&self, token: Green) -> bool {
        if self.is_token_start_of_new_query_clause(token) {
            return true;
        }

        matches!(
            self.arena_ref().contextual_kind(token),
            SyntaxKind::OnKeyword
                | SyntaxKind::EqualsKeyword
                | SyntaxKind::AscendingKeyword
                | SyntaxKind::DescendingKeyword
                | SyntaxKind::ByKeyword
        )
    }

    /// `private static bool IsTokenStartOfNewQueryClause(SyntaxToken token)` (LanguageParser.cs:14124).
    pub(crate) fn is_token_start_of_new_query_clause(&self, token: Green) -> bool {
        matches!(
            self.arena_ref().contextual_kind(token),
            SyntaxKind::FromKeyword
                | SyntaxKind::JoinKeyword
                | SyntaxKind::IntoKeyword
                | SyntaxKind::WhereKeyword
                | SyntaxKind::OrderByKeyword
                | SyntaxKind::GroupKeyword
                | SyntaxKind::SelectKeyword
                | SyntaxKind::LetKeyword
        )
    }

    /// `private bool IsQueryExpression(bool mayBeVariableDeclaration, bool mayBeMemberDeclaration)` (LanguageParser.cs:14142).
    pub(crate) fn is_query_expression(
        &mut self,
        may_be_variable_declaration: bool,
        may_be_member_declaration: bool,
    ) -> bool {
        self.current_contextual_kind() == SyntaxKind::FromKeyword
            && self.is_query_expression_after_from(may_be_variable_declaration, may_be_member_declaration)
    }

    /// `private bool IsQueryExpressionAfterFrom(bool mayBeVariableDeclaration, bool mayBeMemberDeclaration)` (LanguageParser.cs:14149):
    /// from_clause ::= from <type>? <identifier> in expression
    pub(crate) fn is_query_expression_after_from(
        &mut self,
        may_be_variable_declaration: bool,
        may_be_member_declaration: bool,
    ) -> bool {
        // from x ...
        let pk1 = self.peek_kind(1);
        if Self::is_predefined_type(pk1) {
            return true;
        }

        if pk1 == SyntaxKind::IdentifierToken {
            let pk2 = self.peek_kind(2);
            if pk2 == SyntaxKind::InKeyword {
                return true;
            }

            if may_be_variable_declaration
                && matches!(
                    pk2,
                    SyntaxKind::SemicolonToken // from x;
                        | SyntaxKind::CommaToken // from x, y;
                        | SyntaxKind::EqualsToken // from x = null;
                )
            {
                return false;
            }

            if may_be_member_declaration {
                // from idf { ...   property decl
                // from idf(...     method decl
                if matches!(pk2, SyntaxKind::OpenParenToken | SyntaxKind::OpenBraceToken) {
                    return false;
                }

                // otherwise we need to scan a type
            } else {
                return true;
            }
        }

        // from T x ...
        let point = self.get_disposable_reset_point(true);

        self.eat_token();
        let result = self.scan_type(false) != ScanTypeFlags::NOT_TYPE
            && matches!(self.current_kind(), SyntaxKind::IdentifierToken | SyntaxKind::InKeyword);
        self.dispose_reset_point(point);
        result
    }

    /// `private QueryExpressionSyntax ParseQueryExpression(Precedence precedence)` (LanguageParser.cs:14200).
    pub(crate) fn parse_query_expression(&mut self, precedence: Precedence) -> Green {
        let resetter = ParserSyntaxContextResetter::new(self, None, Some(true), None, None);
        let from_clause = self.parse_from_clause();

        let from_clause = if precedence <= Precedence::ASSIGNMENT {
            from_clause
        } else {
            self.add_error_args(
                from_clause,
                ErrorCode::WRN_PrecedenceInversion,
                &[DiagArg::Str(facts::get_text(SyntaxKind::FromKeyword))],
            )
        };
        let body = self.parse_query_body();
        let result = self.arena().query_expression(from_clause, body);
        resetter.dispose(self);
        result
    }

    /// `private QueryBodySyntax ParseQueryBody()` (LanguageParser.cs:14212). A query continuation recurses into
    /// another body with no `ParseSubExpression` between, which Roslyn leaves unguarded: the recursion depth counts
    /// each level.
    pub(crate) fn parse_query_body(&mut self) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();

        let mut clauses = self.pool.allocate();

        // from, join, let, where and orderby
        loop {
            let clause = match self.current_contextual_kind() {
                SyntaxKind::FromKeyword => self.parse_from_clause(),
                SyntaxKind::JoinKeyword => self.parse_join_clause(),
                SyntaxKind::LetKeyword => self.parse_let_clause(),
                SyntaxKind::WhereKeyword => self.parse_where_clause(),
                SyntaxKind::OrderByKeyword => self.parse_order_by_clause(),
                _ => break,
            };
            clauses.push(clause);
        }

        // select or group clause
        let select_or_group_by = match self.current_contextual_kind() {
            SyntaxKind::SelectKeyword => self.parse_select_clause(),
            SyntaxKind::GroupKeyword => self.parse_group_clause(),
            _ => {
                let select_keyword =
                    self.eat_token_code(SyntaxKind::SelectKeyword, ErrorCode::ERR_ExpectedSelectOrGroup, true);
                let expression = self.create_missing_identifier_name();
                self.arena().select_clause(select_keyword, expression)
            }
        };

        let clauses = to_list_and_free(self, clauses);
        let continuation = if self.current_contextual_kind() == SyntaxKind::IntoKeyword {
            self.parse_query_continuation()
        } else {
            Green::NULL
        };
        let result = self.arena().query_body(clauses, select_or_group_by, continuation);
        self.recursion_depth -= 1;
        result
    }

    /// `private FromClauseSyntax ParseFromClause()` (LanguageParser.cs:14259).
    pub(crate) fn parse_from_clause(&mut self) -> Green {
        let from = self.eat_contextual_token(SyntaxKind::FromKeyword);

        let type_ =
            if self.peek_kind(1) != SyntaxKind::InKeyword { self.parse_type(ParseTypeMode::Normal) } else { Green::NULL };

        let peek1 = self.peek_token(1);
        let name = if self.arena_ref().contextual_kind(peek1) == SyntaxKind::InKeyword
            && (self.current_kind() != SyntaxKind::IdentifierToken
                || facts::is_query_contextual_keyword(self.current_contextual_kind()))
        {
            //if this token is a something other than an identifier (someone accidentally used a contextual
            //keyword or a literal, for example), but we can see that the "in" is in the right place, then
            //just replace whatever is here with a missing identifier
            let name = self.eat_token();
            let (contextual_kind, leading, width) = {
                let a = self.arena_ref();
                (a.contextual_kind(name), a.leading_trivia_width(name) as i32, a.width(name) as i32)
            };
            let error = self.get_expected_token_error(SyntaxKind::IdentifierToken, contextual_kind, leading, width);
            let name = self.with_additional_diagnostics(name, &[error]);
            self.convert_to_missing_with_trailing_trivia(name, SyntaxKind::IdentifierToken)
        } else {
            self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected)
        };

        let in_keyword = self.eat_token_kind(SyntaxKind::InKeyword);
        let expression = self.parse_expression_core();
        self.arena().from_clause(from, type_, name, in_keyword, expression)
    }

    /// `private JoinClauseSyntax ParseJoinClause()` (LanguageParser.cs:14292).
    pub(crate) fn parse_join_clause(&mut self) -> Green {
        let join_keyword = self.eat_contextual_token(SyntaxKind::JoinKeyword);
        let type_ =
            if self.peek_kind(1) != SyntaxKind::InKeyword { self.parse_type(ParseTypeMode::Normal) } else { Green::NULL };
        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let in_keyword = self.eat_token_kind(SyntaxKind::InKeyword);
        let in_expression = self.parse_expression_core();
        let on_keyword = self.eat_contextual_token_code(SyntaxKind::OnKeyword, ErrorCode::ERR_ExpectedContextualKeywordOn);
        let left_expression = self.parse_expression_core();
        let equals_keyword =
            self.eat_contextual_token_code(SyntaxKind::EqualsKeyword, ErrorCode::ERR_ExpectedContextualKeywordEquals);
        let right_expression = self.parse_expression_core();
        let into = if self.current_contextual_kind() == SyntaxKind::IntoKeyword {
            let into_keyword = self.eat_token();
            let into_keyword = self.convert_to_keyword(into_keyword);
            let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
            self.arena().join_into_clause(into_keyword, identifier)
        } else {
            Green::NULL
        };
        self.arena().join_clause(
            join_keyword,
            type_,
            identifier,
            in_keyword,
            in_expression,
            on_keyword,
            left_expression,
            equals_keyword,
            right_expression,
            into,
        )
    }

    /// `private LetClauseSyntax ParseLetClause()` (LanguageParser.cs:14312).
    pub(crate) fn parse_let_clause(&mut self) -> Green {
        let let_keyword = self.eat_contextual_token(SyntaxKind::LetKeyword);
        // If we see a keyword followed by '=', use EatTokenAsKind to produce a better error message and recover well.
        let identifier =
            if facts::is_reserved_keyword(self.current_kind()) && self.peek_kind(1) == SyntaxKind::EqualsToken {
                self.eat_token_as_kind(SyntaxKind::IdentifierToken)
            } else {
                self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected)
            };
        let equals_token = self.eat_token_kind(SyntaxKind::EqualsToken);
        let expression = self.parse_expression_core();
        self.arena().let_clause(let_keyword, identifier, equals_token, expression)
    }

    /// `private WhereClauseSyntax ParseWhereClause()` (LanguageParser.cs:14325).
    pub(crate) fn parse_where_clause(&mut self) -> Green {
        let where_keyword = self.eat_contextual_token(SyntaxKind::WhereKeyword);
        let condition = self.parse_expression_core();
        self.arena().where_clause(where_keyword, condition)
    }

    /// `private OrderByClauseSyntax ParseOrderByClause()` (LanguageParser.cs:14333).
    pub(crate) fn parse_order_by_clause(&mut self) -> Green {
        let orderby = self.eat_contextual_token(SyntaxKind::OrderByKeyword);

        let mut list = self.pool.allocate();
        // first argument
        let ordering = self.parse_ordering();
        list.push(ordering);

        // additional arguments
        while self.current_kind() == SyntaxKind::CommaToken {
            if matches!(self.current_kind(), SyntaxKind::CloseParenToken | SyntaxKind::SemicolonToken) {
                break;
            } else if self.current_kind() == SyntaxKind::CommaToken {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                list.push(comma);
                let ordering = self.parse_ordering();
                list.push(ordering);
                continue;
            } else if skip_bad_ordering_list_tokens(self, &mut list, SyntaxKind::CommaToken) == PostSkipAction::Abort {
                break;
            }
        }

        let orderings = to_list_and_free(self, list);
        self.arena().order_by_clause(orderby, orderings)
    }
}

/// `skipBadOrderingListTokens` of `ParseOrderByClause`.
fn skip_bad_ordering_list_tokens(
    p: &mut LanguageParser<'_, '_>,
    list: &mut SyntaxListBuilder,
    expected: SyntaxKind,
) -> PostSkipAction {
    let mut tmp = Green::NULL;
    p.skip_bad_separated_list_tokens_with_expected_kind(
        &mut tmp,
        list,
        |p| p.current_kind() != SyntaxKind::CommaToken,
        |p, _| {
            p.current_kind() == SyntaxKind::CloseParenToken
                || p.current_kind() == SyntaxKind::SemicolonToken
                || p.is_current_token_query_contextual_keyword()
        },
        expected,
        SyntaxKind::None,
    )
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private OrderingSyntax ParseOrdering()` (LanguageParser.cs:14378).
    pub(crate) fn parse_ordering(&mut self) -> Green {
        let expression = self.parse_expression_core();
        let mut direction = Green::NULL;
        let mut kind = SyntaxKind::AscendingOrdering;

        if matches!(self.current_contextual_kind(), SyntaxKind::AscendingKeyword | SyntaxKind::DescendingKeyword) {
            let token = self.eat_token();
            direction = self.convert_to_keyword(token);
            if self.arena_ref().kind(direction) == SyntaxKind::DescendingKeyword {
                kind = SyntaxKind::DescendingOrdering;
            }
        }

        self.arena().ordering(kind, expression, direction)
    }

    /// `private SelectClauseSyntax ParseSelectClause()` (LanguageParser.cs:14396).
    pub(crate) fn parse_select_clause(&mut self) -> Green {
        let select_keyword = self.eat_contextual_token(SyntaxKind::SelectKeyword);
        let expression = self.parse_expression_core();
        self.arena().select_clause(select_keyword, expression)
    }

    /// `private GroupClauseSyntax ParseGroupClause()` (LanguageParser.cs:14404).
    pub(crate) fn parse_group_clause(&mut self) -> Green {
        let group_keyword = self.eat_contextual_token(SyntaxKind::GroupKeyword);
        let group_expression = self.parse_expression_core();
        let by_keyword = self.eat_contextual_token_code(SyntaxKind::ByKeyword, ErrorCode::ERR_ExpectedContextualKeywordBy);
        let by_expression = self.parse_expression_core();
        self.arena().group_clause(group_keyword, group_expression, by_keyword, by_expression)
    }

    /// `private QueryContinuationSyntax ParseQueryContinuation()` (LanguageParser.cs:14414).
    pub(crate) fn parse_query_continuation(&mut self) -> Green {
        let into_keyword = self.eat_contextual_token(SyntaxKind::IntoKeyword);
        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let body = self.parse_query_body();
        self.arena().query_continuation(into_keyword, identifier, body)
    }
}
