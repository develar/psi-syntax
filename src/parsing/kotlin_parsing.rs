//! The port of `KotlinParsing.java` (declarations, types, modifiers, annotations). It creates its
//! `KotlinExpressionParsing` and each calls the other, as in Java.
//!
//! SKELETON: the struct, the constructors, the wiring (`create`, `createTruncatedBuilder`, `createForByClause`, the
//! expression parsing and its by-clause overrides), the nested types and the static token sets are complete; every
//! method body marked `TODO(port)` is a stub that consumes nothing, to be ported line by line from the Java method
//! at the line it names (KotlinParsing.java of Kotlin 2.4.21).
//!
//! Naming of Java overloads: `parseModifierList(Consumer, TokenSet, boolean)` = `parse_modifier_list_with_consumer`,
//! `parseFunction(boolean)` = `parse_function_with`, `parseBlock(boolean)` = `parse_block_with`,
//! `parseTypeRef(TokenSet)` = `parse_type_ref_with`, `parseTypeRef(TokenSet, boolean)` =
//! `parse_type_ref_with_intersections`, `parseValueParameter(boolean, boolean)` = `parse_value_parameter_with`.
//! A Java `IElementType` result is `Option<NodeKind>` (`null` = `None`); a `Consumer<IElementType>` is
//! `Option<&mut ModifierDetector>`, the only consumer the parser passes; a `Supplier<Boolean>` is
//! `&mut dyn FnMut(&mut Self) -> bool`; `String` parameters that only ever receive literals are `&'static str`.

// The Java names of the nested enums' constants are kept; the skeleton's stubs leave parameters and imports
// unused until the port fills them.
#![allow(non_camel_case_types, unused_variables, unused_imports, dead_code)]

use crate::builder::{Adapter, Marker, SemanticWhitespaceAwarePsiBuilder};
use crate::node_kind::NodeKind;
use crate::node_kind::NodeKind::*;
use crate::parsing::abstract_parsing::AbstractKotlinParsing;
use crate::parsing::expression_parsing::KotlinExpressionParsing;
use crate::parsing::patterns::{FirstBefore, LastBefore, TokenStreamPattern, TokenStreamPredicate};
use crate::tokens::TokenKind::*;
use crate::tokens::{MODIFIER_KEYWORDS, TokenKind, TokenSet, VAL_VAR};

const GT_COMMA_COLON_SET: TokenSet = TokenSet::create(&[GT, COMMA, COLON]);

const TOP_LEVEL_DECLARATION_FIRST: TokenSet = TokenSet::create(&[
    TYPE_ALIAS_KEYWORD,
    INTERFACE_KEYWORD,
    CLASS_KEYWORD,
    OBJECT_KEYWORD,
    FUN_KEYWORD,
    VAL_KEYWORD,
    VAR_KEYWORD,
    PACKAGE_KEYWORD,
]);
const TOP_LEVEL_DECLARATION_FIRST_SEMICOLON_SET: TokenSet =
    TokenSet::or_set(&[TOP_LEVEL_DECLARATION_FIRST, TokenSet::create(&[SEMICOLON])]);
const LT_EQ_SEMICOLON_TOP_LEVEL_DECLARATION_FIRST_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[LT, EQ, SEMICOLON]), TOP_LEVEL_DECLARATION_FIRST]);
const DECLARATION_FIRST: TokenSet = TokenSet::or_set(&[
    TOP_LEVEL_DECLARATION_FIRST,
    TokenSet::create(&[INIT_KEYWORD, GET_KEYWORD, SET_KEYWORD, CONSTRUCTOR_KEYWORD]),
]);

