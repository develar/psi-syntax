//! The port of `KotlinExpressionParsing.java` (expressions, statements, string templates, `when`, lambdas), and of
//! `BinaryOperationPrecedence` (psi-api) it parses binary operations with.
//!
//! `KotlinExpressionParsing` is a view of the `KotlinParsing` that owns it: the Java object pair (each holding the
//! other and the same builder) is one `KotlinParsing` plus this borrow of it. `myKotlinParsing.x()` is
//! `self.my_kotlin_parsing.x()`; `KotlinParsing`'s `myExpressionParsing.y()` is `self.my_expression_parsing().y()`.
//! The anonymous subclass `KotlinParsing` creates for a by-clause (`isTopLevel == false`) is the same type: its
//! overrides `parseCallWithClosure` and `create` check `my_kotlin_parsing.is_top_level`.
//!
//! Every method is ported in the Java order (KotlinExpressionParsing.java of Kotlin 2.4.21). A Java `assert` is a
//! `debug_assert!`; a `TokenSet.create(..)` built inline in a Java method is a `const` here.
//!
//! Naming of Java overloads: `parseExpression(String)` = `parse_expression_with`, `parseFunctionLiteral(boolean,
//! boolean)` = `parse_function_literal_with`, `parseStatements(boolean)` = `parse_statements_with`. A Java
//! `IElementType` result is `Option<NodeKind>`; `String` parameters that only ever receive literals are `&'static str`.

// The Java names of the enum constants are kept.
#![allow(non_camel_case_types)]

use crate::builder::{
    Adapter, Binder, Marker, PRECEDING_ALL_COMMENTS_BINDER, SemanticWhitespaceAwarePsiBuilder,
    TRAILING_ALL_COMMENTS_BINDER,
};
use crate::node_kind::NodeKind;
use crate::node_kind::NodeKind::*;
use crate::parsing::abstract_parsing::AbstractKotlinParsing;
use crate::parsing::kotlin_parsing::{
    AnnotationParsingMode, DeclarationParsingMode, EXPRESSION_FIRST, EXPRESSION_FOLLOW, KotlinParsing,
    ModifierDetector, MultiDeclarationMode, NameParsingMode, PARAMETER_NAME_RECOVERY_SET,
};
use crate::tokens::TokenKind::*;
use crate::tokens::{
    KEYWORDS, MODIFIER_KEYWORDS, POSTFIX_OPERATIONS, PREFIX_OPERATIONS, SOFT_KEYWORDS, TokenKind, TokenSet,
    WHITE_SPACE_OR_COMMENT_BIT_SET,
};

/// `org.jetbrains.kotlin.lang.BinaryOperationPrecedence`, from the highest priority (`AS`) to the lowest
/// (`ASSIGNMENT`); `ordinal()` is `as usize`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum BinaryOperationPrecedence {
    AS,
    MULTIPLICATIVE,
    ADDITIVE,
    RANGE,
    INFIX,
    ELVIS,
    IN_OR_IS,
    COMPARISON,
    EQUALITY,
    CONJUNCTION,
    DISJUNCTION,
    ASSIGNMENT,
}

impl BinaryOperationPrecedence {
    /// `getEntries()`.
    pub const ENTRIES: [BinaryOperationPrecedence; 12] = {
        use BinaryOperationPrecedence::*;
        [
            AS,
            MULTIPLICATIVE,
            ADDITIVE,
            RANGE,
            INFIX,
            ELVIS,
            IN_OR_IS,
            COMPARISON,
            EQUALITY,
            CONJUNCTION,
            DISJUNCTION,
            ASSIGNMENT,
        ]
    };

    /// `ordinal()`.
    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// `getHigherPriority()`: the next precedence towards `AS`; `None` for `AS`.
    pub const fn get_higher_priority(self) -> Option<BinaryOperationPrecedence> {
        use BinaryOperationPrecedence::*;
        match self {
            AS => None,
            MULTIPLICATIVE => Some(AS),
            ADDITIVE => Some(MULTIPLICATIVE),
            RANGE => Some(ADDITIVE),
            INFIX => Some(RANGE),
            ELVIS => Some(INFIX),
            IN_OR_IS => Some(ELVIS),
            COMPARISON => Some(IN_OR_IS),
            EQUALITY => Some(COMPARISON),
            CONJUNCTION => Some(EQUALITY),
            DISJUNCTION => Some(CONJUNCTION),
            ASSIGNMENT => Some(DISJUNCTION),
        }
    }

    /// `getTokens()`.
    pub const fn tokens(self) -> &'static [TokenKind] {
        use BinaryOperationPrecedence::*;
        match self {
            AS => &[AS_KEYWORD, AS_SAFE],
            MULTIPLICATIVE => &[MUL, DIV, PERC],
            ADDITIVE => &[PLUS, MINUS],
            RANGE => &[TokenKind::RANGE, RANGE_UNTIL],
            INFIX => &[IDENTIFIER],
            ELVIS => &[TokenKind::ELVIS],
            IN_OR_IS => &[IN_KEYWORD, NOT_IN, IS_KEYWORD, NOT_IS],
            COMPARISON => &[LT, GT, LTEQ, GTEQ],
            EQUALITY => &[EQEQ, EXCLEQ, EQEQEQ, EXCLEQEQEQ],
            CONJUNCTION => &[ANDAND],
            DISJUNCTION => &[OROR],
            ASSIGNMENT => &[EQ, PLUSEQ, MINUSEQ, MULTEQ, DIVEQ, PERCEQ],
        }
    }

    /// `TOKEN_TO_BINARY_PRECEDENCE_MAP_WITH_SOFT_IDENTIFIERS.get(token)`: soft keywords are `INFIX` like `IDENTIFIER`.
    pub fn token_to_binary_precedence_with_soft_identifiers(
        token: Option<TokenKind>,
    ) -> Option<BinaryOperationPrecedence> {
        let token = token?;
        if SOFT_KEYWORDS.contains(token) {
            return Some(BinaryOperationPrecedence::INFIX);
        }
        Self::token_to_binary_precedence(token)
    }

    /// `TOKEN_TO_BINARY_PRECEDENCE_MAP.get(token)`.
    pub fn token_to_binary_precedence(token: TokenKind) -> Option<BinaryOperationPrecedence> {
        Self::ENTRIES.iter().copied().find(|p| p.tokens().contains(&token))
    }
}

const WHEN_CONDITION_RECOVERY_SET: TokenSet =
    TokenSet::create(&[RBRACE, IN_KEYWORD, NOT_IN, IS_KEYWORD, NOT_IS, ELSE_KEYWORD]);
const WHEN_CONDITION_RECOVERY_SET_WITH_ARROW: TokenSet =
    TokenSet::create(&[RBRACE, IN_KEYWORD, NOT_IN, IS_KEYWORD, NOT_IS, ELSE_KEYWORD, ARROW, DOT]);

const TOKEN_SET_TO_FOLLOW_AFTER_DESTRUCTURING_DECLARATION_IN_LAMBDA: TokenSet =
    TokenSet::create(&[ARROW, COMMA, COLON]);
const TOKEN_SET_TO_FOLLOW_AFTER_DESTRUCTURING_DECLARATION_IN_LAMBDA_RECOVERY: TokenSet =
    TokenSet::or_set(&[TOKEN_SET_TO_FOLLOW_AFTER_DESTRUCTURING_DECLARATION_IN_LAMBDA, PARAMETER_NAME_RECOVERY_SET]);
const EQ_RPAR_SET: TokenSet = TokenSet::create(&[EQ, RPAR]);
const ARROW_SET: TokenSet = TokenSet::create(&[ARROW]);
const ARROW_COMMA_SET: TokenSet = TokenSet::create(&[ARROW, COMMA]);
const IN_KEYWORD_R_PAR_COLON_SET: TokenSet = TokenSet::create(&[IN_KEYWORD, RPAR, COLON]);
const IN_KEYWORD_L_BRACE_SET: TokenSet = TokenSet::create(&[IN_KEYWORD, LBRACE]);
const IN_KEYWORD_L_BRACE_RECOVERY_SET: TokenSet =
    TokenSet::or_set(&[IN_KEYWORD_L_BRACE_SET, PARAMETER_NAME_RECOVERY_SET]);
const COLON_IN_KEYWORD_SET: TokenSet = TokenSet::create(&[COLON, IN_KEYWORD]);
const L_PAR_L_BRACE_R_PAR_SET: TokenSet = TokenSet::create(&[LPAR, LBRACE, RPAR]);
const IN_KEYWORD_SET: TokenSet = TokenSet::create(&[IN_KEYWORD]);
const TRY_CATCH_RECOVERY_TOKEN_SET: TokenSet = TokenSet::create(&[LBRACE, RBRACE, FINALLY_KEYWORD, CATCH_KEYWORD]);

/// `KEYWORD_TEXTS = tokenSetToMap(KEYWORDS)`: a hard keyword by its `toString()`.
fn keyword_texts(text: &[u8]) -> Option<TokenKind> {
    let keyword = TokenKind::keyword_by_debug_name(text)?;
    KEYWORDS.contains(keyword).then_some(keyword)
}

const TYPE_ARGUMENT_LIST_STOPPERS: TokenSet = TokenSet::create(&[
    INTEGER_LITERAL,
    FLOAT_LITERAL,
    CHARACTER_LITERAL,
    INTERPOLATION_PREFIX,
    OPEN_QUOTE,
    PACKAGE_KEYWORD,
    AS_KEYWORD,
    TYPE_ALIAS_KEYWORD,
    INTERFACE_KEYWORD,
    CLASS_KEYWORD,
    THIS_KEYWORD,
    VAL_KEYWORD,
    VAR_KEYWORD,
    FUN_KEYWORD,
    FOR_KEYWORD,
    NULL_KEYWORD,
    TRUE_KEYWORD,
    FALSE_KEYWORD,
    IS_KEYWORD,
    THROW_KEYWORD,
    RETURN_KEYWORD,
    BREAK_KEYWORD,
    CONTINUE_KEYWORD,
    OBJECT_KEYWORD,
    IF_KEYWORD,
    TRY_KEYWORD,
    ELSE_KEYWORD,
    WHILE_KEYWORD,
    DO_KEYWORD,
    WHEN_KEYWORD,
    RBRACKET,
    RBRACE,
    RPAR,
    PLUSPLUS,
    MINUSMINUS,
    EXCLEXCL,
    //            MUL,
    PLUS,
    MINUS,
    EXCL,
    DIV,
    PERC,
    LTEQ,
    // TODO GTEQ,   foo<bar, baz>=x
    EQEQEQ,
    EXCLEQEQEQ,
    EQEQ,
    EXCLEQ,
    ANDAND,
    OROR,
    SAFE_ACCESS,
    ELVIS,
    SEMICOLON,
    RANGE,
    RANGE_UNTIL,
    EQ,
    MULTEQ,
    DIVEQ,
    PERCEQ,
    PLUSEQ,
    MINUSEQ,
    NOT_IN,
    NOT_IS,
    COLONCOLON,
    COLON,
]);

