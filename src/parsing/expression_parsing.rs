//! The port of `KotlinExpressionParsing.java` (expressions, statements, string templates, `when`, lambdas), and of
//! `BinaryOperationPrecedence` (psi-api) it parses binary operations with.
//!
//! `KotlinExpressionParsing` is a view of the `KotlinParsing` that owns it: the Java object pair (each holding the
//! other and the same builder) is one `KotlinParsing` plus this borrow of it. `myKotlinParsing.x()` is
//! `self.my_kotlin_parsing.x()`; `KotlinParsing`'s `myExpressionParsing.y()` is `self.my_expression_parsing().y()`.
//! The anonymous subclass `KotlinParsing` creates for a by-clause (`isTopLevel == false`) is the same type: its
//! overrides `parseCallWithClosure` and `create` check `my_kotlin_parsing.is_top_level`.
//!
//! SKELETON: wiring and static token sets are complete; every body marked `TODO(port)` is a stub that consumes
//! nothing, to be ported from the Java method at the line it names (KotlinExpressionParsing.java of Kotlin 2.4.21).
//!
//! Naming of Java overloads: `parseExpression(String)` = `parse_expression_with`, `parseFunctionLiteral(boolean,
//! boolean)` = `parse_function_literal_with`, `parseStatements(boolean)` = `parse_statements_with`. A Java
//! `IElementType` result is `Option<NodeKind>`; `String` parameters that only ever receive literals are `&'static str`.

// The Java names of the nested enums' constants are kept; the skeleton's stubs leave parameters and imports
// unused until the port fills them.
#![allow(non_camel_case_types, unused_variables, unused_imports, dead_code)]