const CLASS_NAME_RECOVERY_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[LT, LPAR, COLON, LBRACE]), TOP_LEVEL_DECLARATION_FIRST]);
const TYPE_PARAMETER_GT_RECOVERY_SET: TokenSet = TokenSet::create(&[WHERE_KEYWORD, LPAR, COLON, LBRACE, GT]);
pub const PARAMETER_NAME_RECOVERY_SET: TokenSet = TokenSet::create(&[COLON, EQ, COMMA, RPAR, VAL_KEYWORD, VAR_KEYWORD]);
const PACKAGE_NAME_RECOVERY_SET: TokenSet = TokenSet::create(&[DOT, EOL_OR_SEMICOLON]);
const IMPORT_RECOVERY_SET: TokenSet = TokenSet::create(&[AS_KEYWORD, DOT, EOL_OR_SEMICOLON]);
const TYPE_REF_FIRST: TokenSet = TokenSet::create(&[LBRACKET, IDENTIFIER, LPAR, HASH, DYNAMIC_KEYWORD]);
const LBRACE_RBRACE_TYPE_REF_FIRST_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[LBRACE, RBRACE]), TYPE_REF_FIRST]);
const COLON_COMMA_LBRACE_RBRACE_TYPE_REF_FIRST_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[COLON, COMMA, LBRACE, RBRACE]), TYPE_REF_FIRST]);
const RECEIVER_TYPE_TERMINATORS: TokenSet = TokenSet::create(&[DOT, SAFE_ACCESS]);
const VALUE_PARAMETER_FIRST: TokenSet = TokenSet::or_set(&[
    TokenSet::create(&[IDENTIFIER, LBRACKET, VAL_KEYWORD, VAR_KEYWORD]),
    TokenSet::and_not(MODIFIER_KEYWORDS, TokenSet::create(&[FUN_KEYWORD])),
]);
const LAMBDA_VALUE_PARAMETER_FIRST: TokenSet = TokenSet::or_set(&[
    TokenSet::create(&[IDENTIFIER, LBRACKET]),
    TokenSet::and_not(MODIFIER_KEYWORDS, TokenSet::create(&[FUN_KEYWORD])),
]);
const SOFT_KEYWORDS_AT_MEMBER_START: TokenSet = TokenSet::create(&[CONSTRUCTOR_KEYWORD, INIT_KEYWORD]);
const ANNOTATION_TARGETS: TokenSet = TokenSet::create(&[
    ALL_KEYWORD,
    FILE_KEYWORD,
    FIELD_KEYWORD,
    GET_KEYWORD,
    SET_KEYWORD,
    PROPERTY_KEYWORD,
    RECEIVER_KEYWORD,
    PARAM_KEYWORD,
    SETPARAM_KEYWORD,
    DELEGATE_KEYWORD,
]);
const BLOCK_DOC_COMMENT_SET: TokenSet = TokenSet::create(&[BLOCK_COMMENT, DOC_COMMENT]);
const SEMICOLON_SET: TokenSet = TokenSet::create(&[SEMICOLON]);
const COMMA_COLON_GT_SET: TokenSet = TokenSet::create(&[COMMA, COLON, GT]);
const IDENTIFIER_RBRACKET_LBRACKET_SET: TokenSet = TokenSet::create(&[IDENTIFIER, RBRACKET, LBRACKET]);
const LBRACE_RBRACE_SET: TokenSet = TokenSet::create(&[LBRACE, RBRACE]);
const COMMA_SEMICOLON_RBRACE_SET: TokenSet = TokenSet::create(&[COMMA, SEMICOLON, RBRACE]);
const VALUE_ARGS_RECOVERY_SET: TokenSet = TokenSet::create(&[LBRACE, SEMICOLON, RPAR, EOL_OR_SEMICOLON, RBRACE]);
const PROPERTY_NAME_FOLLOW_SET: TokenSet =
    TokenSet::create(&[COLON, EQ, LBRACE, RBRACE, SEMICOLON, VAL_KEYWORD, VAR_KEYWORD, FUN_KEYWORD, CLASS_KEYWORD]);
const DESTRUCTURING_PROPERTY_NAME_FOLLOW_SET: TokenSet = TokenSet::and_not(PROPERTY_NAME_FOLLOW_SET, VAL_VAR);
const PROPERTY_NAME_FOLLOW_MULTI_DECLARATION_RECOVERY_SET: TokenSet =
    TokenSet::or_set(&[PROPERTY_NAME_FOLLOW_SET, PARAMETER_NAME_RECOVERY_SET]);
const PROPERTY_NAME_FOLLOW_FUNCTION_OR_PROPERTY_RECOVERY_SET: TokenSet =
    TokenSet::or_set(&[PROPERTY_NAME_FOLLOW_SET, LBRACE_RBRACE_SET, TOP_LEVEL_DECLARATION_FIRST]);
const IDENTIFIER_EQ_COLON_SEMICOLON_SET: TokenSet = TokenSet::create(&[IDENTIFIER, EQ, COLON, SEMICOLON]);
const COMMA_RPAR_RBRACKET_COLON_EQ_SET: TokenSet = TokenSet::create(&[COMMA, RPAR, RBRACKET, COLON, EQ]);
const ACCESSOR_FIRST_OR_PROPERTY_END: TokenSet = TokenSet::or_set(&[
    MODIFIER_KEYWORDS,
    TokenSet::create(&[AT, GET_KEYWORD, SET_KEYWORD, FIELD_KEYWORD, EOL_OR_SEMICOLON, RBRACE]),
]);
const RPAR_IDENTIFIER_COLON_LBRACE_EQ_SET: TokenSet = TokenSet::create(&[RPAR, IDENTIFIER, COLON, LBRACE, EQ]);
const COMMA_COLON_RPAR_SET: TokenSet = TokenSet::create(&[COMMA, COLON, RPAR]);
const RPAR_COLON_LBRACE_EQ_SET: TokenSet = TokenSet::create(&[RPAR, COLON, LBRACE, EQ]);
const LBRACKET_LBRACE_RBRACE_LPAR_SET: TokenSet = TokenSet::create(&[LBRACKET, LBRACE, RBRACE, LPAR]);
const FUNCTION_NAME_FOLLOW_SET: TokenSet = TokenSet::create(&[LT, LPAR, RPAR, COLON, EQ]);
const FUNCTION_NAME_RECOVERY_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[LT, LPAR, RPAR, COLON, EQ]), LBRACE_RBRACE_SET, TOP_LEVEL_DECLARATION_FIRST]);
const VALUE_PARAMETERS_FOLLOW_SET: TokenSet = TokenSet::create(&[EQ, LBRACE, RBRACE, SEMICOLON, RPAR]);
const CONTEXT_PARAMETERS_FOLLOW_SET: TokenSet =
    TokenSet::create(&[CLASS_KEYWORD, OBJECT_KEYWORD, FUN_KEYWORD, VAL_KEYWORD, VAR_KEYWORD]);
const LPAR_VALUE_PARAMETERS_FOLLOW_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[LPAR]), VALUE_PARAMETERS_FOLLOW_SET]);
const LPAR_LBRACE_COLON_CONSTRUCTOR_KEYWORD_SET: TokenSet =
    TokenSet::create(&[LPAR, LBRACE, COLON, CONSTRUCTOR_KEYWORD]);