/// `KotlinExpressionParsing.STATEMENT_FIRST`.
pub const STATEMENT_FIRST: TokenSet = TokenSet::or_set(&[
    EXPRESSION_FIRST,
    TokenSet::create(&[
        // declaration
        FUN_KEYWORD,
        VAL_KEYWORD,
        VAR_KEYWORD,
        INTERFACE_KEYWORD,
        CLASS_KEYWORD,
        TYPE_ALIAS_KEYWORD,
    ]),
    MODIFIER_KEYWORDS,
]);

const STATEMENT_NEW_LINE_QUICK_RECOVERY_SET: TokenSet = TokenSet::or_set(&[
    TokenSet::and_set(STATEMENT_FIRST, TokenSet::and_not(KEYWORDS, TokenSet::create(&[IN_KEYWORD]))),
    TokenSet::create(&[EOL_OR_SEMICOLON]),
]);

const MIN_BINARY_OPERATION_PRECEDENCE: BinaryOperationPrecedence = BinaryOperationPrecedence::ENTRIES[0];
const MAX_BINARY_OPERATION_PRECEDENCE: BinaryOperationPrecedence = BinaryOperationPrecedence::ENTRIES[11];

const ALLOW_NEWLINE_OPERATIONS: TokenSet = TokenSet::create(&[
    DOT,
    SAFE_ACCESS,
    COLON,
    AS_KEYWORD,
    AS_SAFE,
    ELVIS,
    // Can't allow `is` and `!is` because of when entry conditions: IS_KEYWORD, NOT_IS,
    ANDAND,
    OROR,
]);

/// `TokenSet.orSet(EXPRESSION_FOLLOW, TokenSet.create(LONG_TEMPLATE_ENTRY_END))` of `parseAtomicExpression`.
const EXPRESSION_FOLLOW_OR_LONG_TEMPLATE_ENTRY_END: TokenSet =
    TokenSet::or_set(&[EXPRESSION_FOLLOW, TokenSet::create(&[LONG_TEMPLATE_ENTRY_END])]);
/// `TokenSet.create(ARROW, LBRACE, RBRACE, EOL_OR_SEMICOLON)` of `parseWhenEntry`.
const ARROW_LBRACE_RBRACE_EOL_OR_SEMICOLON_SET: TokenSet = TokenSet::create(&[ARROW, LBRACE, RBRACE, EOL_OR_SEMICOLON]);
/// `TokenSet.create(LBRACE, RBRACE, ARROW)` of `parseWhenEntryGuardOrSuggest`.
const LBRACE_RBRACE_ARROW_SET: TokenSet = TokenSet::create(&[LBRACE, RBRACE, ARROW]);
/// `TokenSet.create(EOL_OR_SEMICOLON, LBRACE, RBRACE)` of `parseStatements`.
const EOL_OR_SEMICOLON_LBRACE_RBRACE_SET: TokenSet = TokenSet::create(&[EOL_OR_SEMICOLON, LBRACE, RBRACE]);

/// `KotlinExpressionParsing`.
pub struct KotlinExpressionParsing<'p, 'b> {
    /// `myKotlinParsing`, which also holds `myBuilder` and `isLazy`.
    pub(crate) my_kotlin_parsing: &'p mut KotlinParsing<'b>,
}

impl<'b> AbstractKotlinParsing<'b> for KotlinExpressionParsing<'_, 'b> {
    fn my_builder(&mut self) -> &mut SemanticWhitespaceAwarePsiBuilder<'b> {
        &mut self.my_kotlin_parsing.my_builder
    }

    fn is_lazy(&self) -> bool {
        self.my_kotlin_parsing.is_lazy
    }

    fn create_truncated_builder(&mut self, eof_position: i32) -> KotlinParsing<'_> {
        let is_by_clause = !self.my_kotlin_parsing.is_top_level;
        let is_lazy = self.my_kotlin_parsing.is_lazy;
        let builder = self.my_kotlin_parsing.my_builder.reborrow().wrapped(Adapter::Truncated { eof_position });
        Self::create(is_by_clause, is_lazy, builder)
    }
}

impl<'p, 'b> KotlinExpressionParsing<'p, 'b> {
    /// `new KotlinExpressionParsing(builder, kotlinParsing, isLazy)`: the builder and `isLazy` are `kotlinParsing`'s.
    pub fn new(kotlin_parsing: &'p mut KotlinParsing<'b>) -> Self {
        KotlinExpressionParsing { my_kotlin_parsing: kotlin_parsing }
    }