use crate::builder::{Adapter, Marker, SemanticWhitespaceAwarePsiBuilder};
use crate::node_kind::NodeKind;
use crate::node_kind::NodeKind::*;
use crate::parsing::abstract_parsing::AbstractKotlinParsing;
use crate::parsing::kotlin_parsing::{EXPRESSION_FIRST, KotlinParsing, ModifierDetector, PARAMETER_NAME_RECOVERY_SET};
use crate::tokens::TokenKind::*;
use crate::tokens::{KEYWORDS, MODIFIER_KEYWORDS, SOFT_KEYWORDS, TokenKind, TokenSet};

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
fn keyword_texts(text: &str) -> Option<TokenKind> {
    let keyword = TokenKind::keyword_by_debug_name(text.as_bytes())?;
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
        // TODO(port): KotlinExpressionParsing.java:142
    }

    /// `parseExpression(messageIfNotExpressionFirst)`.
    pub(crate) fn parse_expression_with(&mut self, message_if_not_expression_first: &'static str) {
        // TODO(port): KotlinExpressionParsing.java:146
    }

    /// `parseBinaryExpression(maxPrecedence)`: `element (operation element)*`; returns the `minPrecedence` to consider
    /// next.
    pub(crate) fn parse_binary_expression(
        &mut self,
        max_precedence: BinaryOperationPrecedence,
    ) -> BinaryOperationPrecedence {
        // TODO(port): KotlinExpressionParsing.java:161
        MIN_BINARY_OPERATION_PRECEDENCE
    }

    /// `parseLabeledExpression()`.
    pub(crate) fn parse_labeled_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:241
    }

    /// `parsePrefixExpression()`.
    pub(crate) fn parse_prefix_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:251
    }

    /// `parseDoubleColonSuffix(expression)`.
    pub(crate) fn parse_double_colon_suffix(&mut self, expression: Marker) -> bool {
        // TODO(port): KotlinExpressionParsing.java:288
        false
    }

    /// `skipQuestionMarksBeforeDoubleColon()`.
    pub(crate) fn skip_question_marks_before_double_colon(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:322
    }

    /// `parsePostfixExpression()`.
    pub(crate) fn parse_postfix_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:348
    }

    /// `parseCallSuffix()`.
    pub(crate) fn parse_call_suffix(&mut self) -> bool {
        // TODO(port): KotlinExpressionParsing.java:400
        false
    }

    /// `parseSelectorCallExpression()`.
    pub(crate) fn parse_selector_call_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:430
    }

    /// `parseOperationReference()`.
    pub(crate) fn parse_operation_reference(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:441
    }

    /// `parseCallWithClosure()`: `annotatedLambda*`. The by-clause subclass overrides it: no trailing lambdas while
    /// its builder's newline stack is at the by-clause's level (`getStackSize() > 0` falls through to this body).
    pub(crate) fn parse_call_with_closure(&mut self) -> bool {
        if !self.my_kotlin_parsing.is_top_level && self.my_builder().get_stack_size() <= 0 {
            return false;
        }
        // TODO(port): KotlinExpressionParsing.java:450 (the base body)
        false
    }

    /// `parseAnnotatedLambda(preferBlock)`: `("@" annotationEntry)* labelDefinition? functionLiteral`.
    pub(crate) fn parse_annotated_lambda(&mut self, prefer_block: bool) -> bool {
        // TODO(port): KotlinExpressionParsing.java:472
        false
    }

    /// `doneOrDrop(marker, type, condition)`.
    pub(crate) fn done_or_drop(&mut self, marker: Marker, element_type: NodeKind, condition: bool) {
        // TODO(port): KotlinExpressionParsing.java:496
    }

    /// `isAtLabelDefinitionOrMissingIdentifier()`.
    pub(crate) fn is_at_label_definition_or_missing_identifier(&mut self) -> bool {
        // TODO(port): KotlinExpressionParsing.java:509
        false
    }

    /// `parseAtomicExpression()`.
    pub(crate) fn parse_atomic_expression(&mut self) -> bool {
        // TODO(port): KotlinExpressionParsing.java:530
        false
    }

    /// `parseStringTemplate()`.
    pub(crate) fn parse_string_template(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:653
    }

    /// `parseStringTemplateElement()`.
    pub(crate) fn parse_string_template_element(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:695
    }

    /// `parseWhen()`.
    pub(crate) fn parse_when(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:769
    }

    /// `parseWhenEntry()`.
    pub(crate) fn parse_when_entry(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:819
    }

    /// `parseWhenEntryNotElse()`.
    pub(crate) fn parse_when_entry_not_else(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:859
    }

    /// `parseWhenCondition()`.
    pub(crate) fn parse_when_condition(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:889
    }

    /// `parseWhenEntryGuardOrSuggest()`.
    pub(crate) fn parse_when_entry_guard_or_suggest(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:935
    }

    /// `parseWhenEntryGuard()`.
    pub(crate) fn parse_when_entry_guard(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:948
    }

    /// `parseArrayAccess()`.
    pub(crate) fn parse_array_access(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:962
    }

    /// `parseCollectionLiteralExpression()`.
    pub(crate) fn parse_collection_literal_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:971
    }

    /// `parseAsCollectionLiteralExpression(nodeType, canBeEmpty, missingElementErrorMessage)`.
    pub(crate) fn parse_as_collection_literal_expression(
        &mut self,
        node_type: NodeKind,
        can_be_empty: bool,
        missing_element_error_message: &'static str,
    ) {
        // TODO(port): KotlinExpressionParsing.java:975
    }

    /// `parseInnerExpressions(missingElementErrorMessage)`.
    pub(crate) fn parse_inner_expressions(&mut self, missing_element_error_message: &'static str) {
        // TODO(port): KotlinExpressionParsing.java:996
    }

    /// `parseContractDescriptionBlock()`.
    pub fn parse_contract_description_block(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1008
    }

    /// `parseContractEffectList()`.
    pub(crate) fn parse_contract_effect_list(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1016
    }

    /// `parseContractEffects()`.
    pub(crate) fn parse_contract_effects(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1030
    }

    /// `parseSimpleNameExpression()`.
    pub fn parse_simple_name_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1048
    }

    /// `parseLocalDeclaration(rollbackIfDefinitelyNotExpression, isScriptTopLevel)`.
    pub(crate) fn parse_local_declaration(
        &mut self,
        rollback_if_definitely_not_expression: bool,
        is_script_top_level: bool,
    ) -> bool {
        // TODO(port): KotlinExpressionParsing.java:1057
        false
    }

    /// `parseFunctionLiteral()`.
    pub(crate) fn parse_function_literal(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1082
    }

    /// `parseFunctionLiteral(preferBlock, collapse)`: a block if it has no `->`, else a function literal.
    pub fn parse_function_literal_with(&mut self, prefer_block: bool, collapse: bool) {
        // TODO(port): KotlinExpressionParsing.java:1091
    }

    /// `rollbackOrDropAt(rollbackMarker, dropAt)`.
    pub(crate) fn rollback_or_drop_at(&mut self, rollback_marker: Marker, drop_at: TokenKind) -> bool {
        // TODO(port): KotlinExpressionParsing.java:1155
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
        // TODO(port): KotlinExpressionParsing.java:1165
        false
    }

    /// `parseFunctionLiteralParameterList()`.
    pub(crate) fn parse_function_literal_parameter_list(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1191
    }

    /// `parseStatements()`.
    pub fn parse_statements(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1241
    }

    /// `parseStatements(isScriptTopLevel)`: `SEMI* statement{SEMI+} SEMI*`.
    pub fn parse_statements_with(&mut self, is_script_top_level: bool) {
        // TODO(port): KotlinExpressionParsing.java:1249
    }

    /// `parseStatement(isScriptTopLevel)`.
    pub(crate) fn parse_statement(&mut self, is_script_top_level: bool) {
        // TODO(port): KotlinExpressionParsing.java:1283
    }

    /// `parseBlockLevelExpression()`.
    pub(crate) fn parse_block_level_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1304
    }

    /// `parseLocalDeclarationRest(modifierDetector, failIfDefinitelyNotExpression, isScriptTopLevel)`.
    pub(crate) fn parse_local_declaration_rest(
        &mut self,
        modifier_detector: &ModifierDetector,
        fail_if_definitely_not_expression: bool,
        is_script_top_level: bool,
    ) -> Option<NodeKind> {
        // TODO(port): KotlinExpressionParsing.java:1334
        None
    }

    /// `parseDoWhile()`.
    pub(crate) fn parse_do_while(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1371
    }

    /// `parseWhile()`.
    pub(crate) fn parse_while(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1394
    }

    /// `parseFor()`.
    pub(crate) fn parse_for(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1415
    }

    /// `parseControlStructureBody()`.
    pub(crate) fn parse_control_structure_body(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1472
    }

    /// `parseLoopBody()`.
    pub(crate) fn parse_loop_body(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1481
    }

    /// `parseTry()`.
    pub(crate) fn parse_try(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1501
    }

    /// `parseIf()`.
    pub(crate) fn parse_if(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1567
    }

    /// `parseCondition()`.
    pub(crate) fn parse_condition(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1602
    }

    /// `parseJump(type)`.
    pub(crate) fn parse_jump(&mut self, element_type: NodeKind) {
        // TODO(port): KotlinExpressionParsing.java:1619
    }

    /// `parseReturn()`.
    pub(crate) fn parse_return(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1634
    }

    /// `parseLabelReferenceWithNoWhitespace()`.
    pub(crate) fn parse_label_reference_with_no_whitespace(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1651
    }

    /// `parseLabelDefinition()`.
    pub(crate) fn parse_label_definition(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1663
    }

    /// `parseLabelReference()`.
    pub(crate) fn parse_label_reference(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1687
    }

    /// `parseThrow()`.
    pub(crate) fn parse_throw(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1712
    }

    /// `parseParenthesizedExpression()`.
    pub(crate) fn parse_parenthesized_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1727
    }

    /// `parseThisExpression()`.
    pub(crate) fn parse_this_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1750
    }

    /// `parseSuperExpression()`.
    pub(crate) fn parse_super_expression(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1766
    }

    /// `parseValueArgumentList()`.
    pub fn parse_value_argument_list(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1802
    }

    /// `parseValueArgument()`.
    pub(crate) fn parse_value_argument(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1841
    }

    /// `parseObjectLiteral()`.
    pub fn parse_object_literal(&mut self) {
        // TODO(port): KotlinExpressionParsing.java:1861
    }

    /// `parseOneTokenExpression(type)`.
    pub(crate) fn parse_one_token_expression(&mut self, element_type: NodeKind) {
        // TODO(port): KotlinExpressionParsing.java:1869
    }

    /// `interruptedWithNewLine()`.
    pub(crate) fn interrupted_with_new_line(&mut self) -> bool {
        // TODO(port): KotlinExpressionParsing.java:1880
        false
    }
}