const DEFINITELY_OUT_OF_RECEIVER_SET: TokenSet =
    TokenSet::or_set(&[TokenSet::create(&[EQ, COLON, LBRACE, RBRACE, BY_KEYWORD]), TOP_LEVEL_DECLARATION_FIRST]);
const EOL_OR_SEMICOLON_RBRACE_SET: TokenSet = TokenSet::create(&[EOL_OR_SEMICOLON, RBRACE]);
const CLASS_INTERFACE_SET: TokenSet = TokenSet::create(&[CLASS_KEYWORD, INTERFACE_KEYWORD]);

const NO_MODIFIER_BEFORE_FOR_VALUE_PARAMETER: TokenSet = TokenSet::create(&[COMMA, COLON, EQ, RPAR]);

/// `KotlinParsing.EXPRESSION_FIRST`.
pub const EXPRESSION_FIRST: TokenSet = TokenSet::create(&[
    // Prefix
    MINUS,
    PLUS,
    MINUSMINUS,
    PLUSPLUS,
    EXCL,
    EXCLEXCL, // Joining complex tokens makes it necessary to put EXCLEXCL here
    // Atomic
    COLONCOLON, // callable reference
    LPAR,       // parenthesized
    // literal constant
    TRUE_KEYWORD,
    FALSE_KEYWORD,
    INTERPOLATION_PREFIX,
    OPEN_QUOTE,
    INTEGER_LITERAL,
    CHARACTER_LITERAL,
    FLOAT_LITERAL,
    NULL_KEYWORD,
    LBRACE,         // functionLiteral
    FUN_KEYWORD,    // expression function
    THIS_KEYWORD,   // this
    SUPER_KEYWORD,  // super
    IF_KEYWORD,     // if
    WHEN_KEYWORD,   // when
    TRY_KEYWORD,    // try
    OBJECT_KEYWORD, // object
    // jump
    THROW_KEYWORD,
    RETURN_KEYWORD,
    CONTINUE_KEYWORD,
    BREAK_KEYWORD, // loop
    FOR_KEYWORD,
    WHILE_KEYWORD,
    DO_KEYWORD,
    IDENTIFIER, // SimpleName
    AT,         // Just for better recovery and maybe for annotations
    LBRACKET,   // Collection literal expression
]);

/// `KotlinParsing.EXPRESSION_FOLLOW`.
pub const EXPRESSION_FOLLOW: TokenSet = TokenSet::create(&[EOL_OR_SEMICOLON, ARROW, COMMA, RBRACE, RPAR, RBRACKET]);

const USER_TYPE_NAME_RECOVERY_SET: TokenSet =
    TokenSet::or_set(&[EXPRESSION_FIRST, EXPRESSION_FOLLOW, DECLARATION_FIRST]);

/// `KotlinParsing.NameParsingMode`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NameParsingMode {
    REQUIRED,
    ALLOWED,
    PROHIBITED,
}

/// `KotlinParsing.ParseEnumEntryResult`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ParseEnumEntryResult {
    FAILED,
    NO_DELIMITER,
    COMMA_DELIMITER,
    SEMICOLON_DELIMITER,
}

/// `KotlinParsing.DeclarationParsingMode`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DeclarationParsingMode {
    MEMBER_OR_TOPLEVEL,
    LOCAL,
    SCRIPT_TOPLEVEL,
}

impl DeclarationParsingMode {
    pub const fn destructuring_allowed(self) -> bool {
        matches!(self, DeclarationParsingMode::LOCAL | DeclarationParsingMode::SCRIPT_TOPLEVEL)
    }

    pub const fn accessors_allowed(self) -> bool {
        matches!(self, DeclarationParsingMode::MEMBER_OR_TOPLEVEL | DeclarationParsingMode::SCRIPT_TOPLEVEL)
    }

    pub const fn can_be_enum_used_as_soft_keyword(self) -> bool {
        matches!(self, DeclarationParsingMode::MEMBER_OR_TOPLEVEL)
    }
}

/// `KotlinParsing.MultiDeclarationMode`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MultiDeclarationMode {
    SHORT,
    FULL,
    FULL_VAL_ONLY,
}

/// `KotlinParsing.PropertyComponentKind`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PropertyComponentKind {
    GET,
    SET,
    FIELD,
}

/// `KotlinParsing.PropertyComponentKind.Collector`.
#[derive(Clone, Debug, Default)]
pub struct PropertyComponentKindCollector {
    collected: [bool; 3],
}

impl PropertyComponentKindCollector {
    pub fn collect(&mut self, kind: PropertyComponentKind) {
        self.collected[kind as usize] = true;
    }

    pub fn contains(&self, kind: PropertyComponentKind) -> bool {
        self.collected[kind as usize]
    }
}

/// `KotlinParsing.ModifierDetector`: the `Consumer<IElementType>` that notes `enum` and `companion` modifiers.
#[derive(Clone, Debug, Default)]
pub struct ModifierDetector {
    enum_detected: bool,
    companion_detected: bool,
}

impl ModifierDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// `consume(item)`.
    pub fn consume(&mut self, item: TokenKind) {
        if item == ENUM_KEYWORD {
            self.enum_detected = true;
        } else if item == COMPANION_KEYWORD {
            self.companion_detected = true;
        }
    }

    pub fn is_enum_detected(&self) -> bool {
        self.enum_detected
    }

    pub fn is_companion_detected(&self) -> bool {
        self.companion_detected
    }
}