    /// `create(builder)` (`@Override`): `myKotlinParsing.create(builder)`, which is `createForTopLevel`; the by-clause
    /// subclass's override is `createForByClause(builder, isLazy)`.
    fn create(is_by_clause: bool, is_lazy: bool, builder: SemanticWhitespaceAwarePsiBuilder<'_>) -> KotlinParsing<'_> {
        if is_by_clause {
            KotlinParsing::create_for_by_clause(builder, is_lazy)
        } else {
            KotlinParsing::create(builder)
        }
    }

    /// `parseExpression()`.
    pub fn parse_expression(&mut self) {
        self.parse_expression_with("Expecting an expression");
    }

    /// `parseExpression(messageIfNotExpressionFirst)`.
    pub(crate) fn parse_expression_with(&mut self, message_if_not_expression_first: &'static str) {
        if !self.at_set(&EXPRESSION_FIRST) {
            self.error(message_if_not_expression_first);
            return;
        }

        self.parse_binary_expression(MAX_BINARY_OPERATION_PRECEDENCE);
    }

    /// `parseBinaryExpression(maxPrecedence)`: `element (operation element)*`; returns the `minPrecedence` to consider
    /// next. It's actual for `IS` operation because it's handled in a special way since it doesn't use recursive
    /// parsing on RHS. (Java's `maxPrecedence == null` check is the `Option` of `get_higher_priority`, unwrapped by the
    /// caller: `AS` has no operation above it that recurses.)
    pub(crate) fn parse_binary_expression(
        &mut self,
        max_precedence: BinaryOperationPrecedence,
    ) -> BinaryOperationPrecedence {
        let mut expression = self.mark();

        self.parse_prefix_expression();

        let mut min_precedence = MIN_BINARY_OPERATION_PRECEDENCE;

        while !self.interrupted_with_new_line() {
            let token = self.tt();

            // `token instanceof KtToken ? (KtToken) token : null`
            let Some(operation) = token.filter(|t| t.is_kt_token()) else { break };

            /*
                Match all allowed operations with the current or higher priority (lower precedence) at once.
                For instance, if the precedence is COMPARISON, we can match ADDITIVE, MULTIPLICATIVE but cannot match CONJUNCTION.
                The lowest priority is ASSIGNMENT.

                If the map doesn't contain the given token, the further parsing is not performed. Later that token either will be handled
                by an outer 'parseBinaryExpression()' call with a lower binary priority (higher precedence) or will not be handled at all
                if the expression is not binary.

                All Kotlin binary operations are syntactically left-associative.
                That's why we always use higher priority on recursive `parseBinaryExpression` call.

                If a right-associative operation is added, the same priority instead of the highest should be passed to the recursive call.
            */
            let next_precedence = BinaryOperationPrecedence::token_to_binary_precedence_with_soft_identifiers(token);
            let Some(next_precedence) = next_precedence else { break };
            if next_precedence.ordinal() > max_precedence.ordinal()
                || next_precedence.ordinal() < min_precedence.ordinal()
            {
                break;
            }

            if next_precedence == BinaryOperationPrecedence::INFIX && SOFT_KEYWORDS.contains(operation) {
                // Remap soft keywords and modifiers that are treated as infix functions since there is no mutating `atSetWithRemap` call
                self.my_builder().remap_current_token(IDENTIFIER);
            }

            self.parse_operation_reference();

            let result_type = match operation {
                AS_KEYWORD | AS_SAFE => {
                    self.my_kotlin_parsing.parse_type_ref_without_intersections();
                    min_precedence = BinaryOperationPrecedence::AS;
                    BINARY_WITH_TYPE
                }
                IS_KEYWORD | NOT_IS => {
                    self.my_kotlin_parsing.parse_type_ref_without_intersections();
                    // The handling of `is`, it doesn't parse RHS recursively and greedily.
                    // To prevent parsing of more prioritized operations next to `is` (for instance, INFIX, RANGE, and others),
                    // use the `minPrecedence` in addition to `maxPrecedence`.
                    min_precedence = BinaryOperationPrecedence::IN_OR_IS;
                    IS_EXPRESSION
                }
                _ => {
                    // Parse operations with higher priorities greedily.
                    // `getHigherPriority()` is null only for AS, whose operations are handled above ("Shouldn't be here").
                    min_precedence = match next_precedence.get_higher_priority() {
                        Some(higher_priority) => self.parse_binary_expression(higher_priority),
                        None => MIN_BINARY_OPERATION_PRECEDENCE,
                    };
                    BINARY_EXPRESSION
                }
            };

            expression.done(self.my_builder(), result_type);
            expression = expression.precede(self.my_builder());
        }

        expression.drop(self.my_builder());

        min_precedence
    }

    /// `parseLabeledExpression()`: `label prefixExpression`.
    pub(crate) fn parse_labeled_expression(&mut self) {
        let expression = self.mark();
        self.parse_label_definition();
        self.parse_prefix_expression();
        expression.done(self.my_builder(), LABELED_EXPRESSION);
    }

    /// `parsePrefixExpression()`: `operation? prefixExpression`.
    pub(crate) fn parse_prefix_expression(&mut self) {
        if self.at(AT) {
            if !self.parse_local_declaration(/* rollbackIfDefinitelyNotExpression = */ false, false) {
                let expression = self.mark();
                self.my_kotlin_parsing.parse_annotations(AnnotationParsingMode::DEFAULT);
                self.parse_prefix_expression();
                expression.done(self.my_builder(), ANNOTATED_EXPRESSION);
            }
        } else {
            self.my_builder().disable_joining_complex_tokens();
            if self.is_at_label_definition_or_missing_identifier() {
                self.my_builder().restore_joining_complex_tokens_state();
                self.parse_labeled_expression();
            } else if self.at_set(&PREFIX_OPERATIONS) {
                let expression = self.mark();

                self.parse_operation_reference();

                self.my_builder().restore_joining_complex_tokens_state();

                self.parse_prefix_expression();
                expression.done(self.my_builder(), PREFIX_EXPRESSION);
            } else {
                self.my_builder().restore_joining_complex_tokens_state();
                self.parse_postfix_expression();
            }
        }
    }

    /// `parseDoubleColonSuffix(expression)`: `"::" SimpleName typeArguments?`.
    pub(crate) fn parse_double_colon_suffix(&mut self, expression: Marker) -> bool {
        if !self.at(COLONCOLON) {
            return false;
        }

        self.advance(); // COLONCOLON

        if self.at(CLASS_KEYWORD) {
            self.advance(); // CLASS_KEYWORD

            expression.done(self.my_builder(), CLASS_LITERAL_EXPRESSION);
            return true;
        }

        self.parse_simple_name_expression();

        if self.at(LT) {
            let type_argument_list = self.mark();
            if self.my_kotlin_parsing.try_parse_type_argument_list(&TYPE_ARGUMENT_LIST_STOPPERS) {
                type_argument_list.error(self.my_builder(), "Type arguments are not allowed");
            } else {
                type_argument_list.rollback_to(self.my_builder());
            }
        }

        if self.at(LPAR) && !self.my_builder().newline_before_current_token() {
            let lpar = self.mark();
            self.parse_call_suffix();
            lpar.error(
                self.my_builder(),
                "This syntax is reserved for future use; to call a reference, enclose it in parentheses: (foo::bar)(args)",
            );
        }

        expression.done(self.my_builder(), CALLABLE_REFERENCE_EXPRESSION);
        true
    }

    /// `skipQuestionMarksBeforeDoubleColon()`.
    pub(crate) fn skip_question_marks_before_double_colon(&mut self) {
        if self.at(QUEST) {
            let mut k = 1;
            while self.lookahead(k) == QUEST {
                k += 1;
            }
            if self.lookahead(k) == COLONCOLON {
                while k > 0 {
                    self.advance(); // QUEST
                    k -= 1;
                }
            }
        }
    }

    /// `parsePostfixExpression()`: `atomicExpression postfixUnaryOperation*`.
    ///
    /// ```text
    /// postfixUnaryOperation
    ///   : "++" : "--" : "!!"
    ///   : typeArguments? valueArguments (getEntryPoint? functionLiteral)
    ///   : typeArguments (getEntryPoint? functionLiteral)
    ///   : arrayAccess
    ///   : memberAccessOperation postfixUnaryExpression // TODO: Review
    ///   ;
    /// ```
    pub(crate) fn parse_postfix_expression(&mut self) {
        let mut expression = self.mark();

        let mut first_expression_parsed = if self.at(COLONCOLON) {
            let marker = self.mark();
            self.parse_double_colon_suffix(marker)
        } else {
            self.parse_atomic_expression()
        };

        loop {
            if self.interrupted_with_new_line() {
                break;
            } else if self.at(LBRACKET) {
                self.parse_array_access();
                expression.done(self.my_builder(), ARRAY_ACCESS_EXPRESSION);
            } else if self.parse_call_suffix() {
                expression.done(self.my_builder(), CALL_EXPRESSION);
            } else if self.at(DOT) || self.at(SAFE_ACCESS) {
                let expression_type = if self.at(DOT) { DOT_QUALIFIED_EXPRESSION } else { SAFE_ACCESS_EXPRESSION };
                self.advance(); // DOT or SAFE_ACCESS

                if !first_expression_parsed {
                    expression.drop(self.my_builder());
                    expression = self.mark();
                    first_expression_parsed = self.parse_atomic_expression();
                    continue;
                }

                self.parse_selector_call_expression();

                expression.done(self.my_builder(), expression_type);
            } else if self.at_set(&POSTFIX_OPERATIONS) {
                self.parse_operation_reference();
                expression.done(self.my_builder(), POSTFIX_EXPRESSION);
            } else {
                self.skip_question_marks_before_double_colon();
                if !self.parse_double_colon_suffix(expression) {
                    break;
                }
            }
            expression = expression.precede(self.my_builder());
        }
        expression.drop(self.my_builder());
    }

    /// `parseCallSuffix()`: `typeArguments? valueArguments annotatedLambda | typeArguments annotatedLambda`.
    pub(crate) fn parse_call_suffix(&mut self) -> bool {
        if self.parse_call_with_closure() {
            // do nothing
        } else if self.at(LPAR) {
            self.parse_value_argument_list();
            self.parse_call_with_closure();
        } else if self.at(LT) {
            let type_argument_list = self.mark();
            if self.my_kotlin_parsing.try_parse_type_argument_list(&TYPE_ARGUMENT_LIST_STOPPERS) {
                type_argument_list.done(self.my_builder(), TYPE_ARGUMENT_LIST);
                if !self.my_builder().newline_before_current_token() && self.at(LPAR) {
                    self.parse_value_argument_list();
                }
                self.parse_call_with_closure();
            } else {
                type_argument_list.rollback_to(self.my_builder());
                return false;
            }
        } else {
            return false;
        }

        true
    }

    /// `parseSelectorCallExpression()`: `atomicExpression typeParameters? valueParameters? functionLiteral*`.
    pub(crate) fn parse_selector_call_expression(&mut self) {
        let mark = self.mark();
        self.parse_atomic_expression();
        if !self.my_builder().newline_before_current_token() && self.parse_call_suffix() {
            mark.done(self.my_builder(), CALL_EXPRESSION);
        } else {
            mark.drop(self.my_builder());
        }
    }

    /// `parseOperationReference()`.
    pub(crate) fn parse_operation_reference(&mut self) {
        let operation_reference = self.mark();
        self.advance(); // operation
        operation_reference.done(self.my_builder(), OPERATION_REFERENCE);
    }

    /// `parseCallWithClosure()`: `annotatedLambda*`. The by-clause subclass overrides it: no trailing lambdas while
    /// its builder's newline stack is at the by-clause's level (`getStackSize() > 0` falls through to this body).
    pub(crate) fn parse_call_with_closure(&mut self) -> bool {
        if !self.my_kotlin_parsing.is_top_level && self.my_builder().get_stack_size() <= 0 {
            return false;
        }
        let mut success = false;

        loop {
            let argument = self.mark();

            if !self.parse_annotated_lambda(/* preferBlock = */ false) {
                argument.drop(self.my_builder());
                break;
            }

            argument.done(self.my_builder(), LAMBDA_ARGUMENT);
            success = true;
        }

        success
    }

    /// `parseAnnotatedLambda(preferBlock)`: `("@" annotationEntry)* labelDefinition? functionLiteral`.
    pub(crate) fn parse_annotated_lambda(&mut self, prefer_block: bool) -> bool {
        let annotated = self.mark();

        let were_annotations = self.my_kotlin_parsing.parse_annotations(AnnotationParsingMode::DEFAULT);
        let labeled = self.mark();

        let was_label = self.is_at_label_definition_or_missing_identifier();
        if was_label {
            self.parse_label_definition();
        }

        if !self.at(LBRACE) {
            annotated.rollback_to(self.my_builder());
            return false;
        }

        self.parse_function_literal_with(prefer_block, /* collapse = */ true);

        self.done_or_drop(labeled, LABELED_EXPRESSION, was_label);
        self.done_or_drop(annotated, ANNOTATED_EXPRESSION, were_annotations);

        true
    }

    /// `doneOrDrop(marker, type, condition)`.
    pub(crate) fn done_or_drop(&mut self, marker: Marker, element_type: NodeKind, condition: bool) {
        if condition {
            marker.done(self.my_builder(), element_type);
        } else {
            marker.drop(self.my_builder());
        }
    }

    /// `isAtLabelDefinitionOrMissingIdentifier()`.
    pub(crate) fn is_at_label_definition_or_missing_identifier(&mut self) -> bool {
        (self.at(IDENTIFIER) && self.my_builder().raw_lookup(1) == AT) || self.at(AT)
    }

    /// `parseAtomicExpression()`.
    ///
    /// ```text
    /// atomicExpression
    ///   : "this" label?
    ///   : "super" ("<" type ">")? label?
    ///   : objectLiteral
    ///   : jump
    ///   : if
    ///   : when
    ///   : try
    ///   : loop
    ///   : literalConstant
    ///   : functionLiteral
    ///   : declaration
    ///   : SimpleName
    ///   : collectionLiteral
    ///   ;
    /// ```
    pub(crate) fn parse_atomic_expression(&mut self) -> bool {
        let mut ok = true;

        // `switch (getTokenId())`: a non-Kotlin token's id is `INVALID_Id`, the default branch.
        match self.tt().filter(|t| t.is_kt_token()) {
            Some(LPAR) => self.parse_parenthesized_expression(),
            Some(LBRACKET) => self.parse_collection_literal_expression(),
            Some(THIS_KEYWORD) => self.parse_this_expression(),
            Some(SUPER_KEYWORD) => self.parse_super_expression(),
            Some(OBJECT_KEYWORD) => self.parse_object_literal(),
            Some(THROW_KEYWORD) => self.parse_throw(),
            Some(RETURN_KEYWORD) => self.parse_return(),
            Some(CONTINUE_KEYWORD) => self.parse_jump(CONTINUE),
            Some(BREAK_KEYWORD) => self.parse_jump(BREAK),
            Some(IF_KEYWORD) => self.parse_if(),
            Some(WHEN_KEYWORD) => self.parse_when(),
            Some(TRY_KEYWORD) => self.parse_try(),
            Some(FOR_KEYWORD) => self.parse_for(),
            Some(WHILE_KEYWORD) => self.parse_while(),
            Some(DO_KEYWORD) => self.parse_do_while(),
            Some(IDENTIFIER) => 'identifier: {
                // Try to parse anonymous function with context parameters
                if self.at(CONTEXT_KEYWORD) && self.lookahead(1) == LPAR {
                    if self.parse_local_declaration(true, false) {
                        break 'identifier;
                    } else {
                        self.at(IDENTIFIER);
                    }
                }

                self.parse_simple_name_expression();
            }
            Some(LBRACE) => self.parse_function_literal(),
            Some(INTERPOLATION_PREFIX | OPEN_QUOTE) => self.parse_string_template(),
            /*
             * literalConstant
             *   : "true" | "false"
             *   : stringTemplate
             *   : NoEscapeString
             *   : IntegerLiteral
             *   : CharacterLiteral
             *   : FloatLiteral
             *   : "null"
             *   ;
             */
            Some(TRUE_KEYWORD | FALSE_KEYWORD) => self.parse_one_token_expression(BOOLEAN_CONSTANT),
            Some(INTEGER_LITERAL) => self.parse_one_token_expression(INTEGER_CONSTANT),
            Some(CHARACTER_LITERAL) => self.parse_one_token_expression(CHARACTER_CONSTANT),
            Some(FLOAT_LITERAL) => self.parse_one_token_expression(FLOAT_CONSTANT),
            Some(NULL_KEYWORD) => self.parse_one_token_expression(NULL),
            Some(CLASS_KEYWORD | INTERFACE_KEYWORD | FUN_KEYWORD | VAL_KEYWORD | VAR_KEYWORD | TYPE_ALIAS_KEYWORD) => {
                let rollback_if_definitely_not_expression = self.my_builder().newline_before_current_token();
                if !self.parse_local_declaration(rollback_if_definitely_not_expression, false) {
                    ok = false;
                }
                // declaration was parsed, do nothing
            }
            _ => ok = false,
        }

        if !ok {
            // TODO: better recovery if FIRST(element) did not match
            self.error_with_recovery("Expecting an element", Some(&EXPRESSION_FOLLOW_OR_LONG_TEMPLATE_ENTRY_END));
        }

        ok
    }

    /// `parseStringTemplate()`: `INTERPOLATION_PREFIX OPEN_QUOTE stringTemplateElement* CLOSING_QUOTE`.
    pub(crate) fn parse_string_template(&mut self) {
        debug_assert!(self._at(INTERPOLATION_PREFIX) || self._at(OPEN_QUOTE));

        let template = self.mark();

        if self.at(INTERPOLATION_PREFIX) {
            let mark = self.mark();
            self.advance(); // INTERPOLATION_PREFIX
            mark.done(self.my_builder(), STRING_INTERPOLATION_PREFIX);
        }

        debug_assert!(self._at(OPEN_QUOTE));
        self.advance(); // OPEN_QUOTE

        while !self.eof() {
            if self.at(CLOSING_QUOTE) || self.at(DANGLING_NEWLINE) {
                break;
            }
            self.parse_string_template_element();
        }

        if self.at(DANGLING_NEWLINE) {
            self.error_and_advance("Expecting '\"'");
        } else {
            self.expect_with(CLOSING_QUOTE, "Expecting '\"'");
        }
        template.done(self.my_builder(), STRING_TEMPLATE);
    }

    /// `parseStringTemplateElement()`.
    ///
    /// ```text
    /// stringTemplateElement
    ///   : RegularStringPart
    ///   : ShortTemplateEntrySTART (SimpleName | "this")
    ///   : EscapeSequence
    ///   : longTemplate
    ///   ;
    ///
    /// longTemplate
    ///   : "${" expression "}"
    ///   ;
    /// ```
    pub(crate) fn parse_string_template_element(&mut self) {
        if self.at(REGULAR_STRING_PART) {
            let mark = self.mark();
            self.advance(); // REGULAR_STRING_PART
            mark.done(self.my_builder(), LITERAL_STRING_TEMPLATE_ENTRY);
        } else if self.at(ESCAPE_SEQUENCE) {
            let mark = self.mark();
            self.advance(); // ESCAPE_SEQUENCE
            mark.done(self.my_builder(), ESCAPE_STRING_TEMPLATE_ENTRY);
        } else if self.at(SHORT_TEMPLATE_ENTRY_START) {
            let entry = self.mark();
            self.advance(); // SHORT_TEMPLATE_ENTRY_START

            if self.at(THIS_KEYWORD) {
                let this_expression = self.mark();
                let reference = self.mark();
                self.advance(); // THIS_KEYWORD
                reference.done(self.my_builder(), REFERENCE_EXPRESSION);
                this_expression.done(self.my_builder(), THIS_EXPRESSION);
            } else {
                let keyword = self.my_builder().get_token_bytes().and_then(keyword_texts);
                if let Some(keyword) = keyword {
                    self.my_builder().remap_current_token(keyword);
                    self.error_and_advance("Keyword cannot be used as a reference");
                } else {
                    let reference = self.mark();
                    self.expect_with(IDENTIFIER, "Expecting a name");
                    reference.done(self.my_builder(), REFERENCE_EXPRESSION);
                }
            }

            entry.done(self.my_builder(), SHORT_STRING_TEMPLATE_ENTRY);
        } else if self.at(LONG_TEMPLATE_ENTRY_START) {
            let long_template_entry = self.mark();

            self.advance(); // LONG_TEMPLATE_ENTRY_START

            while !self.eof() {
                let offset = self.my_builder().get_current_offset();

                self.parse_expression();

                if self._at(LONG_TEMPLATE_ENTRY_END) {
                    self.advance();
                    break;
                } else {
                    self.error("Expecting '}'");
                    if offset == self.my_builder().get_current_offset() {
                        // Prevent hang if can't advance with parseExpression()
                        self.advance();
                    }
                }
            }

            long_template_entry.done(self.my_builder(), LONG_STRING_TEMPLATE_ENTRY);
        } else {
            self.error_and_advance("Unexpected token in a string template");
        }
    }

    /// `parseWhen()`.
    ///
    /// ```text
    /// when
    ///   : "when" ("(" (modifiers "val" SimpleName "=")? element ")")? "{"
    ///         whenEntry*
    ///     "}"
    ///   ;
    /// ```
    pub(crate) fn parse_when(&mut self) {
        debug_assert!(self._at(WHEN_KEYWORD));

        let when = self.mark();

        self.advance(); // WHEN_KEYWORD

        // Parse condition
        self.my_builder().disable_newlines();
        if self.at(LPAR) {
            self.advance_at(LPAR);

            let at_when_start = self.mark();
            self.my_kotlin_parsing.parse_annotations_list(&EQ_RPAR_SET);
            if self.at(VAL_KEYWORD) || self.at(VAR_KEYWORD) {
                let decl_type = self.my_kotlin_parsing.parse_property(DeclarationParsingMode::LOCAL);

                // `parseProperty` never returns null in the Java: PROPERTY or DESTRUCTURING_DECLARATION.
                at_when_start.done(self.my_builder(), decl_type.unwrap_or(PROPERTY));
                at_when_start.set_custom_edge_token_binders(
                    self.my_builder(),
                    Some(Binder::PrecedingDocComments),
                    Some(Binder::TrailingComments),
                );
            } else {
                at_when_start.rollback_to(self.my_builder());
                self.parse_expression();
            }

            self.expect_with(RPAR, "Expecting ')'");
        }
        self.my_builder().restore_newlines_state();

        // Parse when block
        self.my_builder().enable_newlines();
        if self.expect_with(LBRACE, "Expecting '{'") {
            while !self.eof() && !self.at(RBRACE) {
                self.parse_when_entry();
            }

            self.expect_with(RBRACE, "Expecting '}'");
        }
        self.my_builder().restore_newlines_state();

        when.done(self.my_builder(), WHEN);
    }

    /// `parseWhenEntry()`.
    ///
    /// ```text
    /// whenEntry
    ///   // TODO : consider empty after ->
    ///   : whenCondition{","} whenEntryGuard? "->" element SEMI
    ///   : "else" whenEntryGuard? "->" element SEMI
    ///   ;
    /// ```
    pub(crate) fn parse_when_entry(&mut self) {
        let entry = self.mark();

        if self.at(ELSE_KEYWORD) {
            self.advance(); // ELSE_KEYWORD

            self.parse_when_entry_guard_or_suggest();

            if !self.at(ARROW) {
                self.error_until("Expecting '->'", &ARROW_LBRACE_RBRACE_EOL_OR_SEMICOLON_SET);
            }

            if self.at(ARROW) {
                self.advance(); // ARROW

                if self.at_set(&WHEN_CONDITION_RECOVERY_SET) {
                    self.error("Expecting an element");
                } else {
                    self.parse_control_structure_body();
                }
            } else if self.at(LBRACE) {
                // no arrow, probably it's simply missing
                self.parse_control_structure_body();
            } else if !self.at_set(&WHEN_CONDITION_RECOVERY_SET) {
                self.error_and_advance("Expecting '->'");
            }
        } else {
            self.parse_when_entry_not_else();
        }

        entry.done(self.my_builder(), WHEN_ENTRY);
        self.consume_if(SEMICOLON);
    }

    /// `parseWhenEntryNotElse()`: `whenCondition{","} whenEntryGuard? "->" element SEMI`.
    pub(crate) fn parse_when_entry_not_else(&mut self) {
        loop {
            while self.at(COMMA) {
                self.error_and_advance("Expecting a when-condition");
            }
            self.parse_when_condition();
            if !self.at(COMMA) {
                break;
            }
            self.advance(); // COMMA
            if self.at(ARROW) {
                break;
            }
        }

        self.parse_when_entry_guard_or_suggest();

        self.expect_with_recovery(ARROW, "Expecting '->'", Some(&WHEN_CONDITION_RECOVERY_SET));
        if self.at_set(&WHEN_CONDITION_RECOVERY_SET) {
            self.error("Expecting an element");
        } else {
            self.parse_control_structure_body();
        }
        // SEMI is consumed in parseWhenEntry
    }