/// `KotlinParsing.AnnotationParsingMode`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AnnotationParsingMode {
    DEFAULT,
    FILE_ANNOTATIONS_BEFORE_PACKAGE,
    FILE_ANNOTATIONS_WHEN_PACKAGE_OMITTED,
    TYPE_CONTEXT,
    WITH_SIGNIFICANT_WHITESPACE_BEFORE_ARGUMENTS,
    WITH_SIGNIFICANT_WHITESPACE_BEFORE_ARGUMENTS_NO_CONTEXT,
    NO_ANNOTATIONS_NO_CONTEXT,
}

impl AnnotationParsingMode {
    /// `(isFileAnnotationParsingMode, allowAnnotations, allowContextList, typeContext,
    /// withSignificantWhitespaceBeforeArguments)` as the Java constructor receives them.
    const fn fields(self) -> (bool, bool, bool, bool, bool) {
        use AnnotationParsingMode::*;
        match self {
            DEFAULT => (false, true, true, false, false),
            FILE_ANNOTATIONS_BEFORE_PACKAGE => (true, true, false, false, false),
            FILE_ANNOTATIONS_WHEN_PACKAGE_OMITTED => (true, true, false, false, false),
            TYPE_CONTEXT => (false, true, false, true, false),
            WITH_SIGNIFICANT_WHITESPACE_BEFORE_ARGUMENTS => (false, true, true, true, true),
            WITH_SIGNIFICANT_WHITESPACE_BEFORE_ARGUMENTS_NO_CONTEXT => (false, true, false, true, true),
            NO_ANNOTATIONS_NO_CONTEXT => (false, false, false, false, false),
        }
    }

    pub const fn is_file_annotation_parsing_mode(self) -> bool {
        self.fields().0
    }

    pub const fn allow_annotations(self) -> bool {
        self.fields().1
    }

    pub const fn allow_context_list(self) -> bool {
        self.fields().2
    }

    pub const fn type_context(self) -> bool {
        self.fields().3
    }

    pub const fn with_significant_whitespace_before_arguments(self) -> bool {
        self.fields().4
    }
}

/// `KotlinParsing`.
pub struct KotlinParsing<'b> {
    /// `myBuilder`.
    pub(crate) my_builder: SemanticWhitespaceAwarePsiBuilder<'b>,
    /// `isLazy`.
    pub(crate) is_lazy: bool,
    /// The constructor's `isTopLevel`: `false` for the parser of `createForByClause`, whose `myExpressionParsing` is
    /// the anonymous subclass overriding `parseCallWithClosure` and `create`.
    pub(crate) is_top_level: bool,
}

impl<'b> AbstractKotlinParsing<'b> for KotlinParsing<'b> {
    fn my_builder(&mut self) -> &mut SemanticWhitespaceAwarePsiBuilder<'b> {
        &mut self.my_builder
    }

    fn is_lazy(&self) -> bool {
        self.is_lazy
    }

    fn create_truncated_builder(&mut self, eof_position: i32) -> KotlinParsing<'_> {
        KotlinParsing::create(self.my_builder.reborrow().wrapped(Adapter::Truncated { eof_position }))
    }
}

impl<'b> KotlinParsing<'b> {
    /// `createForTopLevel(builder)`.
    pub fn create_for_top_level(builder: SemanticWhitespaceAwarePsiBuilder<'b>) -> Self {
        KotlinParsing::new(builder, true, true)
    }

    /// `createForTopLevelNonLazy(builder)`.
    pub fn create_for_top_level_non_lazy(builder: SemanticWhitespaceAwarePsiBuilder<'b>) -> Self {
        KotlinParsing::new(builder, true, false)
    }

    /// `createForByClause(builder, isLazy)`: a parser over `new SemanticWhitespaceAwarePsiBuilderForByClause(builder)`.
    pub(crate) fn create_for_by_clause(builder: SemanticWhitespaceAwarePsiBuilder<'b>, is_lazy: bool) -> Self {
        KotlinParsing::new(builder.wrapped(Adapter::ForByClause { stack_size: 0 }), false, is_lazy)
    }

    /// The private constructor `KotlinParsing(builder, isTopLevel, isLazy)`.
    fn new(builder: SemanticWhitespaceAwarePsiBuilder<'b>, is_top_level: bool, is_lazy: bool) -> Self {
        KotlinParsing { my_builder: builder, is_lazy, is_top_level }
    }

    /// `myExpressionParsing`: the expression parser over the same builder, calling back into this parser.
    pub(crate) fn my_expression_parsing(&mut self) -> KotlinExpressionParsing<'_, 'b> {
        KotlinExpressionParsing::new(self)
    }