    /// `parseWhenCondition()`.
    ///
    /// ```text
    /// whenCondition
    ///   : expression
    ///   : ("in" | "!in") expression
    ///   : ("is" | "!is") isRHS
    ///   ;
    /// ```
    pub(crate) fn parse_when_condition(&mut self) {
        let condition = self.mark();
        self.my_builder().disable_newlines();
        match self.tt().filter(|t| t.is_kt_token()) {
            Some(IN_KEYWORD | NOT_IN) => {
                let mark = self.mark();
                self.advance(); // IN_KEYWORD or NOT_IN
                mark.done(self.my_builder(), OPERATION_REFERENCE);

                if self.at_set(&WHEN_CONDITION_RECOVERY_SET_WITH_ARROW) {
                    self.error("Expecting an element");
                } else {
                    self.parse_expression();
                }
                condition.done(self.my_builder(), WHEN_CONDITION_IN_RANGE);
            }
            Some(IS_KEYWORD | NOT_IS) => {
                self.advance(); // IS_KEYWORD or NOT_IS

                if self.at_set(&WHEN_CONDITION_RECOVERY_SET_WITH_ARROW) {
                    self.error("Expecting a type");
                } else {
                    self.my_kotlin_parsing.parse_type_ref();
                }
                condition.done(self.my_builder(), WHEN_CONDITION_IS_PATTERN);
            }
            Some(RBRACE | ELSE_KEYWORD | ARROW | DOT) => {
                self.error("Expecting an expression, is-condition or in-condition");
                condition.done(self.my_builder(), WHEN_CONDITION_EXPRESSION);
            }
            _ => {
                self.parse_expression();
                condition.done(self.my_builder(), WHEN_CONDITION_EXPRESSION);
            }
        }
        self.my_builder().restore_newlines_state();
    }

    /// `parseWhenEntryGuardOrSuggest()`.
    pub(crate) fn parse_when_entry_guard_or_suggest(&mut self) {
        if self.at(ANDAND) {
            self.error_until(
                "Unexpected '&&', use 'if' to introduce additional conditions; see https://kotl.in/guards-in-when",
                &LBRACE_RBRACE_ARROW_SET,
            );
        } else if self.at(IF_KEYWORD) {
            self.parse_when_entry_guard();
        }
    }

    /// `parseWhenEntryGuard()`: `"if" expression`.
    pub(crate) fn parse_when_entry_guard(&mut self) {
        debug_assert!(self._at(IF_KEYWORD));

        let guard = self.mark();
        self.advance(); // IF_KEYWORD
        self.parse_expression();
        guard.done(self.my_builder(), WHEN_ENTRY_GUARD);
    }

    /// `parseArrayAccess()`: `"[" element{","} "]"`.
    pub(crate) fn parse_array_access(&mut self) {
        self.parse_as_collection_literal_expression(INDICES, false, "Expecting an index element");
    }

    /// `parseCollectionLiteralExpression()`: `"[" element{","}? "]"`.
    pub(crate) fn parse_collection_literal_expression(&mut self) {
        self.parse_as_collection_literal_expression(COLLECTION_LITERAL_EXPRESSION, true, "Expecting an element");
    }

    /// `parseAsCollectionLiteralExpression(nodeType, canBeEmpty, missingElementErrorMessage)`.
    pub(crate) fn parse_as_collection_literal_expression(
        &mut self,
        node_type: NodeKind,
        can_be_empty: bool,
        missing_element_error_message: &'static str,
    ) {
        debug_assert!(self._at(LBRACKET));

        let inner_expressions = self.mark();

        self.my_builder().disable_newlines();
        self.advance(); // LBRACKET

        if !can_be_empty && self.at(RBRACKET) {
            self.error(missing_element_error_message);
        } else {
            self.parse_inner_expressions(missing_element_error_message);
        }

        self.expect_with(RBRACKET, "Expecting ']'");
        self.my_builder().restore_newlines_state();

        inner_expressions.done(self.my_builder(), node_type);
    }

    /// `parseInnerExpressions(missingElementErrorMessage)`.
    pub(crate) fn parse_inner_expressions(&mut self, missing_element_error_message: &'static str) {
        loop {
            if self.at(RBRACKET) {
                break;
            }
            self.parse_expression_with(missing_element_error_message);

            if !self.at(COMMA) {
                break;
            }
            self.advance(); // COMMA
        }
    }

    /// `parseContractDescriptionBlock()`.
    pub fn parse_contract_description_block(&mut self) {
        debug_assert!(self._at(CONTRACT_KEYWORD));

        self.advance(); // CONTRACT_KEYWORD

        self.parse_contract_effect_list();
    }

    /// `parseContractEffectList()`.
    pub(crate) fn parse_contract_effect_list(&mut self) {
        let block = self.mark();

        self.expect_with(LBRACKET, "Expecting '['");
        self.my_builder().enable_newlines();

        self.parse_contract_effects();

        self.expect_with(RBRACKET, "Expecting ']'");
        self.my_builder().restore_newlines_state();

        block.done(self.my_builder(), CONTRACT_EFFECT_LIST);
    }

    /// `parseContractEffects()`.
    pub(crate) fn parse_contract_effects(&mut self) {
        loop {
            if self.at(COMMA) {
                self.error_and_advance("Expecting a contract effect");
            }
            if self.at(RBRACKET) {
                break;
            }
            let effect = self.mark();
            self.parse_expression();
            effect.done(self.my_builder(), CONTRACT_EFFECT);

            if !self.at(COMMA) {
                break;
            }
            self.advance(); // COMMA
        }
    }

    /// `parseSimpleNameExpression()`: `SimpleName`.
    pub fn parse_simple_name_expression(&mut self) {
        let simple_name = self.mark();
        self.expect_with(IDENTIFIER, "Expecting an identifier");
        simple_name.done(self.my_builder(), REFERENCE_EXPRESSION);
    }

    /// `parseLocalDeclaration(rollbackIfDefinitelyNotExpression, isScriptTopLevel)`: `modifiers declarationRest`.
    pub(crate) fn parse_local_declaration(
        &mut self,
        rollback_if_definitely_not_expression: bool,
        is_script_top_level: bool,
    ) -> bool {
        let decl = self.mark();
        let mut detector = ModifierDetector::new();
        self.my_kotlin_parsing.parse_modifier_list_with_consumer(
            Some(&mut detector),
            &TokenSet::EMPTY,
            /* localDeclaration = */ true,
        );

        let decl_type =
            self.parse_local_declaration_rest(&detector, rollback_if_definitely_not_expression, is_script_top_level);

        if let Some(decl_type) = decl_type {
            // we do not attach preceding comments (non-doc) to local variables because they are likely commenting a few statements below
            self.close_declaration_with_comment_binders(
                decl,
                decl_type,
                decl_type != PROPERTY && decl_type != DESTRUCTURING_DECLARATION,
            );
            true
        } else {
            decl.rollback_to(self.my_builder());
            false
        }
    }

    /// `parseFunctionLiteral()`.
    ///
    /// ```text
    /// functionLiteral  // one can use "it" as a parameter name
    ///   : "{" expressions "}"
    ///   : "{" (modifiers SimpleName (":" type)?){","} "->" statements "}"
    ///   ;
    /// ```
    pub(crate) fn parse_function_literal(&mut self) {
        self.parse_function_literal_with(/* preferBlock = */ false, /* collapse = */ true);
    }

    /// `parseFunctionLiteral(preferBlock, collapse)`: if it has no `->`, it's a block, otherwise a function literal.
    pub fn parse_function_literal_with(&mut self, prefer_block: bool, collapse: bool) {
        debug_assert!(self._at(LBRACE));

        let literal_expression = self.mark();

        let literal = self.mark();

        self.my_builder().enable_newlines();
        self.advance(); // LBRACE

        let mut params_found = false;

        let token = self.tt();
        if token == ARROW {
            //   { -> ...}
            let parameter_list = self.mark();
            parameter_list.done(self.my_builder(), VALUE_PARAMETER_LIST);
            self.advance(); // ARROW
            params_found = true;
        } else if token == IDENTIFIER || token == COLON || token == LPAR || token == LBRACKET {
            // Try to parse a simple name list followed by an ARROW
            //   {a -> ...}
            //   {a, b -> ...}
            //   {(a, b) -> ... }
            let rollback_marker = self.mark();
            let next_token = self.lookahead(1);
            let prefer_params_to_expressions = next_token == COMMA || next_token == COLON;
            self.parse_function_literal_parameter_list();

            params_found = if prefer_params_to_expressions {
                self.rollback_or_drop(rollback_marker, ARROW, "An -> is expected", RBRACE)
            } else {
                self.rollback_or_drop_at(rollback_marker, ARROW)
            };
        }

        if !params_found && prefer_block {
            literal.drop(self.my_builder());
            self.parse_statements();
            self.expect_with(RBRACE, "Expecting '}'");
            literal_expression.done(self.my_builder(), BLOCK);
            self.my_builder().restore_newlines_state();

            return;
        }

        if collapse && self.is_lazy() {
            self.my_kotlin_parsing.advance_balanced_block();
            literal.done(self.my_builder(), FUNCTION_LITERAL);
            literal_expression.collapse(self.my_builder(), LAMBDA_EXPRESSION);
        } else {
            let body = self.mark();
            self.parse_statements();

            body.done(self.my_builder(), BLOCK);
            body.set_custom_edge_token_binders(
                self.my_builder(),
                Some(PRECEDING_ALL_COMMENTS_BINDER),
                Some(TRAILING_ALL_COMMENTS_BINDER),
            );

            self.expect_with(RBRACE, "Expecting '}'");
            literal.done(self.my_builder(), FUNCTION_LITERAL);
            literal_expression.done(self.my_builder(), LAMBDA_EXPRESSION);
        }

        self.my_builder().restore_newlines_state();
    }

    /// `rollbackOrDropAt(rollbackMarker, dropAt)`.
    pub(crate) fn rollback_or_drop_at(&mut self, rollback_marker: Marker, drop_at: TokenKind) -> bool {
        if self.at(drop_at) {
            self.advance(); // dropAt
            rollback_marker.drop(self.my_builder());
            return true;
        }
        rollback_marker.rollback_to(self.my_builder());
        false
    }

    /// `rollbackOrDrop(rollbackMarker, expected, expectMessage, validForDrop)`.
    pub(crate) fn rollback_or_drop(
        &mut self,
        rollback_marker: Marker,
        expected: TokenKind,
        expect_message: &'static str,
        valid_for_drop: TokenKind,
    ) -> bool {
        if self.at(expected) {
            self.advance(); // dropAt
            rollback_marker.drop(self.my_builder());
            return true;
        } else if self.at(valid_for_drop) {
            rollback_marker.drop(self.my_builder());
            self.expect_with(expected, expect_message);
            return true;
        }

        rollback_marker.rollback_to(self.my_builder());
        false
    }

    /// `parseFunctionLiteralParameterList()`: `lambdaParameter{","}`.
    ///
    /// ```text
    /// lambdaParameter
    ///   : variableDeclarationEntry
    ///   : multipleVariableDeclarations (":" type)?
    /// ```
    pub(crate) fn parse_function_literal_parameter_list(&mut self) {
        let parameter_list = self.mark();

        while !self.eof() {
            if self.at(ARROW) {
                break;
            }
            let parameter = self.mark();

            if self.at(COLON) {
                self.error("Expecting parameter name");
            } else if self.at(LPAR) || self.at(LBRACKET) {
                let destructuring_declaration = self.mark();
                // No var in lambda parameter destructuring
                let mode = if self.lookahead(1) == VAL_KEYWORD {
                    MultiDeclarationMode::FULL_VAL_ONLY
                } else {
                    MultiDeclarationMode::SHORT
                };
                self.my_kotlin_parsing.parse_multi_declaration_entry(
                    &TOKEN_SET_TO_FOLLOW_AFTER_DESTRUCTURING_DECLARATION_IN_LAMBDA,
                    &TOKEN_SET_TO_FOLLOW_AFTER_DESTRUCTURING_DECLARATION_IN_LAMBDA_RECOVERY,
                    mode,
                );
                destructuring_declaration.done(self.my_builder(), DESTRUCTURING_DECLARATION);
            } else {
                self.expect_with_recovery(IDENTIFIER, "Expecting parameter name", Some(&ARROW_SET));
            }

            if self.at(COLON) {
                self.advance(); // COLON
                self.my_kotlin_parsing.parse_type_ref_with(&ARROW_COMMA_SET);
            }
            parameter.done(self.my_builder(), VALUE_PARAMETER);

            if self.at(ARROW) {
                break;
            } else if self.at(COMMA) {
                self.advance(); // COMMA
            } else {
                self.error("Expecting '->' or ','");
                break;
            }
        }

        parameter_list.done(self.my_builder(), VALUE_PARAMETER_LIST);
    }

    /// `parseStatements()`: `SEMI* statement{SEMI+} SEMI*`.
    pub fn parse_statements(&mut self) {
        self.parse_statements_with(false);
    }

    /// `parseStatements(isScriptTopLevel)`: `SEMI* statement{SEMI+} SEMI*`.
    pub fn parse_statements_with(&mut self, is_script_top_level: bool) {
        while self.at(SEMICOLON) {
            self.advance(); // SEMICOLON
        }
        while !self.eof() && !self.at(RBRACE) {
            if !self.at_set(&STATEMENT_FIRST) {
                self.error_and_advance("Expecting an element");
            }
            if self.at_set(&STATEMENT_FIRST) {
                self.parse_statement(is_script_top_level);
            }
            if self.at(SEMICOLON) {
                while self.at(SEMICOLON) {
                    self.advance(); // SEMICOLON
                }
            } else if self.at(RBRACE) {
                break;
            } else if !is_script_top_level && !self.my_builder().newline_before_current_token() {
                let several_statements_error = "Unexpected tokens (use ';' to separate expressions on the same line)";

                if self.at_set(&STATEMENT_NEW_LINE_QUICK_RECOVERY_SET) {
                    self.error(several_statements_error);
                } else {
                    self.error_until(several_statements_error, &EOL_OR_SEMICOLON_LBRACE_RBRACE_SET);
                }
            }
        }
    }

    /// `parseStatement(isScriptTopLevel)`: `declaration | blockLevelExpression`.
    pub(crate) fn parse_statement(&mut self, is_script_top_level: bool) {
        if !self.parse_local_declaration(
            /* rollbackIfDefinitelyNotExpression = */ false,
            /* isScriptTopLevel = */ is_script_top_level,
        ) {
            if !self.at_set(&EXPRESSION_FIRST) {
                self.error_and_advance("Expecting a statement");
            } else if is_script_top_level {
                let script_initializer = self.mark();
                self.parse_block_level_expression();
                script_initializer.done(self.my_builder(), SCRIPT_INITIALIZER);
            } else {
                self.parse_block_level_expression();
            }
        }
    }

    /// `parseBlockLevelExpression()`: `annotations + ("\n")+ expression`.
    pub(crate) fn parse_block_level_expression(&mut self) {
        if self.at(AT) {
            let expression = self.mark();
            self.my_kotlin_parsing.parse_annotations(AnnotationParsingMode::DEFAULT);

            if !self.my_builder().newline_before_current_token() {
                expression.rollback_to(self.my_builder());
                self.parse_expression();
                return;
            }

            self.parse_block_level_expression();
            expression.done(self.my_builder(), ANNOTATED_EXPRESSION);
            return;
        }

        self.parse_expression();
    }

    /// `parseLocalDeclarationRest(modifierDetector, failIfDefinitelyNotExpression, isScriptTopLevel)`.
    ///
    /// ```text
    /// declaration
    ///   : function
    ///   : property
    ///   : extension
    ///   : class
    ///   : typeAlias
    ///   : object
    ///   ;
    /// ```
    pub(crate) fn parse_local_declaration_rest(
        &mut self,
        modifier_detector: &ModifierDetector,
        fail_if_definitely_not_expression: bool,
        is_script_top_level: bool,
    ) -> Option<NodeKind> {
        let keyword_token = self.tt();
        if fail_if_definitely_not_expression {
            if keyword_token != FUN_KEYWORD {
                return None;
            }

            return self.my_kotlin_parsing.parse_function_with(/* failIfIdentifierExists = */ true);
        }

        if keyword_token == OBJECT_KEYWORD {
            // Object expression may appear at the statement position: should parse it
            // as expression instead of object declaration
            // sample:
            // {
            //   object : Thread() {
            //   }
            // }
            let lookahead = self.lookahead(1);
            if lookahead == COLON || lookahead == LBRACE {
                return None;
            }
        }

        self.my_kotlin_parsing.parse_common_declaration(
            modifier_detector,
            NameParsingMode::REQUIRED,
            if is_script_top_level { DeclarationParsingMode::SCRIPT_TOPLEVEL } else { DeclarationParsingMode::LOCAL },
        )
    }

    /// `parseDoWhile()`: `"do" element "while" "(" element ")"`.
    pub(crate) fn parse_do_while(&mut self) {
        debug_assert!(self._at(DO_KEYWORD));

        let loop_marker = self.mark();

        self.advance(); // DO_KEYWORD

        if !self.at(WHILE_KEYWORD) {
            self.parse_loop_body();
        }

        if self.expect_with(WHILE_KEYWORD, "Expecting 'while' followed by a post-condition") {
            self.parse_condition();
        }

        loop_marker.done(self.my_builder(), DO_WHILE);
    }

    /// `parseWhile()`: `"while" "(" element ")" element`.
    pub(crate) fn parse_while(&mut self) {
        debug_assert!(self._at(WHILE_KEYWORD));

        let loop_marker = self.mark();

        self.advance(); // WHILE_KEYWORD

        self.parse_condition();

        self.parse_loop_body();

        loop_marker.done(self.my_builder(), WHILE);
    }

    /// `parseFor()`.
    ///
    /// ```text
    /// for
    ///   : "for" "(" annotations ("val" | "var")? (multipleVariableDeclarations | variableDeclarationEntry) "in" expression ")" expression
    ///   ;
    /// ```
    ///
    /// TODO: empty loop body (at the end of the block)?
    pub(crate) fn parse_for(&mut self) {
        debug_assert!(self._at(FOR_KEYWORD));

        let loop_marker = self.mark();

        self.advance(); // FOR_KEYWORD

        if self.expect_with_recovery(LPAR, "Expecting '(' to open a loop range", Some(&EXPRESSION_FIRST)) {
            self.my_builder().disable_newlines();

            if !self.at(RPAR) {
                let parameter = self.mark();

                if !self.at(IN_KEYWORD) {
                    self.my_kotlin_parsing.parse_modifier_list(&IN_KEYWORD_R_PAR_COLON_SET);
                }

                if self.at(VAL_KEYWORD) || self.at(VAR_KEYWORD) {
                    self.advance(); // VAL_KEYWORD or VAR_KEYWORD
                }

                if self.at(LPAR) || self.at(LBRACKET) {
                    let destructuring_declaration = self.mark();
                    // No var in destructured loop parameter
                    let mode = if self.lookahead(1) == VAL_KEYWORD {
                        MultiDeclarationMode::FULL_VAL_ONLY
                    } else {
                        MultiDeclarationMode::SHORT
                    };
                    self.my_kotlin_parsing.parse_multi_declaration_entry(
                        &IN_KEYWORD_L_BRACE_SET,
                        &IN_KEYWORD_L_BRACE_RECOVERY_SET,
                        mode,
                    );
                    destructuring_declaration.done(self.my_builder(), DESTRUCTURING_DECLARATION);
                } else {
                    self.expect_with_recovery(IDENTIFIER, "Expecting a variable name", Some(&COLON_IN_KEYWORD_SET));

                    if self.at(COLON) {
                        self.advance(); // COLON
                        self.my_kotlin_parsing.parse_type_ref_with(&IN_KEYWORD_SET);
                    }
                }
                parameter.done(self.my_builder(), VALUE_PARAMETER);

                if self.expect_with_recovery(IN_KEYWORD, "Expecting 'in'", Some(&L_PAR_L_BRACE_R_PAR_SET)) {
                    let range = self.mark();
                    self.parse_expression();
                    range.done(self.my_builder(), LOOP_RANGE);
                }
            } else {
                self.error("Expecting a variable name");
            }

            self.expect_no_advance(RPAR, "Expecting ')'");
            self.my_builder().restore_newlines_state();
        }

        self.parse_loop_body();

        loop_marker.done(self.my_builder(), FOR);
    }

    /// `parseControlStructureBody()`.
    pub(crate) fn parse_control_structure_body(&mut self) {
        if !self.parse_annotated_lambda(/* preferBlock = */ true) {
            self.parse_block_level_expression();
        }
    }

    /// `parseLoopBody()`: `element`.
    pub(crate) fn parse_loop_body(&mut self) {
        let body = self.mark();
        if !self.at(SEMICOLON) {
            self.parse_control_structure_body();
        }
        body.done(self.my_builder(), BODY);
    }