    /// `create(builder)` (`@Override`): `createForTopLevel(builder)`.
    pub(crate) fn create(builder: SemanticWhitespaceAwarePsiBuilder<'_>) -> KotlinParsing<'_> {
        KotlinParsing::create_for_top_level(builder)
    }

    /// The field `lastDotAfterReceiverLParPattern`.
    fn last_dot_after_receiver_l_par_pattern() -> FirstBefore<KotlinParsing<'b>> {
        FirstBefore::new(
            TokenStreamPredicate::at_set(RECEIVER_TYPE_TERMINATORS),
            TokenStreamPredicate::Custom(|p: &mut KotlinParsing<'b>, top_level: bool| {
                if top_level && p.at_set(&DEFINITELY_OUT_OF_RECEIVER_SET) {
                    return true;
                }
                top_level && !p.at(QUEST) && !p.at(LPAR) && !p.at(RPAR)
            }),
        )
    }

    /// The field `lastDotAfterReceiverNotLParPattern`.
    fn last_dot_after_receiver_not_l_par_pattern() -> LastBefore<KotlinParsing<'b>> {
        LastBefore::new(
            TokenStreamPredicate::at_set(RECEIVER_TYPE_TERMINATORS),
            TokenStreamPredicate::Custom(|p: &mut KotlinParsing<'b>, top_level: bool| {
                if top_level && (p.at_set(&DEFINITELY_OUT_OF_RECEIVER_SET) || p.at(LPAR)) {
                    return true;
                }
                if top_level && p.at(IDENTIFIER) {
                    let lookahead = p.lookahead(1);
                    return lookahead != LT && lookahead != DOT && lookahead != SAFE_ACCESS && lookahead != QUEST;
                }
                false
            }),
        )
    }

    /// `parseFile()`: `kotlinFile : preamble toplevelObject* [eof]`.
    pub fn parse_file(&mut self) {
        // TODO(port): KotlinParsing.java:168
    }

    /// `checkUnclosedBlockComment()`.
    pub(crate) fn check_unclosed_block_comment(&mut self) {
        // TODO(port): KotlinParsing.java:181
    }

    /// `parseTypeCodeFragment()`.
    pub fn parse_type_code_fragment(&mut self) {
        // TODO(port): KotlinParsing.java:194
    }

    /// `parseExpressionCodeFragment()`.
    pub fn parse_expression_code_fragment(&mut self) {
        // TODO(port): KotlinParsing.java:203
    }

    /// `parseBlockCodeFragment()`.
    pub fn parse_block_code_fragment(&mut self) {
        // TODO(port): KotlinParsing.java:212
    }

    /// `parseLambdaExpression()`.
    pub fn parse_lambda_expression(&mut self) {
        // TODO(port): KotlinParsing.java:230
    }

    /// `parseBlockExpression()`.
    pub fn parse_block_expression(&mut self) {
        // TODO(port): KotlinParsing.java:234
    }

    /// `parseScript()`.
    pub fn parse_script(&mut self) {
        // TODO(port): KotlinParsing.java:238
    }

    /// `checkForUnexpectedSymbols()`.
    pub(crate) fn check_for_unexpected_symbols(&mut self) {
        // TODO(port): KotlinParsing.java:260
    }

    /// `parsePreamble()`: `fileAnnotationList? packageDirective? import*`.
    pub(crate) fn parse_preamble(&mut self) {
        // TODO(port): KotlinParsing.java:271
    }

    /// `parsePackageName()`: `SimpleName{"."}`.
    pub(crate) fn parse_package_name(&mut self) {
        // TODO(port): KotlinParsing.java:315
    }

    /// `parseImportDirective()`.
    pub(crate) fn parse_import_directive(&mut self) {
        // TODO(port): KotlinParsing.java:369
    }

    /// `closeImportWithErrorIfNewline(importDirective, importAlias, errorMessage)`.
    pub(crate) fn close_import_with_error_if_newline(
        &mut self,
        import_directive: Marker,
        import_alias: Option<Marker>,
        error_message: &'static str,
    ) -> bool {
        // TODO(port): KotlinParsing.java:444
        false
    }

    /// `parseImportDirectives()`.
    pub(crate) fn parse_import_directives(&mut self) {
        // TODO(port): KotlinParsing.java:459
    }

    /// `parseTopLevelDeclaration()`.
    pub(crate) fn parse_top_level_declaration(&mut self) {
        // TODO(port): KotlinParsing.java:482
    }

    /// `parseCommonDeclaration(detector, nameParsingModeForObject, declarationParsingMode)`.
    pub fn parse_common_declaration(
        &mut self,
        detector: &ModifierDetector,
        name_parsing_mode_for_object: NameParsingMode,
        declaration_parsing_mode: DeclarationParsingMode,
    ) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:514
        None
    }

    /// `parseModifierList(noModifiersBefore)`: `(modifier | annotation)*`.
    pub fn parse_modifier_list(&mut self, no_modifiers_before: &TokenSet) -> bool {
        // TODO(port): KotlinParsing.java:549
        false
    }

    /// `parseAnnotationsList(noModifiersBefore)`.
    pub fn parse_annotations_list(&mut self, no_modifiers_before: &TokenSet) {
        // TODO(port): KotlinParsing.java:553
    }

    /// `parseModifierList(tokenConsumer, noModifiersBefore, localDeclaration)`.
    pub fn parse_modifier_list_with_consumer(
        &mut self,
        token_consumer: Option<&mut ModifierDetector>,
        no_modifiers_before: &TokenSet,
        local_declaration: bool,
    ) -> bool {
        // TODO(port): KotlinParsing.java:566
        false
    }

    /// `parseFunctionTypeValueParameterModifierList()`.
    pub(crate) fn parse_function_type_value_parameter_modifier_list(&mut self) {
        // TODO(port): KotlinParsing.java:570
    }

    /// `parseTypeModifierList()`.
    pub(crate) fn parse_type_modifier_list(&mut self) {
        // TODO(port): KotlinParsing.java:574
    }

    /// `parseTypeArgumentModifierList()`.
    pub(crate) fn parse_type_argument_modifier_list(&mut self) {
        // TODO(port): KotlinParsing.java:578
    }

    /// `doParseModifierListBody(tokenConsumer, modifierKeywords, annotationParsingMode, noModifiersBefore,
    /// localDeclaration)`.
    pub(crate) fn do_parse_modifier_list_body(
        &mut self,
        token_consumer: Option<&mut ModifierDetector>,
        modifier_keywords: &TokenSet,
        annotation_parsing_mode: AnnotationParsingMode,
        no_modifiers_before: &TokenSet,
        local_declaration: bool,
    ) -> bool {
        // TODO(port): KotlinParsing.java:582
        true
    }

    /// `doParseModifierList(tokenConsumer, modifierKeywords, annotationParsingMode, noModifiersBefore,
    /// localDeclaration)`.
    pub(crate) fn do_parse_modifier_list(
        &mut self,
        token_consumer: Option<&mut ModifierDetector>,
        modifier_keywords: &TokenSet,
        annotation_parsing_mode: AnnotationParsingMode,
        no_modifiers_before: &TokenSet,
        local_declaration: bool,
    ) -> bool {
        // TODO(port): KotlinParsing.java:633
        false
    }

    /// `tryParseModifier(tokenConsumer, noModifiersBefore, modifierKeywords)`.
    pub(crate) fn try_parse_modifier(
        &mut self,
        token_consumer: Option<&mut ModifierDetector>,
        no_modifiers_before: &TokenSet,
        modifier_keywords: &TokenSet,
    ) -> bool {
        // TODO(port): KotlinParsing.java:659
        false
    }

    /// `parseContextParameterOrReceiverList(inFunctionType)`.
    pub(crate) fn parse_context_parameter_or_receiver_list(&mut self, in_function_type: bool) -> bool {
        // TODO(port): KotlinParsing.java:696
        false
    }

    /// `parseValueParameterOrTypeRef(inFunctionType)`.
    pub(crate) fn parse_value_parameter_or_type_ref(&mut self, in_function_type: bool) -> bool {
        // TODO(port): KotlinParsing.java:728
        false
    }

    /// `parseFileAnnotationList(mode)`.
    pub(crate) fn parse_file_annotation_list(&mut self, mode: AnnotationParsingMode) {
        // TODO(port): KotlinParsing.java:747
    }

    /// `parseAnnotations(mode)`.
    pub fn parse_annotations(&mut self, mode: AnnotationParsingMode) -> bool {
        // TODO(port): KotlinParsing.java:767
        false
    }

    /// `parseAnnotationOrList(mode)`.
    pub(crate) fn parse_annotation_or_list(&mut self, mode: AnnotationParsingMode) -> bool {
        // TODO(port): KotlinParsing.java:796
        false
    }

    /// `parseAnnotationList(mode)`.
    pub(crate) fn parse_annotation_list(&mut self, mode: AnnotationParsingMode) -> bool {
        // TODO(port): KotlinParsing.java:837
        false
    }

    /// `parseAnnotationTargetIfNeeded(mode)`.
    pub(crate) fn parse_annotation_target_if_needed(&mut self, mode: AnnotationParsingMode) -> bool {
        // TODO(port): KotlinParsing.java:879
        false
    }

    /// `parseAnnotationTarget(keyword)`.
    pub(crate) fn parse_annotation_target(&mut self, keyword: TokenKind) {
        // TODO(port): KotlinParsing.java:910
    }

    /// `generateAnnotationTargetErrorMessage(keyword)`.
    pub(crate) fn generate_annotation_target_error_message(keyword: TokenKind) -> String {
        // TODO(port): KotlinParsing.java:926
        String::new()
    }

    /// `parseAnnotation(mode)`.
    pub(crate) fn parse_annotation(&mut self, mode: AnnotationParsingMode) -> bool {
        // TODO(port): KotlinParsing.java:939
        false
    }

    /// `isNextRawTokenCommentOrWhitespace()`.
    pub(crate) fn is_next_raw_token_comment_or_whitespace(&mut self) -> bool {
        // TODO(port): KotlinParsing.java:993
        false
    }

    /// `parseClassOrObject(object, nameParsingMode, optionalBody, enumClass, expectKindKeyword)`.
    pub(crate) fn parse_class_or_object(
        &mut self,
        object: bool,
        name_parsing_mode: NameParsingMode,
        optional_body: bool,
        enum_class: bool,
        expect_kind_keyword: bool,
    ) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1021
        None
    }

    /// `parseClass(enumClass, expectKindKeyword)`.
    pub(crate) fn parse_class(&mut self, enum_class: bool, expect_kind_keyword: bool) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1125
        None
    }

    /// `parseObject(nameParsingMode, optionalBody)`.
    pub fn parse_object(&mut self, name_parsing_mode: NameParsingMode, optional_body: bool) {
        // TODO(port): KotlinParsing.java:1129
    }

    /// `parseEnumClassBody()`.
    pub(crate) fn parse_enum_class_body(&mut self) {
        // TODO(port): KotlinParsing.java:1138
    }

    /// `parseEnumEntries()`.
    pub(crate) fn parse_enum_entries(&mut self) -> bool {
        // TODO(port): KotlinParsing.java:1163
        false
    }

    /// `parseEnumEntry()`.
    pub(crate) fn parse_enum_entry(&mut self) -> ParseEnumEntryResult {
        // TODO(port): KotlinParsing.java:1198
        ParseEnumEntryResult::FAILED
    }

    /// `parseClassBody()`.
    pub(crate) fn parse_class_body(&mut self) {
        // TODO(port): KotlinParsing.java:1255
    }

    /// `parseMembers()`.
    pub(crate) fn parse_members(&mut self) {
        // TODO(port): KotlinParsing.java:1275
    }

    /// `parseMemberDeclaration()`.
    pub(crate) fn parse_member_declaration(&mut self) {
        // TODO(port): KotlinParsing.java:1298
    }

    /// `parseMemberDeclarationRest(modifierDetector)`.
    pub(crate) fn parse_member_declaration_rest(&mut self, modifier_detector: &ModifierDetector) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1321
        None
    }

    /// `parseCompanionBlock()`.
    pub(crate) fn parse_companion_block(&mut self) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1352
        None
    }

    /// `parseSecondaryConstructor()`.
    pub(crate) fn parse_secondary_constructor(&mut self) {
        // TODO(port): KotlinParsing.java:1364
    }

    /// `parseThisOrSuper()`.
    pub(crate) fn parse_this_or_super(&mut self) {
        // TODO(port): KotlinParsing.java:1418
    }

    /// `parseTypeAlias()`.
    pub(crate) fn parse_type_alias(&mut self) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1432
        None
    }

    /// `parseProperty(mode)`.
    pub fn parse_property(&mut self, mode: DeclarationParsingMode) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1487
        None
    }

    /// `parsePropertyDelegateOrAssignment()`.
    pub(crate) fn parse_property_delegate_or_assignment(&mut self) -> bool {
        // TODO(port): KotlinParsing.java:1580
        false
    }

    /// `parsePropertyDelegate()`.
    pub(crate) fn parse_property_delegate(&mut self) {
        // TODO(port): KotlinParsing.java:1599
    }

    /// `parseMultiDeclarationEntry(follow, recoverySet, mode)`.
    pub fn parse_multi_declaration_entry(
        &mut self,
        follow: &TokenSet,
        recovery_set: &TokenSet,
        mode: MultiDeclarationMode,
    ) {
        // TODO(port): KotlinParsing.java:1616
    }

    /// `parsePropertyComponent(notAllowedKind)`.
    pub(crate) fn parse_property_component(
        &mut self,
        not_allowed_kind: &mut PropertyComponentKindCollector,
    ) -> Option<PropertyComponentKind> {
        // TODO(port): KotlinParsing.java:1712
        None
    }

    /// `parseFunction()`.
    pub(crate) fn parse_function(&mut self) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1804
        None
    }

    /// `parseFunction(failIfIdentifierExists)`.
    pub fn parse_function_with(&mut self, fail_if_identifier_exists: bool) -> Option<NodeKind> {
        // TODO(port): KotlinParsing.java:1819
        None
    }

    /// `parseReceiverType(title, nameFollow)`.
    pub(crate) fn parse_receiver_type(&mut self, title: &'static str, name_follow: &TokenSet) -> bool {
        // TODO(port): KotlinParsing.java:1900
        false
    }

    /// `lastDotAfterReceiver()`.
    pub(crate) fn last_dot_after_receiver(&mut self) -> i32 {
        // TODO(port): KotlinParsing.java:1930
        -1
    }

    /// `parseFunctionOrPropertyName(receiverFound, title, nameFollow, recoverySet, nameRequired)`.
    pub(crate) fn parse_function_or_property_name(
        &mut self,
        receiver_found: bool,
        title: &'static str,
        name_follow: &TokenSet,
        recovery_set: &TokenSet,
        name_required: bool,
    ) {
        // TODO(port): KotlinParsing.java:1939
    }

    /// `parseFunctionBody()`.
    pub(crate) fn parse_function_body(&mut self) {
        // TODO(port): KotlinParsing.java:1958
    }

    /// `parseBlock()`.
    pub fn parse_block(&mut self) {
        // TODO(port): KotlinParsing.java:1977
    }

    /// `parseBlock(collapse)`.
    pub(crate) fn parse_block_with(&mut self, collapse: bool) {
        // TODO(port): KotlinParsing.java:1981
    }

    /// `advanceBalancedBlock()`.
    pub fn advance_balanced_block(&mut self) {
        // TODO(port): KotlinParsing.java:2007
    }

    /// `parseDelegationSpecifierList()`.
    pub(crate) fn parse_delegation_specifier_list(&mut self) {
        // TODO(port): KotlinParsing.java:2029
    }

    /// `parseDelegationSpecifier()`. Its `by` branch is
    /// `KotlinParsing::create_for_by_clause(self.my_builder.reborrow(), self.is_lazy).my_expression_parsing().parse_expression()`.
    pub(crate) fn parse_delegation_specifier(&mut self) {
        // TODO(port): KotlinParsing.java:2056
    }

    /// `parseTypeParameterList(recoverySet)`.
    pub(crate) fn parse_type_parameter_list(&mut self, recovery_set: &TokenSet) -> bool {
        // TODO(port): KotlinParsing.java:2083
        false
    }

    /// `parseTypeConstraintsGuarded(typeParameterListOccurred)`.
    pub(crate) fn parse_type_constraints_guarded(&mut self, type_parameter_list_occurred: bool) {
        // TODO(port): KotlinParsing.java:2116
    }

    /// `parseTypeConstraints()`.
    pub(crate) fn parse_type_constraints(&mut self) -> bool {
        // TODO(port): KotlinParsing.java:2122
        false
    }

    /// `parseTypeConstraintList()`.
    pub(crate) fn parse_type_constraint_list(&mut self) {
        // TODO(port): KotlinParsing.java:2133
    }

    /// `parseTypeConstraint()`.
    pub(crate) fn parse_type_constraint(&mut self) {
        // TODO(port): KotlinParsing.java:2155
    }

    /// `parseFunctionContract()`.
    pub(crate) fn parse_function_contract(&mut self) -> bool {
        // TODO(port): KotlinParsing.java:2175
        false
    }

    /// `parseTypeParameter()`.
    pub(crate) fn parse_type_parameter(&mut self) {
        // TODO(port): KotlinParsing.java:2188
    }

    /// `parseTypeRef()`.
    pub fn parse_type_ref(&mut self) {
        // TODO(port): KotlinParsing.java:2225
    }

    /// `parseTypeRefWithoutIntersections()`.
    pub fn parse_type_ref_without_intersections(&mut self) {
        // TODO(port): KotlinParsing.java:2229
    }

    /// `parseTypeRef(extraRecoverySet)`.
    pub fn parse_type_ref_with(&mut self, extra_recovery_set: &TokenSet) {
        // TODO(port): KotlinParsing.java:2233
    }

    /// `parseTypeRef(extraRecoverySet, allowSimpleIntersectionTypes)`.
    pub(crate) fn parse_type_ref_with_intersections(
        &mut self,
        extra_recovery_set: &TokenSet,
        allow_simple_intersection_types: bool,
    ) {
        // TODO(port): KotlinParsing.java:2237
    }

    /// `parseTypeRefContents(extraRecoverySet, allowSimpleIntersectionTypes)`.
    pub(crate) fn parse_type_ref_contents(
        &mut self,
        extra_recovery_set: &TokenSet,
        allow_simple_intersection_types: bool,
    ) -> Marker {
        // TODO(port): KotlinParsing.java:2244
        self.mark()
    }

    /// `parseNullableTypeSuffix(typeElementMarker)`.
    pub(crate) fn parse_nullable_type_suffix(&mut self, type_element_marker: Marker) -> Marker {
        // TODO(port): KotlinParsing.java:2362
        type_element_marker
    }

    /// `parseUserType()`.
    pub(crate) fn parse_user_type(&mut self) {
        // TODO(port): KotlinParsing.java:2383
    }

    /// `atParenthesizedMutableForPlatformTypes(offset)`.
    pub(crate) fn at_parenthesized_mutable_for_platform_types(&mut self, offset: i32) -> bool {
        // TODO(port): KotlinParsing.java:2429
        false
    }

    /// `recoverOnParenthesizedWordForPlatformTypes(offset, word, consume)`.
    pub(crate) fn recover_on_parenthesized_word_for_platform_types(
        &mut self,
        offset: i32,
        word: &'static str,
        consume: bool,
    ) -> bool {
        // TODO(port): KotlinParsing.java:2433
        false
    }

    /// `recoverOnPlatformTypeSuffix()`.
    pub(crate) fn recover_on_platform_type_suffix(&mut self) {
        // TODO(port): KotlinParsing.java:2463
    }

    /// `parseTypeArgumentList()`.
    pub(crate) fn parse_type_argument_list(&mut self) {
        // TODO(port): KotlinParsing.java:2475
    }

    /// `tryParseTypeArgumentList(extraRecoverySet)`.
    pub fn try_parse_type_argument_list(&mut self, extra_recovery_set: &TokenSet) -> bool {
        // TODO(port): KotlinParsing.java:2485
        false
    }

    /// `parseFunctionType(functionType)`.
    pub(crate) fn parse_function_type(&mut self, function_type: Marker) {
        // TODO(port): KotlinParsing.java:2528
    }

    /// `parseFunctionTypeContents(functionType)`.
    pub(crate) fn parse_function_type_contents(&mut self, function_type: Marker) -> Marker {
        // TODO(port): KotlinParsing.java:2532
        function_type
    }

    /// `parseValueParameterList(isFunctionTypeContents, typeRequired, recoverySet)`.
    pub(crate) fn parse_value_parameter_list(
        &mut self,
        is_function_type_contents: bool,
        type_required: bool,
        recovery_set: &TokenSet,
    ) {
        // TODO(port): KotlinParsing.java:2610
    }

    /// `valueParameterLoop(inFunctionTypeContext, recoverySet, parseParameter)`.
    pub(crate) fn value_parameter_loop(
        &mut self,
        in_function_type_context: bool,
        recovery_set: &TokenSet,
        parse_parameter: &mut dyn FnMut(&mut Self) -> bool,
    ) -> bool {
        // TODO(port): KotlinParsing.java:2643
        false
    }

    /// `tryParseValueParameter(typeRequired)`.
    pub(crate) fn try_parse_value_parameter(&mut self, type_required: bool) -> bool {
        // TODO(port): KotlinParsing.java:2685
        false
    }

    /// `parseValueParameter(typeRequired)`.
    pub fn parse_value_parameter(&mut self, type_required: bool) {
        // TODO(port): KotlinParsing.java:2689
    }

    /// `parseValueParameter(rollbackOnFailure, typeRequired)`.
    pub(crate) fn parse_value_parameter_with(&mut self, rollback_on_failure: bool, type_required: bool) -> bool {
        // TODO(port): KotlinParsing.java:2693
        false
    }

    /// `parseFunctionParameterRest(typeRequired)`.
    pub(crate) fn parse_function_parameter_rest(&mut self, type_required: bool) -> bool {
        // TODO(port): KotlinParsing.java:2716
        false
    }
}