    /// `parseTry()`.
    ///
    /// ```text
    /// try
    ///   : "try" block catchBlock* finallyBlock?
    ///   ;
    /// catchBlock
    ///   : "catch" "(" annotations SimpleName ":" userType ")" block
    ///   ;
    ///
    /// finallyBlock
    ///   : "finally" block
    ///   ;
    /// ```
    pub(crate) fn parse_try(&mut self) {
        debug_assert!(self._at(TRY_KEYWORD));

        let try_expression = self.mark();

        self.advance(); // TRY_KEYWORD

        self.my_kotlin_parsing.parse_block();

        let mut catch_or_finally = false;
        while self.at(CATCH_KEYWORD) {
            catch_or_finally = true;
            let catch_block = self.mark();
            self.advance(); // CATCH_KEYWORD

            if self.at_set(&TRY_CATCH_RECOVERY_TOKEN_SET) {
                self.error("Expecting exception variable declaration");
            } else {
                let parameters = self.mark();
                self.expect_with_recovery(LPAR, "Expecting '('", Some(&TRY_CATCH_RECOVERY_TOKEN_SET));
                if !self.at_set(&TRY_CATCH_RECOVERY_TOKEN_SET) {
                    self.my_kotlin_parsing.parse_value_parameter(/*typeRequired = */ true);
                    if self.at(COMMA) {
                        self.advance(); // trailing comma
                    }
                    self.expect_with_recovery(RPAR, "Expecting ')'", Some(&TRY_CATCH_RECOVERY_TOKEN_SET));
                } else {
                    self.error("Expecting exception variable declaration");
                }
                parameters.done(self.my_builder(), VALUE_PARAMETER_LIST);
            }

            if self.at(LBRACE) {
                self.my_kotlin_parsing.parse_block();
            } else {
                self.error("Expecting a block: { ... }");
            }
            catch_block.done(self.my_builder(), CATCH);
        }

        if self.at(FINALLY_KEYWORD) {
            catch_or_finally = true;
            let finally_block = self.mark();

            self.advance(); // FINALLY_KEYWORD

            self.my_kotlin_parsing.parse_block();

            finally_block.done(self.my_builder(), FINALLY);
        }

        if !catch_or_finally {
            self.error("Expecting 'catch' or 'finally'");
        }

        try_expression.done(self.my_builder(), TRY);
    }

    /// `parseIf()`: `"if" "(" element ")" element SEMI? ("else" element)?`.
    pub(crate) fn parse_if(&mut self) {
        debug_assert!(self._at(IF_KEYWORD));

        let marker = self.mark();

        self.advance(); //IF_KEYWORD

        self.parse_condition();

        let then_branch = self.mark();
        if !self.at(ELSE_KEYWORD) && !self.at(SEMICOLON) {
            self.parse_control_structure_body();
        }
        if self.at(SEMICOLON) && self.lookahead(1) == ELSE_KEYWORD {
            self.advance(); // SEMICOLON
        }
        then_branch.done(self.my_builder(), THEN);

        // lookahead for arrow is needed to prevent capturing of whenEntry like "else -> "
        if self.at(ELSE_KEYWORD) && self.lookahead(1) != ARROW {
            self.advance(); // ELSE_KEYWORD

            let else_branch = self.mark();
            if !self.at(SEMICOLON) {
                self.parse_control_structure_body();
            }
            else_branch.done(self.my_builder(), ELSE);
        }

        marker.done(self.my_builder(), IF);
    }

    /// `parseCondition()`: `"(" element ")"`.
    pub(crate) fn parse_condition(&mut self) {
        self.my_builder().disable_newlines();

        if self.expect_with_recovery(LPAR, "Expecting a condition in parentheses '(...)'", Some(&EXPRESSION_FIRST)) {
            let condition = self.mark();
            self.parse_expression();
            condition.done(self.my_builder(), CONDITION);
            self.expect_with(RPAR, "Expecting ')");
        }

        self.my_builder().restore_newlines_state();
    }

    /// `parseJump(type)`: `"continue" getEntryPoint? | "break" getEntryPoint?`.
    pub(crate) fn parse_jump(&mut self, element_type: NodeKind) {
        debug_assert!(self._at(BREAK_KEYWORD) || self._at(CONTINUE_KEYWORD));

        let marker = self.mark();

        self.advance(); // BREAK_KEYWORD or CONTINUE_KEYWORD

        self.parse_label_reference_with_no_whitespace();

        marker.done(self.my_builder(), element_type);
    }

    /// `parseReturn()`: `"return" getEntryPoint? element?`.
    pub(crate) fn parse_return(&mut self) {
        debug_assert!(self._at(RETURN_KEYWORD));

        let return_expression = self.mark();

        self.advance(); // RETURN_KEYWORD

        self.parse_label_reference_with_no_whitespace();

        if self.at_set(&EXPRESSION_FIRST) && !self.at(EOL_OR_SEMICOLON) {
            self.parse_expression();
        }

        return_expression.done(self.my_builder(), RETURN);
    }

    /// `parseLabelReferenceWithNoWhitespace()`: `labelReference?`.
    pub(crate) fn parse_label_reference_with_no_whitespace(&mut self) {
        if self.at(AT) && !self.my_builder().newline_before_current_token() {
            if WHITE_SPACE_OR_COMMENT_BIT_SET.contains(self.my_builder().raw_lookup(-1)) {
                self.error("There should be no space or comments before '@' in label reference");
            }
            self.parse_label_reference();
        }
    }

    /// `parseLabelDefinition()`: `IDENTIFIER "@"`.
    pub(crate) fn parse_label_definition(&mut self) {
        debug_assert!(
            self.is_at_label_definition_or_missing_identifier(),
            "Callers must check that current token is IDENTIFIER followed with '@'"
        );

        let label_wrap = self.mark();
        let mark = self.mark();

        if self.at(AT) {
            self.error_and_advance("Expecting identifier before '@' in label definition");
            label_wrap.drop(self.my_builder());
            mark.drop(self.my_builder());
            return;
        }

        self.advance(); // IDENTIFIER
        self.advance(); // AT

        mark.done(self.my_builder(), LABEL);

        label_wrap.done(self.my_builder(), LABEL_QUALIFIER);
    }

    /// `parseLabelReference()`: `"@" IDENTIFIER`.
    pub(crate) fn parse_label_reference(&mut self) {
        debug_assert!(self._at(AT));

        let label_wrap = self.mark();

        let mark = self.mark();

        if self.my_builder().raw_lookup(1) != IDENTIFIER {
            self.error_and_advance("Label must be named"); // AT
            label_wrap.drop(self.my_builder());
            mark.drop(self.my_builder());
            return;
        }

        self.advance(); // AT
        self.advance(); // IDENTIFIER

        mark.done(self.my_builder(), LABEL);

        label_wrap.done(self.my_builder(), LABEL_QUALIFIER);
    }

    /// `parseThrow()`: `"throw" element`.
    pub(crate) fn parse_throw(&mut self) {
        debug_assert!(self._at(THROW_KEYWORD));

        let marker = self.mark();

        self.advance(); // THROW_KEYWORD

        self.parse_expression();

        marker.done(self.my_builder(), THROW);
    }

    /// `parseParenthesizedExpression()`: `"(" expression ")"`.
    pub(crate) fn parse_parenthesized_expression(&mut self) {
        debug_assert!(self._at(LPAR));

        let mark = self.mark();

        self.my_builder().disable_newlines();
        self.advance(); // LPAR
        if self.at(RPAR) {
            self.error("Expecting an expression");
        } else {
            self.parse_expression();
        }

        self.expect_with(RPAR, "Expecting ')'");
        self.my_builder().restore_newlines_state();

        mark.done(self.my_builder(), PARENTHESIZED);
    }

    /// `parseThisExpression()`: `"this" label?`.
    pub(crate) fn parse_this_expression(&mut self) {
        debug_assert!(self._at(THIS_KEYWORD));
        let mark = self.mark();

        let this_reference = self.mark();
        self.advance(); // THIS_KEYWORD
        this_reference.done(self.my_builder(), REFERENCE_EXPRESSION);

        self.parse_label_reference_with_no_whitespace();

        mark.done(self.my_builder(), THIS_EXPRESSION);
    }

    /// `parseSuperExpression()`: `"super" ("<" type ">")? label?`.
    pub(crate) fn parse_super_expression(&mut self) {
        debug_assert!(self._at(SUPER_KEYWORD));
        let mark = self.mark();

        let super_reference = self.mark();
        self.advance(); // SUPER_KEYWORD
        super_reference.done(self.my_builder(), REFERENCE_EXPRESSION);

        if self.at(LT) {
            // This may be "super < foo" or "super<foo>", thus the backtracking
            let supertype = self.mark();

            self.my_builder().disable_newlines();
            self.advance(); // LT

            self.my_kotlin_parsing.parse_type_ref();

            if self.at(GT) {
                self.advance(); // GT
                supertype.drop(self.my_builder());
            } else {
                supertype.rollback_to(self.my_builder());
            }
            self.my_builder().restore_newlines_state();
        }
        self.parse_label_reference_with_no_whitespace();

        mark.done(self.my_builder(), SUPER_EXPRESSION);
    }

    /// `parseValueArgumentList()`: `"(" (SimpleName "=")? "*"? element{","} ")"`.
    pub fn parse_value_argument_list(&mut self) {
        let list = self.mark();

        self.my_builder().disable_newlines();

        if self.expect_with_recovery(LPAR, "Expecting an argument list", Some(&EXPRESSION_FOLLOW)) {
            if !self.at(RPAR) {
                loop {
                    self.parse_value_argument();
                    if self.at(COLON) && self.lookahead(1) == IDENTIFIER {
                        self.error_and_advance_n("Unexpected type specification", 2);
                    }
                    if !self.at(COMMA) {
                        if self.at_set(&EXPRESSION_FIRST) {
                            self.error("Expecting ','");
                            continue;
                        } else {
                            break;
                        }
                    }
                    self.advance(); // COMMA
                    if self.at(RPAR) {
                        break;
                    }
                }
            }

            self.expect_with_recovery(RPAR, "Expecting ')'", Some(&EXPRESSION_FOLLOW));
        }

        self.my_builder().restore_newlines_state();

        list.done(self.my_builder(), VALUE_ARGUMENT_LIST);
    }

    /// `parseValueArgument()`: `(SimpleName "=")? "*"? element`.
    pub(crate) fn parse_value_argument(&mut self) {
        let argument = self.mark();
        if self.at(IDENTIFIER) && self.lookahead(1) == EQ {
            let arg_name = self.mark();
            let reference = self.mark();
            self.advance(); // IDENTIFIER
            reference.done(self.my_builder(), REFERENCE_EXPRESSION);
            arg_name.done(self.my_builder(), VALUE_ARGUMENT_NAME);
            self.advance(); // EQ
        }
        if self.at(MUL) {
            self.advance(); // MUL
        }
        self.parse_expression_with("Expecting an argument");
        argument.done(self.my_builder(), VALUE_ARGUMENT);
    }

    /// `parseObjectLiteral()`: `"object" (":" delegationSpecifier{","})? classBody`. Cannot make class body optional:
    /// `foo(object : F, A)`.
    pub fn parse_object_literal(&mut self) {
        let literal = self.mark();
        let declaration = self.mark();
        self.my_kotlin_parsing.parse_object(NameParsingMode::PROHIBITED, false); // Body is not optional because of foo(object : A, B)
        declaration.done(self.my_builder(), OBJECT_DECLARATION);
        literal.done(self.my_builder(), OBJECT_LITERAL);
    }

    /// `parseOneTokenExpression(type)`.
    pub(crate) fn parse_one_token_expression(&mut self, element_type: NodeKind) {
        let mark = self.mark();
        self.advance();
        mark.done(self.my_builder(), element_type);
    }

    /// `interruptedWithNewLine()`.
    pub(crate) fn interrupted_with_new_line(&mut self) -> bool {
        !ALLOW_NEWLINE_OPERATIONS.contains(self.tt()) && self.my_builder().newline_before_current_token()
    }
}

#[cfg(test)]
mod tests {
    //! The expression half on its own: `KotlinParsing.parseScript` without a preamble (no annotations, package or
    //! imports, so the declaration half is not needed), compared from the `SCRIPT` line on with what Kotlin 2.4.21
    //! prints for the same `.kts` text. Statements that are declarations, types and `try` need the other half.

    use crate::builder::{PRECEDING_ALL_BINDER, TRAILING_ALL_BINDER};
    use crate::dump::psi_to_string;
    use crate::parsing::abstract_parsing::AbstractKotlinParsing;
    use crate::parsing::kotlin_parsing::KotlinParsing;
    use crate::tree::{FileKind, Tree};

    use super::*;

    /// `parseScript()` minus `parsePreamble` and `checkForUnexpectedSymbols`, non-lazy (no collapsed lambdas, which
    /// the declaration half's `advanceBalancedBlock` would skip); the dump from its `SCRIPT` line on.
    fn script(text: &str) -> String {
        let tree = Tree::parse_with(text.as_bytes(), FileKind::Kts, |b| {
            let mut kp = KotlinParsing::create_for_top_level_non_lazy(b.handle());
            let file_marker = kp.mark();
            let script_marker = kp.mark();
            let block_marker = kp.mark();
            kp.my_expression_parsing().parse_statements_with(/*isScriptTopLevel = */ true);
            block_marker.done(kp.my_builder(), BLOCK);
            block_marker.set_custom_edge_token_binders(
                kp.my_builder(),
                Some(PRECEDING_ALL_BINDER),
                Some(TRAILING_ALL_BINDER),
            );
            script_marker.done(kp.my_builder(), SCRIPT);
            script_marker.set_custom_edge_token_binders(
                kp.my_builder(),
                Some(PRECEDING_ALL_BINDER),
                Some(TRAILING_ALL_BINDER),
            );
            file_marker.done(kp.my_builder(), KT_FILE);
        });
        from_script_line(&psi_to_string(&tree, "a.kts"))
    }

    fn from_script_line(dump: &str) -> String {
        let start = dump.find("\n  SCRIPT\n").map_or(0, |i| i + 1);
        dump[start..].to_string()
    }

    /// What Kotlin 2.4.21's `DebugUtil.psiToString` prints for these scripts, from the `SCRIPT` line on: recovery in a
    /// `when` entry (an empty error binds left), in a long template entry and after a keyword in a template, `;`
    /// separators, a labeled lambda, a lambda's trailing comma before `->`, and newlines before `.` and `?.`.
    #[test]
    #[rustfmt::skip]
    fn scripts_match_the_psi_oracle() {
        let cases: &[(&str, &str)] = &[
        (
            "x = when (a) {\n  else\n}\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        BINARY_EXPRESSION
          REFERENCE_EXPRESSION
            PsiElement(IDENTIFIER)('x')
          PsiWhiteSpace(' ')
          OPERATION_REFERENCE
            PsiElement(EQ)('=')
          PsiWhiteSpace(' ')
          WHEN
            PsiElement(when)('when')
            PsiWhiteSpace(' ')
            PsiElement(LPAR)('(')
            REFERENCE_EXPRESSION
              PsiElement(IDENTIFIER)('a')
            PsiElement(RPAR)(')')
            PsiWhiteSpace(' ')
            PsiElement(LBRACE)('{')
            PsiWhiteSpace('\n  ')
            WHEN_ENTRY
              PsiElement(else)('else')
              PsiErrorElement:Expecting '->'
                <empty list>
            PsiWhiteSpace('\n')
            PsiElement(RBRACE)('}')
      PsiWhiteSpace('\n')"#,
        ),
        (
            "\"abc ${ }\"\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        STRING_TEMPLATE
          PsiElement(OPEN_QUOTE)('"')
          LITERAL_STRING_TEMPLATE_ENTRY
            PsiElement(REGULAR_STRING_PART)('abc ')
          LONG_STRING_TEMPLATE_ENTRY
            PsiElement(LONG_TEMPLATE_ENTRY_START)('${')
            PsiErrorElement:Expecting an expression
              <empty list>
            PsiWhiteSpace(' ')
            PsiElement(LONG_TEMPLATE_ENTRY_END)('}')
          PsiElement(CLOSING_QUOTE)('"')
      PsiWhiteSpace('\n')"#,
        ),
        (
            "a; b;; c\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        REFERENCE_EXPRESSION
          PsiElement(IDENTIFIER)('a')
      PsiElement(SEMICOLON)(';')
      PsiWhiteSpace(' ')
      SCRIPT_INITIALIZER
        REFERENCE_EXPRESSION
          PsiElement(IDENTIFIER)('b')
      PsiErrorElement:Expecting an element
        PsiElement(DOUBLE_SEMICOLON)(';;')
      PsiWhiteSpace(' ')
      SCRIPT_INITIALIZER
        REFERENCE_EXPRESSION
          PsiElement(IDENTIFIER)('c')
      PsiWhiteSpace('\n')"#,
        ),
        (
            "l@ { }\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        LABELED_EXPRESSION
          LABEL_QUALIFIER
            LABEL
              PsiElement(IDENTIFIER)('l')
              PsiElement(AT)('@')
          PsiWhiteSpace(' ')
          LAMBDA_EXPRESSION
            FUNCTION_LITERAL
              PsiElement(LBRACE)('{')
              PsiWhiteSpace(' ')
              BLOCK
                <empty list>
              PsiElement(RBRACE)('}')
      PsiWhiteSpace('\n')"#,
        ),
        (
            "{ x, -> }\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        LAMBDA_EXPRESSION
          FUNCTION_LITERAL
            PsiElement(LBRACE)('{')
            PsiWhiteSpace(' ')
            VALUE_PARAMETER_LIST
              VALUE_PARAMETER
                PsiElement(IDENTIFIER)('x')
              PsiElement(COMMA)(',')
            PsiWhiteSpace(' ')
            PsiElement(ARROW)('->')
            PsiWhiteSpace(' ')
            BLOCK
              <empty list>
            PsiElement(RBRACE)('}')
      PsiWhiteSpace('\n')"#,
        ),
        (
            "a\n  .b\n  ?.c\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        SAFE_ACCESS_EXPRESSION
          DOT_QUALIFIED_EXPRESSION
            REFERENCE_EXPRESSION
              PsiElement(IDENTIFIER)('a')
            PsiWhiteSpace('\n  ')
            PsiElement(DOT)('.')
            REFERENCE_EXPRESSION
              PsiElement(IDENTIFIER)('b')
          PsiWhiteSpace('\n  ')
          PsiElement(SAFE_ACCESS)('?.')
          REFERENCE_EXPRESSION
            PsiElement(IDENTIFIER)('c')
      PsiWhiteSpace('\n')"#,
        ),
        (
            "\"${if}\"\n",
            r#"  SCRIPT
    BLOCK
      SCRIPT_INITIALIZER
        STRING_TEMPLATE
          PsiElement(OPEN_QUOTE)('"')
          LONG_STRING_TEMPLATE_ENTRY
            PsiElement(LONG_TEMPLATE_ENTRY_START)('${')
            IF
              PsiElement(if)('if')
              PsiErrorElement:Expecting a condition in parentheses '(...)'
                PsiElement(LONG_TEMPLATE_ENTRY_END)('}')
              THEN
                PsiErrorElement:Expecting an expression
                  <empty list>
            PsiElement(CLOSING_QUOTE)('"')
          PsiErrorElement:Expecting '"'
            <empty list>
      PsiWhiteSpace('\n')"#,
        ),
        ];
        for (text, expected) in cases {
            assert_eq!(script(text), *expected, "{text:?}");
        }
    }

    /// `KS_SNIPPETS=DIR KS_ORACLE=OUT cargo test --release --lib snippets -- --ignored`: every `DIR/*.kts` against
    /// `OUT/<name>.psi.txt` of the PSI oracle.
    #[test]
    #[ignore]
    fn snippets() {
        let (Ok(dir), Ok(oracle)) = (std::env::var("KS_SNIPPETS"), std::env::var("KS_ORACLE")) else { return };
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        entries.sort();
        let (mut pass, mut total) = (0, 0);
        for path in entries.iter().filter(|p| p.extension().is_some_and(|e| e == "kts")) {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let Ok(expected) = std::fs::read_to_string(format!("{oracle}/{name}.psi.txt")) else { continue };
            total += 1;
            let text = std::fs::read_to_string(path).unwrap();
            let ours = script(&text);
            let expected = from_script_line(&expected);
            if ours == expected {
                pass += 1;
            } else {
                let line = ours.lines().zip(expected.lines()).position(|(a, b)| a != b).unwrap_or(0);
                println!("{name}: line {line} of the SCRIPT dump");
                println!("  ours:   {:?}", ours.lines().nth(line));
                println!("  oracle: {:?}", expected.lines().nth(line));
            }
        }
        println!("snippets: {pass}/{total}");
    }
}
