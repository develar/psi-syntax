//! The port of `KotlinParsing.java` (declarations, types, modifiers, annotations). It creates its
//! `KotlinExpressionParsing` and each calls the other, as in Java.
//!
//! Every Java method is one function here, in the Java order, with the Java logic and error messages (KotlinParsing.java
//! of Kotlin 2.4.21). Java `assert`s are `debug_assert!`s, as the Java ones are off in production.
//!
//! Naming of Java overloads: `parseModifierList(Consumer, TokenSet, boolean)` = `parse_modifier_list_with_consumer`,
//! `parseFunction(boolean)` = `parse_function_with`, `parseBlock(boolean)` = `parse_block_with`,
//! `parseTypeRef(TokenSet)` = `parse_type_ref_with`, `parseTypeRef(TokenSet, boolean)` =
//! `parse_type_ref_with_intersections`, `parseValueParameter(boolean, boolean)` = `parse_value_parameter_with`.
//! A Java `IElementType` result is `Option<NodeKind>` (`null` = `None`); a `Consumer<IElementType>` is
//! `Option<&mut ModifierDetector>`, the only consumer the parser passes; a `Supplier<Boolean>` is
//! `&mut dyn FnMut(&mut Self) -> bool`; `String` parameters that only ever receive literals are `&'static str`.

// The Java names of the nested enums' constants are kept.
#![allow(non_camel_case_types)]

use crate::builder::{
    Adapter, Binder, GREEDY_RIGHT_BINDER, Marker, PRECEDING_ALL_BINDER, SemanticWhitespaceAwarePsiBuilder,
    TRAILING_ALL_BINDER,
};
use crate::node_kind::NodeKind;
use crate::node_kind::NodeKind::*;
use crate::parsing::abstract_parsing::{AbstractKotlinParsing, OptionalMarker};
use crate::parsing::expression_parsing::KotlinExpressionParsing;
use crate::parsing::patterns::{FirstBefore, LastBefore, TokenStreamPattern, TokenStreamPredicate};
use crate::tokens::TokenKind::*;
use crate::tokens::{
    MODIFIER_KEYWORDS, RESERVED_VALUE_PARAMETER_MODIFIER_KEYWORDS, TYPE_ARGUMENT_MODIFIER_KEYWORDS,
    TYPE_MODIFIER_KEYWORDS, TokenKind, TokenSet, VAL_VAR, WHITE_SPACE_OR_COMMENT_BIT_SET,
};

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

    /*
     * [start] kotlinFile
     *   : preamble toplevelObject* [eof]
     *   ;
     */
    /// `parseFile()`.
    pub fn parse_file(&mut self) {
        let file_marker = self.mark();

        self.parse_preamble();

        while !self.eof() {
            self.parse_top_level_declaration();
        }

        self.check_unclosed_block_comment();
        file_marker.done(self.my_builder(), KT_FILE);
    }

    /// `checkUnclosedBlockComment()`. Offsets are bytes; a block comment starts with the two ASCII bytes `/*`, so its
    /// length in bytes exceeds 2 exactly when its length in Java chars does.
    pub(crate) fn check_unclosed_block_comment(&mut self) {
        if BLOCK_DOC_COMMENT_SET.contains(self.my_builder().raw_lookup(-1)) {
            let start_offset = self.my_builder().raw_token_type_start(-1);
            let end_offset = self.my_builder().raw_token_type_start(0);
            let closed = {
                let text = self.my_builder().get_original_text();
                let token_chars = match (usize::try_from(start_offset), usize::try_from(end_offset)) {
                    (Ok(start), Ok(end)) => text.get(start..end).unwrap_or(&[]),
                    _ => &[],
                };
                token_chars.len() > 2 && token_chars.ends_with(b"*/")
            };
            if !closed {
                let marker = self.my_builder().mark();
                marker.error(self.my_builder(), "Unclosed comment");
                marker.set_custom_edge_token_binders(self.my_builder(), Some(GREEDY_RIGHT_BINDER), None);
            }
        }
    }

    /// `parseTypeCodeFragment()`.
    pub fn parse_type_code_fragment(&mut self) {
        let marker = self.mark();
        self.parse_type_ref();

        self.check_for_unexpected_symbols();

        marker.done(self.my_builder(), TYPE_CODE_FRAGMENT);
    }

    /// `parseExpressionCodeFragment()`.
    pub fn parse_expression_code_fragment(&mut self) {
        let marker = self.mark();
        self.my_expression_parsing().parse_expression();

        self.check_for_unexpected_symbols();

        marker.done(self.my_builder(), EXPRESSION_CODE_FRAGMENT);
    }

    /// `parseBlockCodeFragment()`.
    pub fn parse_block_code_fragment(&mut self) {
        let marker = self.mark();
        let block_marker = self.mark();

        if self.at(PACKAGE_KEYWORD) || self.at(IMPORT_KEYWORD) {
            let err = self.mark();
            self.parse_preamble();
            err.error(self.my_builder(), "Package directive and imports are forbidden in code fragments");
        }

        self.my_expression_parsing().parse_statements();

        self.check_for_unexpected_symbols();

        block_marker.done(self.my_builder(), BLOCK);
        marker.done(self.my_builder(), BLOCK_CODE_FRAGMENT);
    }

    /// `parseLambdaExpression()`.
    pub fn parse_lambda_expression(&mut self) {
        self.my_expression_parsing()
            .parse_function_literal_with(/* preferBlock = */ false, /* collapse = */ false);
    }

    /// `parseBlockExpression()`.
    pub fn parse_block_expression(&mut self) {
        self.parse_block_with(/* collapse = */ false);
    }

    /// `parseScript()`.
    pub fn parse_script(&mut self) {
        let file_marker = self.mark();

        self.parse_preamble();

        let script_marker = self.mark();

        let block_marker = self.mark();

        self.my_expression_parsing().parse_statements_with(/*isScriptTopLevel = */ true);

        self.check_for_unexpected_symbols();

        block_marker.done(self.my_builder(), BLOCK);
        block_marker.set_custom_edge_token_binders(
            self.my_builder(),
            Some(PRECEDING_ALL_BINDER),
            Some(TRAILING_ALL_BINDER),
        );

        script_marker.done(self.my_builder(), SCRIPT);
        script_marker.set_custom_edge_token_binders(
            self.my_builder(),
            Some(PRECEDING_ALL_BINDER),
            Some(TRAILING_ALL_BINDER),
        );

        file_marker.done(self.my_builder(), KT_FILE);
    }

    /// `checkForUnexpectedSymbols()`.
    pub(crate) fn check_for_unexpected_symbols(&mut self) {
        while !self.eof() {
            self.error_and_advance("Unexpected symbol");
        }
    }

    /*
     *preamble
     *  : fileAnnotationList? packageDirective? import*
     *  ;
     */
    /// `parsePreamble()`.
    pub(crate) fn parse_preamble(&mut self) {
        let first_entry = self.mark();

        /*
         * fileAnnotationList
         *   : fileAnnotations*
         */
        self.parse_file_annotation_list(AnnotationParsingMode::FILE_ANNOTATIONS_BEFORE_PACKAGE);

        /*
         * packageDirective
         *   : modifiers "package" SimpleName{"."} SEMI?
         *   ;
         */
        let mut package_directive = self.mark();
        self.parse_modifier_list(&TokenSet::EMPTY);

        if self.at(PACKAGE_KEYWORD) {
            self.advance(); // PACKAGE_KEYWORD

            self.parse_package_name();

            first_entry.drop(self.my_builder());

            self.consume_if(SEMICOLON);

            package_directive.done(self.my_builder(), PACKAGE_DIRECTIVE);
        } else {
            // When package directive is omitted we should not report error on non-file annotations at the beginning of the file.
            // So, we rollback the parsing position and reparse file annotation list without report error on non-file annotations.
            first_entry.rollback_to(self.my_builder());

            self.parse_file_annotation_list(AnnotationParsingMode::FILE_ANNOTATIONS_WHEN_PACKAGE_OMITTED);
            package_directive = self.mark();
            package_directive.done(self.my_builder(), PACKAGE_DIRECTIVE);
            // Need to skip everything but shebang comment to allow comments at the start of the file to be bound to the first declaration.
            package_directive.set_custom_edge_token_binders(
                self.my_builder(),
                Some(Binder::BindFirstShebangWithWhitespaceOnly),
                None,
            );
        }

        self.parse_import_directives();
    }

    /* SimpleName{"."} */
    /// `parsePackageName()`.
    pub(crate) fn parse_package_name(&mut self) {
        let mut qualified_expression = self.mark();
        let mut simple_name = true;
        loop {
            if self.my_builder().newline_before_current_token() {
                self.error_with_recovery(
                    "Package name must be a '.'-separated identifier list placed on a single line",
                    Some(&PACKAGE_NAME_RECOVERY_SET),
                );
                break;
            }

            if self.at(DOT) {
                self.advance(); // DOT
                qualified_expression.error(self.my_builder(), "Package name must be a '.'-separated identifier list");
                qualified_expression = self.mark();
                continue;
            }

            let ns_name = self.mark();
            let simple_name_found = self.expect_with_recovery(
                IDENTIFIER,
                "Package name must be a '.'-separated identifier list",
                Some(&PACKAGE_NAME_RECOVERY_SET),
            );
            if simple_name_found {
                ns_name.done(self.my_builder(), REFERENCE_EXPRESSION);
            } else {
                ns_name.drop(self.my_builder());
            }

            if !simple_name {
                let preceding_marker = qualified_expression.precede(self.my_builder());
                qualified_expression.done(self.my_builder(), DOT_QUALIFIED_EXPRESSION);
                qualified_expression = preceding_marker;
            }

            if self.at(DOT) {
                self.advance(); // DOT

                if simple_name && !simple_name_found {
                    qualified_expression.drop(self.my_builder());
                    qualified_expression = self.mark();
                } else {
                    simple_name = false;
                }
            } else {
                break;
            }
        }
        qualified_expression.drop(self.my_builder());
    }

    /*
     * import
     *   : "import" SimpleName{"."} ("." "*" | "as" SimpleName)? SEMI?
     *   ;
     */
    /// `parseImportDirective()`.
    pub(crate) fn parse_import_directive(&mut self) {
        const EOL_OR_SEMICOLON_SET: TokenSet = TokenSet::create(&[EOL_OR_SEMICOLON]);

        debug_assert!(self._at(IMPORT_KEYWORD));
        let import_directive = self.mark();
        self.advance(); // IMPORT_KEYWORD

        if self.close_import_with_error_if_newline(import_directive, None, "Expecting qualified name") {
            return;
        }

        if !self.at(IDENTIFIER) {
            let error = self.mark();
            self.skip_until(&EOL_OR_SEMICOLON_SET);
            error.error(self.my_builder(), "Expecting qualified name");
            import_directive.done(self.my_builder(), IMPORT_DIRECTIVE);
            self.consume_if(SEMICOLON);
            return;
        }

        let mut qualified_name = self.mark();
        let mut reference = self.mark();
        self.advance(); // IDENTIFIER
        reference.done(self.my_builder(), REFERENCE_EXPRESSION);

        while self.at(DOT) && self.lookahead(1) != MUL {
            self.advance(); // DOT

            if self.close_import_with_error_if_newline(import_directive, None, "Import must be placed on a single line")
            {
                qualified_name.drop(self.my_builder());
                return;
            }

            reference = self.mark();
            if self.expect_with_recovery(
                IDENTIFIER,
                "Qualified name must be a '.'-separated identifier list",
                Some(&IMPORT_RECOVERY_SET),
            ) {
                reference.done(self.my_builder(), REFERENCE_EXPRESSION);
            } else {
                reference.drop(self.my_builder());
            }

            let precede = qualified_name.precede(self.my_builder());
            qualified_name.done(self.my_builder(), DOT_QUALIFIED_EXPRESSION);
            qualified_name = precede;
        }
        qualified_name.drop(self.my_builder());

        if self.at(DOT) {
            self.advance(); // DOT
            debug_assert!(self._at(MUL));
            self.advance(); // MUL
            if self.at(AS_KEYWORD) {
                let as_ = self.mark();
                self.advance(); // AS_KEYWORD
                if self.close_import_with_error_if_newline(import_directive, None, "Expecting identifier") {
                    as_.drop(self.my_builder());
                    return;
                }
                self.consume_if(IDENTIFIER);
                as_.done(self.my_builder(), IMPORT_ALIAS);
                as_.precede(self.my_builder())
                    .error(self.my_builder(), "Cannot rename all imported items to one identifier");
            }
        }
        if self.at(AS_KEYWORD) {
            let alias = self.mark();
            self.advance(); // AS_KEYWORD
            if self.close_import_with_error_if_newline(import_directive, Some(alias), "Expecting identifier") {
                return;
            }
            self.expect_with_recovery(IDENTIFIER, "Expecting identifier", Some(&SEMICOLON_SET));
            alias.done(self.my_builder(), IMPORT_ALIAS);
        }
        self.consume_if(SEMICOLON);
        import_directive.done(self.my_builder(), IMPORT_DIRECTIVE);
        import_directive.set_custom_edge_token_binders(self.my_builder(), None, Some(Binder::TrailingComments));
    }

    /// `closeImportWithErrorIfNewline(importDirective, importAlias, errorMessage)`.
    pub(crate) fn close_import_with_error_if_newline(
        &mut self,
        import_directive: Marker,
        import_alias: Option<Marker>,
        error_message: &'static str,
    ) -> bool {
        if self.my_builder().newline_before_current_token() {
            if let Some(import_alias) = import_alias {
                import_alias.done(self.my_builder(), IMPORT_ALIAS);
            }
            self.error(error_message);
            import_directive.done(self.my_builder(), IMPORT_DIRECTIVE);
            return true;
        }
        false
    }

    /// `parseImportDirectives()`.
    pub(crate) fn parse_import_directives(&mut self) {
        let import_list = self.mark();
        if !self.at(IMPORT_KEYWORD) {
            // this is necessary to allow comments at the start of the file to be bound to the first declaration
            import_list.set_custom_edge_token_binders(self.my_builder(), Some(Binder::DoNotBindAnything), None);
        }
        while self.at(IMPORT_KEYWORD) {
            self.parse_import_directive();
        }
        import_list.done(self.my_builder(), IMPORT_LIST);
    }

    /*
     * toplevelObject
     *   : package
     *   : class
     *   : extension
     *   : function
     *   : property
     *   : typeAlias
     *   : object
     *   ;
     */
    /// `parseTopLevelDeclaration()`.
    pub(crate) fn parse_top_level_declaration(&mut self) {
        if self.at(SEMICOLON) {
            self.advance(); // SEMICOLON
            return;
        }
        let decl = self.mark();

        let mut detector = ModifierDetector::new();
        self.parse_modifier_list_with_consumer(
            Some(&mut detector),
            &TokenSet::EMPTY,
            /* localDeclaration = */ false,
        );

        let mut decl_type = self.parse_common_declaration(
            &detector,
            NameParsingMode::REQUIRED,
            DeclarationParsingMode::MEMBER_OR_TOPLEVEL,
        );

        if decl_type.is_none() && self.at(LBRACE) {
            self.error("Expecting a top level declaration");
            self.parse_block();
            decl_type = Some(FUN);
        }

        match decl_type {
            None if self.at(IMPORT_KEYWORD) => {
                self.error("imports are only allowed in the beginning of file");
                self.parse_import_directives();
                decl.drop(self.my_builder());
            }
            None => {
                self.error_and_advance("Expecting a top level declaration");
                decl.drop(self.my_builder());
            }
            Some(decl_type) => {
                self.close_declaration_with_comment_binders(decl, decl_type, true);
            }
        }
    }

    /// `parseCommonDeclaration(detector, nameParsingModeForObject, declarationParsingMode)`. The Java switch over
    /// `getTokenId()` is a match over `tt()`: a token without a `KtTokens` id matches no case either way.
    pub fn parse_common_declaration(
        &mut self,
        detector: &ModifierDetector,
        name_parsing_mode_for_object: NameParsingMode,
        declaration_parsing_mode: DeclarationParsingMode,
    ) -> Option<NodeKind> {
        match self.tt() {
            Some(CLASS_KEYWORD | INTERFACE_KEYWORD) => {
                return self.parse_class(detector.is_enum_detected(), true);
            }
            Some(FUN_KEYWORD) => return self.parse_function(),
            Some(VAL_KEYWORD | VAR_KEYWORD) => return self.parse_property(declaration_parsing_mode),
            Some(LPAR | LBRACKET) => {
                let lookahead = self.lookahead(1);
                return if lookahead == VAL_KEYWORD || lookahead == VAR_KEYWORD {
                    self.parse_property(declaration_parsing_mode)
                } else {
                    None
                };
            }
            Some(TYPE_ALIAS_KEYWORD) => return self.parse_type_alias(),
            Some(OBJECT_KEYWORD) => {
                self.parse_object(name_parsing_mode_for_object, true);
                return Some(OBJECT_DECLARATION);
            }
            Some(IDENTIFIER)
                if detector.is_enum_detected() && declaration_parsing_mode.can_be_enum_used_as_soft_keyword() =>
            {
                return self.parse_class(true, false);
            }
            _ => {}
        }

        None
    }

    /*
     * (modifier | annotation)*
     */
    /// `parseModifierList(noModifiersBefore)`.
    pub fn parse_modifier_list(&mut self, no_modifiers_before: &TokenSet) -> bool {
        self.parse_modifier_list_with_consumer(None, no_modifiers_before, /* localDeclaration = */ false)
    }

    /// `parseAnnotationsList(noModifiersBefore)`.
    pub fn parse_annotations_list(&mut self, no_modifiers_before: &TokenSet) {
        self.do_parse_modifier_list(
            None,
            &TokenSet::EMPTY,
            AnnotationParsingMode::DEFAULT,
            no_modifiers_before,
            /* localDeclaration = */ false,
        );
    }

    /// `parseModifierList(tokenConsumer, noModifiersBefore, localDeclaration)`: `(modifier | annotation)*`.
    ///
    /// Feeds modifiers (not annotations) into the passed consumer, if it is not null.
    ///
    /// `no_modifiers_before` is a token set with elements indicating when met them that previous token must be parsed
    /// as an identifier rather than modifier; `local_declaration` is `true` if we are trying to parse a local
    /// declaration.
    pub fn parse_modifier_list_with_consumer(
        &mut self,
        token_consumer: Option<&mut ModifierDetector>,
        no_modifiers_before: &TokenSet,
        local_declaration: bool,
    ) -> bool {
        self.do_parse_modifier_list(
            token_consumer,
            &MODIFIER_KEYWORDS,
            AnnotationParsingMode::DEFAULT,
            no_modifiers_before,
            local_declaration,
        )
    }

    /// `parseFunctionTypeValueParameterModifierList()`.
    pub(crate) fn parse_function_type_value_parameter_modifier_list(&mut self) {
        self.do_parse_modifier_list(
            None,
            &RESERVED_VALUE_PARAMETER_MODIFIER_KEYWORDS,
            AnnotationParsingMode::NO_ANNOTATIONS_NO_CONTEXT,
            &NO_MODIFIER_BEFORE_FOR_VALUE_PARAMETER,
            /* localDeclaration = */ false,
        );
    }

    /// `parseTypeModifierList()`.
    pub(crate) fn parse_type_modifier_list(&mut self) {
        self.do_parse_modifier_list(
            None,
            &TYPE_MODIFIER_KEYWORDS,
            AnnotationParsingMode::TYPE_CONTEXT,
            &TokenSet::EMPTY,
            /* localDeclaration = */ false,
        );
    }

    /// `parseTypeArgumentModifierList()`.
    pub(crate) fn parse_type_argument_modifier_list(&mut self) {
        self.do_parse_modifier_list(
            None,
            &TYPE_ARGUMENT_MODIFIER_KEYWORDS,
            AnnotationParsingMode::NO_ANNOTATIONS_NO_CONTEXT,
            &COMMA_COLON_GT_SET,
            /* localDeclaration = */ false,
        );
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
        let mut token_consumer = token_consumer;
        let mut empty = true;
        while !self.eof() {
            if self.at(AT) && annotation_parsing_mode.allow_annotations() {
                let before_annotation_marker = self.mark();

                let is_annotation_parsed = self.parse_annotation_or_list(annotation_parsing_mode);

                if !is_annotation_parsed && !annotation_parsing_mode.with_significant_whitespace_before_arguments() {
                    before_annotation_marker.rollback_to(self.my_builder());
                    // try parse again, but with significant whitespace
                    let new_mode = if annotation_parsing_mode.allow_context_list() {
                        AnnotationParsingMode::WITH_SIGNIFICANT_WHITESPACE_BEFORE_ARGUMENTS
                    } else {
                        AnnotationParsingMode::WITH_SIGNIFICANT_WHITESPACE_BEFORE_ARGUMENTS_NO_CONTEXT
                    };
                    self.do_parse_modifier_list_body(
                        token_consumer.as_deref_mut(),
                        modifier_keywords,
                        new_mode,
                        no_modifiers_before,
                        local_declaration,
                    );
                    empty = false;
                    break;
                } else {
                    before_annotation_marker.drop(self.my_builder());
                }
            } else if self.at(CONTEXT_KEYWORD)
                && annotation_parsing_mode.allow_context_list()
                && self.lookahead(1) == LPAR
            {
                let context_marker = self.mark();
                if !self.parse_context_parameter_or_receiver_list(false) && local_declaration {
                    // Rollback the entire context declaration to make it possible to prevent parsing of potential local declarations
                    // that in fact are not declarations (we are trying to parse declarations at first and statements as second).
                    context_marker.rollback_to(self.my_builder());
                    break;
                } else {
                    context_marker.drop(self.my_builder());
                }
            } else if self.try_parse_modifier(token_consumer.as_deref_mut(), no_modifiers_before, modifier_keywords) {
                // modifier advanced
            } else {
                break;
            }
            empty = false;
        }

        empty
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
        let list = self.mark();

        let empty = self.do_parse_modifier_list_body(
            token_consumer,
            modifier_keywords,
            annotation_parsing_mode,
            no_modifiers_before,
            local_declaration,
        );

        if empty {
            list.drop(self.my_builder());
        } else {
            list.done(self.my_builder(), MODIFIER_LIST);
        }
        !empty
    }

    /// `tryParseModifier(tokenConsumer, noModifiersBefore, modifierKeywords)`.
    pub(crate) fn try_parse_modifier(
        &mut self,
        token_consumer: Option<&mut ModifierDetector>,
        no_modifiers_before: &TokenSet,
        modifier_keywords: &TokenSet,
    ) -> bool {
        let marker = self.mark();

        if self.at_set(modifier_keywords) {
            let lookahead = self.lookahead(1);

            if self.at(FUN_KEYWORD) && lookahead != INTERFACE_KEYWORD {
                marker.rollback_to(self.my_builder());
                return false;
            }

            // `tt` is never null here: `atSet` matched a set of keywords.
            if lookahead.is_some()
                && !no_modifiers_before.contains(lookahead)
                && let Some(tt) = self.tt()
            {
                if let Some(token_consumer) = token_consumer {
                    token_consumer.consume(tt);
                }
                self.advance(); // MODIFIER
                marker.collapse(self.my_builder(), tt);
                return true;
            }
        }

        marker.rollback_to(self.my_builder());
        false
    }

    /// `parseContextParameterOrReceiverList(inFunctionType)`:
    ///
    /// ```text
    /// contextReceiverList
    ///   : "context" "(" (contextReceiver{","})+ ")"
    /// ```
    ///
    /// Returns `true` if it parsed a context with value parameters, otherwise `false` if it parsed a context with type
    /// refs (that work as receivers) or encountered a syntax error during parsing.
    pub(crate) fn parse_context_parameter_or_receiver_list(&mut self, in_function_type: bool) -> bool {
        debug_assert!(self._at(CONTEXT_KEYWORD));
        let value_parameter_or_type_ref_list = self.mark();
        self.advance(); // CONTEXT_KEYWORD
        debug_assert!(self._at(LPAR));

        let no_error = if self.lookahead(1) == RPAR {
            self.advance(); // LPAR
            self.error("Empty context parameter list");
            self.advance(); // RPAR
            false
        } else {
            // Treat parsing of context receivers (deprecated syntax) as an error,
            // But an outer caller decides if the entire list should be dropped:
            // If we're trying to parse a local declaration, we should drop it to prevent unexpected parsing of ahead declarations
            self.value_parameter_loop(in_function_type, &CONTEXT_PARAMETERS_FOLLOW_SET, &mut |p| {
                p.parse_value_parameter_or_type_ref(in_function_type)
            })
        };

        value_parameter_or_type_ref_list.done(self.my_builder(), CONTEXT_PARAMETER_LIST);
        no_error
    }

    /// `parseValueParameterOrTypeRef(inFunctionType)`:
    ///
    /// ```text
    /// contextReceiver
    ///   : label? typeReference
    /// ```
    ///
    /// Returns `true` if it parsed a value parameter or type ref in the correct position (in function type) and
    /// `false` otherwise.
    pub(crate) fn parse_value_parameter_or_type_ref(&mut self, in_function_type: bool) -> bool {
        if self.try_parse_value_parameter(true) {
            return true;
        }

        let context_receiver = self.mark();
        if !in_function_type && self.my_expression_parsing().is_at_label_definition_or_missing_identifier() {
            self.my_expression_parsing().parse_label_definition();
        }
        self.parse_type_ref();
        context_receiver.done(self.my_builder(), CONTEXT_RECEIVER);
        in_function_type
    }

    /*
     * fileAnnotationList
     *   : ("[" "file:" annotationEntry+ "]")*
     *   ;
     */
    /// `parseFileAnnotationList(mode)`. Java's `LOG.error` on a non-file mode is a debug assertion: every caller
    /// passes a file mode.
    pub(crate) fn parse_file_annotation_list(&mut self, mode: AnnotationParsingMode) {
        debug_assert!(mode.is_file_annotation_parsing_mode(), "expected file annotation parsing mode, but:{mode:?}");

        let file_annotations_list = self.mark();

        if self.parse_annotations(mode) {
            file_annotations_list.done(self.my_builder(), FILE_ANNOTATION_LIST);
        } else {
            file_annotations_list.drop(self.my_builder());
        }
    }

    /*
     * annotations
     *   : (annotation | annotationList)*
     *   ;
     */
    /// `parseAnnotations(mode)`.
    pub fn parse_annotations(&mut self, mode: AnnotationParsingMode) -> bool {
        if !self.parse_annotation_or_list(mode) {
            return false;
        }

        while self.parse_annotation_or_list(mode) {
            // do nothing
        }

        true
    }

    /*
     * annotation
     *   : "@" (annotationUseSiteTarget ":")? unescapedAnnotation
     *   ;
     *
     * annotationList
     *   : "@" (annotationUseSiteTarget ":")? "[" unescapedAnnotation+ "]"
     *   ;
     *
     * annotationUseSiteTarget
     *   : "file"
     *   : "field"
     *   : "property"
     *   : "get"
     *   : "set"
     *   : "param"
     *   : "setparam"
     *   ;
     */
    /// `parseAnnotationOrList(mode)`.
    pub(crate) fn parse_annotation_or_list(&mut self, mode: AnnotationParsingMode) -> bool {
        if self.at(AT) {
            let next_raw_token = self.my_builder().raw_lookup(1);
            let mut token_to_match = next_raw_token;
            let mut is_targeted_annotation = false;

            if (next_raw_token == IDENTIFIER || ANNOTATION_TARGETS.contains(next_raw_token))
                && self.lookahead(2) == COLON
            {
                token_to_match = self.lookahead(3);
                is_targeted_annotation = true;
            } else if self.lookahead(1) == COLON {
                // recovery for "@:ann"
                is_targeted_annotation = true;
                token_to_match = self.lookahead(2);
            }

            if token_to_match == IDENTIFIER {
                return self.parse_annotation(mode);
            } else if token_to_match == LBRACKET {
                return self.parse_annotation_list(mode);
            } else if is_targeted_annotation {
                if self.lookahead(1) == COLON {
                    self.error_and_advance_n("Expected annotation identifier after ':'", 2); // AT, COLON
                } else {
                    self.error_and_advance_n("Expected annotation identifier after ':'", 3); // AT, (ANNOTATION TARGET KEYWORD), COLON
                }
            } else {
                self.error_and_advance_n("Expected annotation identifier after '@'", 1); // AT
            }
            return true;
        }

        false
    }

    /// `parseAnnotationList(mode)`.
    pub(crate) fn parse_annotation_list(&mut self, mode: AnnotationParsingMode) -> bool {
        debug_assert!(self._at(AT));
        let annotation = self.mark();

        self.my_builder().disable_newlines();

        self.advance(); // AT

        if !self.parse_annotation_target_if_needed(mode) {
            annotation.rollback_to(self.my_builder());
            self.my_builder().restore_newlines_state();
            return false;
        }

        debug_assert!(self._at(LBRACKET));
        self.advance(); // LBRACKET

        if !self.at(IDENTIFIER) && !self.at(AT) {
            self.error("Expecting a list of annotations");
        } else {
            while self.at(IDENTIFIER) || self.at(AT) {
                if self.at(AT) {
                    self.error_and_advance("No '@' needed in annotation list"); // AT
                    continue;
                }

                self.parse_annotation(AnnotationParsingMode::DEFAULT);
                while self.at(COMMA) {
                    self.error_and_advance("No commas needed to separate annotations");
                }
            }
        }

        self.expect_with(RBRACKET, "Expecting ']' to close the annotation list");
        self.my_builder().restore_newlines_state();

        annotation.done(self.my_builder(), ANNOTATION);
        true
    }

    /// `parseAnnotationTargetIfNeeded(mode)`: returns true if we should continue parse annotation.
    pub(crate) fn parse_annotation_target_if_needed(&mut self, mode: AnnotationParsingMode) -> bool {
        let expected_annotation_target_before_colon = "Expected annotation target before ':'";

        if self.at(COLON) {
            // recovery for "@:ann"
            self.error_and_advance(expected_annotation_target_before_colon); // COLON
            return true;
        }

        let target_keyword = if self.at_set(&ANNOTATION_TARGETS) { self.my_builder().get_token_type() } else { None };
        if mode == AnnotationParsingMode::FILE_ANNOTATIONS_WHEN_PACKAGE_OMITTED
            && !(target_keyword == Some(FILE_KEYWORD) && self.lookahead(1) == COLON)
        {
            return false;
        }

        if self.lookahead(1) == COLON && target_keyword.is_none() && self.at(IDENTIFIER) {
            // recovery for "@fil:ann"
            self.error_and_advance(expected_annotation_target_before_colon); // IDENTIFIER
            self.advance(); // COLON
            return true;
        }

        if target_keyword.is_none() && mode.is_file_annotation_parsing_mode() {
            self.parse_annotation_target(FILE_KEYWORD);
        } else if let Some(target_keyword) = target_keyword {
            self.parse_annotation_target(target_keyword);
        }

        true
    }

    /// `parseAnnotationTarget(keyword)`.
    pub(crate) fn parse_annotation_target(&mut self, keyword: TokenKind) {
        let marker = self.mark();

        if !self.expect(keyword) {
            self.error(Self::generate_annotation_target_error_message(keyword));
            marker.drop(self.my_builder());
        } else {
            marker.done(self.my_builder(), ANNOTATION_TARGET);
        }

        if !self.expect(COLON) {
            self.error_with_recovery(
                Self::generate_annotation_target_error_message(keyword),
                Some(&IDENTIFIER_RBRACKET_LBRACKET_SET),
            );
        }
    }

    /// `generateAnnotationTargetErrorMessage(keyword)`: a keyword token's `toString()` is its debug name.
    pub(crate) fn generate_annotation_target_error_message(keyword: TokenKind) -> String {
        format!(
            "Expecting \"{}{}\" prefix for {} annotations",
            keyword.debug_name(),
            COLON.value().unwrap_or(":"),
            keyword.debug_name()
        )
    }

    /*
     * annotation
     *   : "@" (annotationUseSiteTarget ":")? unescapedAnnotation
     *   ;
     *
     * unescapedAnnotation
     *   : SimpleName{"."} typeArguments? valueArguments?
     *   ;
     */
    /// `parseAnnotation(mode)`.
    pub(crate) fn parse_annotation(&mut self, mode: AnnotationParsingMode) -> bool {
        debug_assert!(
            self._at(IDENTIFIER) ||
                // We have "@ann" or "@:ann" or "@ :ann", but not "@ ann"
                // (it's guaranteed that call sites do not allow the latter case)
                (self._at(AT) && (!self.is_next_raw_token_comment_or_whitespace() || self.lookahead(1) == COLON)),
            "Invalid annotation prefix"
        );

        let annotation = self.mark();

        let at_at = self.at(AT);
        if at_at {
            self.advance(); // AT
        }

        if at_at && !self.parse_annotation_target_if_needed(mode) {
            annotation.rollback_to(self.my_builder());
            return false;
        }

        let reference = self.mark();
        let type_reference = self.mark();
        self.parse_user_type();
        type_reference.done(self.my_builder(), TYPE_REFERENCE);
        reference.done(self.my_builder(), CONSTRUCTOR_CALLEE);

        self.parse_type_argument_list();

        if self.at(LPAR)
            && !VAL_VAR.contains(self.lookahead(1))
            && !(WHITE_SPACE_OR_COMMENT_BIT_SET.contains(self.my_builder().raw_lookup(-1))
                && mode.with_significant_whitespace_before_arguments())
        {
            self.my_expression_parsing().parse_value_argument_list();

            /*
             * There are two problem cases relating to parsing of annotations on a functional type:
             *  - Annotation on a functional type was parsed correctly with the capture parentheses of the functional type,
             *      e.g. @Anno () -> Unit
             *                    ^ Parse error only here: Type expected
             *  - It wasn't parsed, e.g. @Anno (x: kotlin.Any) -> Unit
             *                                           ^ Parse error: Expecting ')'
             *
             * In both cases, parser should rollback to start parsing of annotation and tries parse it with significant whitespace.
             * A marker is set here which means that we must to rollback.
             */
            if mode.type_context() && (self.get_last_token() != RPAR || self.at(ARROW)) {
                annotation.done(self.my_builder(), ANNOTATION_ENTRY);
                return false;
            }
        }
        annotation.done(self.my_builder(), ANNOTATION_ENTRY);

        true
    }

    /// `isNextRawTokenCommentOrWhitespace()`.
    pub(crate) fn is_next_raw_token_comment_or_whitespace(&mut self) -> bool {
        WHITE_SPACE_OR_COMMENT_BIT_SET.contains(self.my_builder().raw_lookup(1))
    }

    /*
     * class
     *   : modifiers ("class" | "interface") SimpleName
     *       typeParameters?
     *       primaryConstructor?
     *       (":" annotations delegationSpecifier{","})?
     *       typeConstraints
     *       (classBody? | enumClassBody)
     *   ;
     *
     * primaryConstructor
     *   : (modifiers "constructor")? ("(" functionParameter{","} ")")
     *   ;
     *
     * object
     *   : "object" SimpleName? primaryConstructor? ":" delegationSpecifier{","}? classBody?
     *   ;
     */
    /// `parseClassOrObject(object, nameParsingMode, optionalBody, enumClass, expectKindKeyword)`.
    pub(crate) fn parse_class_or_object(
        &mut self,
        object: bool,
        name_parsing_mode: NameParsingMode,
        optional_body: bool,
        enum_class: bool,
        expect_kind_keyword: bool,
    ) -> Option<NodeKind> {
        if expect_kind_keyword {
            if object {
                debug_assert!(self._at(OBJECT_KEYWORD));
            } else {
                debug_assert!(self._at_set(&CLASS_INTERFACE_SET));
            }
            self.advance(); // CLASS_KEYWORD, INTERFACE_KEYWORD or OBJECT_KEYWORD
        } else {
            debug_assert!(
                enum_class,
                "Currently classifiers without class/interface/object are only allowed for enums"
            );
            self.error("'class' keyword is expected after 'enum'");
        }

        if name_parsing_mode == NameParsingMode::REQUIRED {
            self.expect_with_recovery(IDENTIFIER, "Name expected", Some(&CLASS_NAME_RECOVERY_SET));
        } else {
            debug_assert!(object, "Must be an object to be nameless");
            if self.at(IDENTIFIER) {
                if name_parsing_mode == NameParsingMode::PROHIBITED {
                    self.error_and_advance("An object expression cannot bind a name");
                } else {
                    debug_assert!(name_parsing_mode == NameParsingMode::ALLOWED);
                    self.advance();
                }
            }
        }

        let type_parameters_declared = self.parse_type_parameter_list(&TYPE_PARAMETER_GT_RECOVERY_SET);

        let before_constructor_modifiers = self.mark();
        let primary_constructor_marker = self.mark();
        let has_constructor_modifiers = self.parse_modifier_list(&TokenSet::EMPTY);

        // Some modifiers found, but no parentheses following: class has already ended, and we are looking at something else
        if has_constructor_modifiers && !self.at_set(&LPAR_LBRACE_COLON_CONSTRUCTOR_KEYWORD_SET) {
            before_constructor_modifiers.rollback_to(self.my_builder());
            return Some(if object { OBJECT_DECLARATION } else { CLASS });
        }

        // We are still inside a class declaration
        before_constructor_modifiers.drop(self.my_builder());

        let has_constructor_keyword = self.at(CONSTRUCTOR_KEYWORD);
        if has_constructor_keyword {
            self.advance(); // CONSTRUCTOR_KEYWORD
        }

        if self.at(LPAR) {
            self.parse_value_parameter_list(false, /* typeRequired  = */ true, &LBRACE_RBRACE_SET);
            primary_constructor_marker.done(self.my_builder(), PRIMARY_CONSTRUCTOR);
        } else if has_constructor_modifiers || has_constructor_keyword {
            // A comprehensive error message for cases like:
            //    class A private : Foo
            // or
            //    class A private {
            primary_constructor_marker.done(self.my_builder(), PRIMARY_CONSTRUCTOR);
            if has_constructor_keyword {
                self.error("Expecting primary constructor parameter list");
            } else {
                self.error("Expecting 'constructor' keyword");
            }
        } else {
            primary_constructor_marker.drop(self.my_builder());
        }

        if self.at(COLON) {
            self.advance(); // COLON
            self.parse_delegation_specifier_list();
        }

        let where_marker = OptionalMarker::new(self, object);
        self.parse_type_constraints_guarded(type_parameters_declared);
        where_marker.error(self, "Where clause is not allowed for objects");

        if self.at(LBRACE) {
            if enum_class {
                self.parse_enum_class_body();
            } else {
                self.parse_class_body();
            }
        } else if !optional_body {
            let fake_body = self.mark();
            self.error("Expecting a class body");
            fake_body.done(self.my_builder(), CLASS_BODY);
        }

        Some(if object { OBJECT_DECLARATION } else { CLASS })
    }

    /// `parseClass(enumClass, expectKindKeyword)`.
    pub(crate) fn parse_class(&mut self, enum_class: bool, expect_kind_keyword: bool) -> Option<NodeKind> {
        self.parse_class_or_object(false, NameParsingMode::REQUIRED, true, enum_class, expect_kind_keyword)
    }

    /// `parseObject(nameParsingMode, optionalBody)`.
    pub fn parse_object(&mut self, name_parsing_mode: NameParsingMode, optional_body: bool) {
        self.parse_class_or_object(true, name_parsing_mode, optional_body, false, true);
    }

    /*
     * enumClassBody
     *   : "{" enumEntries (";" members)? "}"
     *   ;
     */
    /// `parseEnumClassBody()`.
    pub(crate) fn parse_enum_class_body(&mut self) {
        if !self.at(LBRACE) {
            return;
        }

        let body = self.mark();
        self.my_builder().enable_newlines();

        self.advance(); // LBRACE

        if !self.parse_enum_entries() && !self.at(RBRACE) {
            self.error("Expecting ';' after the last enum entry or '}' to close enum class body");
        }
        self.parse_members();
        self.expect_with(RBRACE, "Expecting '}' to close enum class body");

        self.my_builder().restore_newlines_state();
        body.done(self.my_builder(), CLASS_BODY);
    }

    /// `parseEnumEntries()`:
    ///
    /// ```text
    /// enumEntries
    ///   : enumEntry{","}?
    ///   ;
    /// ```
    ///
    /// Returns true if enum regular members can follow, false otherwise.
    pub(crate) fn parse_enum_entries(&mut self) -> bool {
        while !self.eof() && !self.at(RBRACE) {
            match self.parse_enum_entry() {
                ParseEnumEntryResult::FAILED => {
                    // Special case without any enum entries but with possible members after semicolon
                    if self.at(SEMICOLON) {
                        self.advance();
                        return true;
                    } else {
                        return false;
                    }
                }
                ParseEnumEntryResult::NO_DELIMITER => return false,
                ParseEnumEntryResult::COMMA_DELIMITER => {}
                ParseEnumEntryResult::SEMICOLON_DELIMITER => return true,
            }
        }
        false
    }

    /*
     * enumEntry
     *   : modifiers SimpleName ("(" arguments ")")? classBody?
     *   ;
     */
    /// `parseEnumEntry()`.
    pub(crate) fn parse_enum_entry(&mut self) -> ParseEnumEntryResult {
        let entry = self.mark();

        self.parse_modifier_list(&COMMA_SEMICOLON_RBRACE_SET);

        if !self.at_set(&SOFT_KEYWORDS_AT_MEMBER_START) && self.at(IDENTIFIER) {
            self.advance(); // IDENTIFIER

            if self.at(LPAR) {
                // Arguments should be parsed here
                // Also, "fake" constructor call tree is created,
                // with empty type name inside
                let initializer_list = self.mark();
                let delegator_super_call = self.mark();

                let callee = self.mark();
                let type_reference = self.mark();
                let type_ = self.mark();
                let reference_expr = self.mark();
                reference_expr.done(self.my_builder(), ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION);
                type_.done(self.my_builder(), USER_TYPE);
                type_reference.done(self.my_builder(), TYPE_REFERENCE);
                callee.done(self.my_builder(), CONSTRUCTOR_CALLEE);

                self.my_expression_parsing().parse_value_argument_list();
                delegator_super_call.done(self.my_builder(), SUPER_TYPE_CALL_ENTRY);
                initializer_list.done(self.my_builder(), INITIALIZER_LIST);
            }
            if self.at(LBRACE) {
                self.parse_class_body();
            }
            let comma_found = self.at(COMMA);
            if comma_found {
                self.advance();
            }
            let semicolon_found = self.at(SEMICOLON);
            if semicolon_found {
                self.advance();
            }

            // Probably some helper function
            self.close_declaration_with_comment_binders(entry, ENUM_ENTRY, true);
            if semicolon_found {
                ParseEnumEntryResult::SEMICOLON_DELIMITER
            } else if comma_found {
                ParseEnumEntryResult::COMMA_DELIMITER
            } else {
                ParseEnumEntryResult::NO_DELIMITER
            }
        } else {
            entry.rollback_to(self.my_builder());
            ParseEnumEntryResult::FAILED
        }
    }

    /*
     * classBody
     *   : ("{" members "}")?
     *   ;
     */
    /// `parseClassBody()`.
    pub(crate) fn parse_class_body(&mut self) {
        let body = self.mark();

        self.my_builder().enable_newlines();

        if self.expect_with(LBRACE, "Expecting a class body") {
            self.parse_members();
            self.expect_with(RBRACE, "Missing '}");
        }

        self.my_builder().restore_newlines_state();

        body.done(self.my_builder(), CLASS_BODY);
    }

    /// `parseMembers()`:
    ///
    /// ```text
    /// members
    ///   : memberDeclaration*
    ///   ;
    /// ```
    pub(crate) fn parse_members(&mut self) {
        while !self.eof() && !self.at(RBRACE) {
            self.parse_member_declaration();
        }
    }

    /*
     * memberDeclaration
     *   : modifiers memberDeclaration'
     *   ;
     *
     * memberDeclaration'
     *   : companionObject
     *   : secondaryConstructor
     *   : function
     *   : property
     *   : class
     *   : extension
     *   : typeAlias
     *   : anonymousInitializer
     *   : object
     *   ;
     */
    /// `parseMemberDeclaration()`.
    pub(crate) fn parse_member_declaration(&mut self) {
        if self.at(SEMICOLON) {
            self.advance(); // SEMICOLON
            return;
        }
        let decl = self.mark();

        let is_companion_block = self.at(COMPANION_KEYWORD) && self.lookahead(1) == LBRACE;

        let mut detector = ModifierDetector::new();
        self.parse_modifier_list_with_consumer(
            Some(&mut detector),
            &TokenSet::EMPTY,
            /* localDeclaration = */ false,
        );

        let decl_type = if is_companion_block {
            self.parse_companion_block()
        } else {
            self.parse_member_declaration_rest(&detector)
        };

        match decl_type {
            None => {
                self.error_with_recovery("Expecting member declaration", Some(&TokenSet::EMPTY));
                decl.drop(self.my_builder());
            }
            Some(decl_type) => {
                self.close_declaration_with_comment_binders(decl, decl_type, true);
            }
        }
    }

    /// `parseMemberDeclarationRest(modifierDetector)`.
    pub(crate) fn parse_member_declaration_rest(&mut self, modifier_detector: &ModifierDetector) -> Option<NodeKind> {
        let mut decl_type = self.parse_common_declaration(
            modifier_detector,
            if modifier_detector.is_companion_detected() {
                NameParsingMode::ALLOWED
            } else {
                NameParsingMode::REQUIRED
            },
            DeclarationParsingMode::MEMBER_OR_TOPLEVEL,
        );

        if decl_type.is_some() {
            return decl_type;
        }

        if self.at(INIT_KEYWORD) {
            self.advance(); // init
            if self.at(LBRACE) {
                self.parse_block();
            } else {
                let marker = self.mark();
                marker.error(self.my_builder(), "Expecting '{' after 'init'");
            }
            decl_type = Some(CLASS_INITIALIZER);
        } else if self.at(CONSTRUCTOR_KEYWORD) {
            self.parse_secondary_constructor();
            decl_type = Some(SECONDARY_CONSTRUCTOR);
        } else if self.at(LBRACE) {
            self.error("Expecting member declaration");
            self.parse_block();
            decl_type = Some(FUN);
        }
        decl_type
    }

    /// `parseCompanionBlock()`.
    pub(crate) fn parse_companion_block(&mut self) -> Option<NodeKind> {
        self.parse_class_body();
        Some(COMPANION_BLOCK)
    }

    /*
     * secondaryConstructor
     *   : modifiers "constructor" valueParameters (":" constructorDelegationCall)? block
     * constructorDelegationCall
     *   : "this" valueArguments
     *   : "super" valueArguments
     */
    /// `parseSecondaryConstructor()`.
    pub(crate) fn parse_secondary_constructor(&mut self) {
        const VALUE_ARGS_RECOVERY_COLON_SET: TokenSet =
            TokenSet::or_set(&[VALUE_ARGS_RECOVERY_SET, TokenSet::create(&[COLON])]);

        debug_assert!(self._at(CONSTRUCTOR_KEYWORD));

        self.advance(); // CONSTRUCTOR_KEYWORD

        if self.at(LPAR) {
            self.parse_value_parameter_list(false, /*typeRequired = */ true, &VALUE_ARGS_RECOVERY_SET);
        } else {
            self.error_with_recovery("Expecting '('", Some(&VALUE_ARGS_RECOVERY_COLON_SET));
        }

        if self.at(COLON) {
            self.advance(); // COLON

            let delegation_call = self.mark();

            if self.at(THIS_KEYWORD) || self.at(SUPER_KEYWORD) {
                self.parse_this_or_super();
                self.my_expression_parsing().parse_value_argument_list();
            } else {
                self.error("Expecting a 'this' or 'super' constructor call");
                let mut before_wrong_delegation_callee = None;
                if !self.at(LPAR) {
                    before_wrong_delegation_callee = Some(self.mark());
                    self.advance(); // wrong delegation callee
                }
                self.my_expression_parsing().parse_value_argument_list();

                if let Some(before_wrong_delegation_callee) = before_wrong_delegation_callee {
                    if self.at(LBRACE) {
                        before_wrong_delegation_callee.drop(self.my_builder());
                    } else {
                        before_wrong_delegation_callee.rollback_to(self.my_builder());
                    }
                }
            }

            delegation_call.done(self.my_builder(), CONSTRUCTOR_DELEGATION_CALL);
        } else {
            // empty constructor delegation call
            let empty_delegation_call = self.mark();
            let reference = self.mark();
            reference.done(self.my_builder(), CONSTRUCTOR_DELEGATION_REFERENCE);
            empty_delegation_call.done(self.my_builder(), CONSTRUCTOR_DELEGATION_CALL);
        }

        if self.at(LBRACE) {
            self.parse_block();
        }
    }

    /// `parseThisOrSuper()`.
    pub(crate) fn parse_this_or_super(&mut self) {
        debug_assert!(self._at(THIS_KEYWORD) || self._at(SUPER_KEYWORD));
        let mark = self.mark();

        self.advance(); // THIS_KEYWORD | SUPER_KEYWORD

        mark.done(self.my_builder(), CONSTRUCTOR_DELEGATION_REFERENCE);
    }

    /*
     * typeAlias
     *   : modifiers "typealias" SimpleName typeParameters? "=" type
     *   ;
     */
    /// `parseTypeAlias()`.
    pub(crate) fn parse_type_alias(&mut self) -> Option<NodeKind> {
        debug_assert!(self._at(TYPE_ALIAS_KEYWORD));

        self.advance(); // TYPE_ALIAS_KEYWORD

        self.expect_with_recovery(
            IDENTIFIER,
            "Type name expected",
            Some(&LT_EQ_SEMICOLON_TOP_LEVEL_DECLARATION_FIRST_SET),
        );

        self.parse_type_parameter_list(&TYPE_PARAMETER_GT_RECOVERY_SET);

        if self.at(WHERE_KEYWORD) {
            let error = self.mark();
            self.parse_type_constraints();
            error.error(self.my_builder(), "Type alias parameters can't have bounds");
        }

        self.expect_with_recovery(EQ, "Expecting '='", Some(&TOP_LEVEL_DECLARATION_FIRST_SEMICOLON_SET));

        self.parse_type_ref();

        self.consume_if(SEMICOLON);

        Some(TYPEALIAS)
    }

    /*
     * variableDeclarationEntry
     *   : SimpleName (":" type)?
     *   ;
     *
     * property
     *   : modifiers ("val" | "var")
     *       typeParameters?
     *       (type ".")?
     *       ("(" variableDeclarationEntry{","} ")" | variableDeclarationEntry)
     *       typeConstraints
     *       ("by" | "=" expression SEMI?)?
     *       (getter? setter? | setter? getter?) SEMI?
     *   ;
     */
    /// `parseProperty(mode)`.
    pub fn parse_property(&mut self, mode: DeclarationParsingMode) -> Option<NodeKind> {
        const PROPERTY_GETTER_OR_SETTER_RECOVERY_SET: TokenSet =
            TokenSet::or_set(&[DECLARATION_FIRST, TokenSet::create(&[EOL_OR_SEMICOLON, LBRACE, RBRACE])]);

        let is_short_form = self.at(VAL_KEYWORD) || self.at(VAR_KEYWORD);
        if is_short_form {
            self.advance();
        }

        let type_parameters_declared =
            self.at(LT) && self.parse_type_parameter_list(&IDENTIFIER_EQ_COLON_SEMICOLON_SET);

        self.my_builder().disable_joining_complex_tokens();

        let receiver = self.mark();
        let receiver_type_declared = self.parse_receiver_type("property", &PROPERTY_NAME_FOLLOW_SET);

        let multi_declaration = self.at(LPAR) || self.at(LBRACKET);

        self.error_if(
            receiver,
            multi_declaration && receiver_type_declared,
            "Receiver type is not allowed on a destructuring declaration",
        );

        let is_name_on_the_next_line = self.eol();
        let before_name = self.mark();

        if multi_declaration {
            let multi_decl = self.mark();
            self.parse_multi_declaration_entry(
                if is_short_form { &PROPERTY_NAME_FOLLOW_SET } else { &DESTRUCTURING_PROPERTY_NAME_FOLLOW_SET },
                &PROPERTY_NAME_FOLLOW_MULTI_DECLARATION_RECOVERY_SET,
                if is_short_form { MultiDeclarationMode::SHORT } else { MultiDeclarationMode::FULL },
            );
            self.error_if(
                multi_decl,
                !mode.destructuring_allowed(),
                "Destructuring declarations are only allowed for local variables/values",
            );
        } else {
            self.parse_function_or_property_name(
                receiver_type_declared,
                "property",
                &PROPERTY_NAME_FOLLOW_SET,
                &PROPERTY_NAME_FOLLOW_FUNCTION_OR_PROPERTY_RECOVERY_SET,
                /*nameRequired = */ true,
            );
        }

        self.my_builder().restore_joining_complex_tokens_state();

        let mut no_type_reference = true;
        if self.at(COLON) {
            no_type_reference = false;
            let type_ = self.mark();
            self.advance(); // COLON
            self.parse_type_ref();
            self.error_if(type_, multi_declaration, "Type annotations are not allowed on destructuring declarations");
        }

        self.parse_type_constraints_guarded(type_parameters_declared);

        if !self.parse_property_delegate_or_assignment()
            && is_name_on_the_next_line
            && no_type_reference
            && !receiver_type_declared
        {
            // Do not parse property identifier on the next line if declaration is invalid
            // In most cases this identifier relates to next statement/declaration
            if !multi_declaration || is_short_form {
                before_name.rollback_to(self.my_builder());
                self.error("Expecting property name or receiver type");
            } else {
                before_name.drop(self.my_builder());
            }

            return Some(if multi_declaration { DESTRUCTURING_DECLARATION } else { PROPERTY });
        }

        before_name.drop(self.my_builder());

        if mode.accessors_allowed() {
            // It's only needed for non-local properties, because in local ones:
            // "val a = 1; b" must not be an infix call of b on "val ...;"

            self.my_builder().enable_newlines();
            let has_new_line_with_semicolon =
                self.consume_if(SEMICOLON) && self.my_builder().newline_before_current_token();
            self.my_builder().restore_newlines_state();

            if !has_new_line_with_semicolon {
                let mut already_read = PropertyComponentKindCollector::default();
                let mut property_component_kind = self.parse_property_component(&mut already_read);

                while let Some(kind) = property_component_kind {
                    already_read.collect(kind);
                    property_component_kind = self.parse_property_component(&mut already_read);
                }

                if !self.at_set(&EOL_OR_SEMICOLON_RBRACE_SET) {
                    if self.get_last_token() != SEMICOLON {
                        self.error_until("Property getter or setter expected", &PROPERTY_GETTER_OR_SETTER_RECOVERY_SET);
                    }
                } else {
                    self.consume_if(SEMICOLON);
                }
            }
        }

        Some(if multi_declaration { DESTRUCTURING_DECLARATION } else { PROPERTY })
    }

    /// `parsePropertyDelegateOrAssignment()`.
    pub(crate) fn parse_property_delegate_or_assignment(&mut self) -> bool {
        if self.at(BY_KEYWORD) {
            self.parse_property_delegate();
            return true;
        } else if self.at(EQ) {
            self.advance(); // EQ
            self.my_expression_parsing().parse_expression();
            return true;
        }

        false
    }

    /*
     * propertyDelegate
     *   : "by" expression
     *   ;
     */
    /// `parsePropertyDelegate()`.
    pub(crate) fn parse_property_delegate(&mut self) {
        debug_assert!(self._at(BY_KEYWORD));
        let delegate = self.mark();
        self.advance(); // BY_KEYWORD
        self.my_expression_parsing().parse_expression();
        delegate.done(self.my_builder(), PROPERTY_DELEGATE);
    }

    /*
     * (SimpleName (":" type){","})
     */
    /// `parseMultiDeclarationEntry(follow, recoverySet, mode)`.
    pub fn parse_multi_declaration_entry(
        &mut self,
        follow: &TokenSet,
        recovery_set: &TokenSet,
        mode: MultiDeclarationMode,
    ) {
        // Parsing multi-name, e.g.
        //   val (a, b) = foo()
        //   (val a: X = aa, var b) = foo()
        //   val [a, b] = foo()
        //   [val a: X, var b] = foo()
        self.my_builder().disable_newlines();

        let is_parentheses = self.at(LPAR);
        let closing_brace = if is_parentheses { RPAR } else { RBRACKET };

        self.advance(); // LPAR | LBRACKET

        if !self.at_set(follow) {
            loop {
                if self.at(COMMA) {
                    self.error_and_advance("Expecting a name");
                } else if self.at(closing_brace) {
                    // For declaration similar to `val () = somethingCall()`
                    self.error("Expecting a name");
                    break;
                }
                let property = self.mark();

                if mode == MultiDeclarationMode::FULL {
                    if self.at(VAL_KEYWORD) || self.at(VAR_KEYWORD) {
                        self.advance();
                    } else {
                        self.error_with_recovery("Expecting val or var keyword", Some(recovery_set));
                    }
                } else if mode == MultiDeclarationMode::FULL_VAL_ONLY {
                    if self.at(VAL_KEYWORD) {
                        self.advance();
                    } else {
                        self.error_with_recovery("Expecting val keyword", Some(recovery_set));
                    }
                } else {
                    self.parse_modifier_list(&COMMA_RPAR_RBRACKET_COLON_EQ_SET);
                }

                self.expect_with_recovery(IDENTIFIER, "Expecting a name", Some(recovery_set));

                if self.at(COLON) {
                    self.advance(); // COLON
                    self.parse_type_ref_with(follow);
                }

                // Renaming is only allowed in name-based destructuring
                if self.at(EQ) && closing_brace == RPAR {
                    self.advance();
                    self.my_expression_parsing().parse_simple_name_expression();
                }

                property.done(self.my_builder(), DESTRUCTURING_DECLARATION_ENTRY);

                if !self.at(COMMA) {
                    break;
                }
                self.advance(); // COMMA
                if self.at(closing_brace) {
                    break;
                }
            }
        }

        self.expect_with_recovery(
            closing_brace,
            if is_parentheses { "Expecting ')'" } else { "Expecting ']'" },
            Some(follow),
        );
        self.my_builder().restore_newlines_state();
    }

    /*
     * propertyComponent
     *   : modifiers ("get" | "set")
     *   :
     *        (     "get" "(" ")"
     *           |
     *              "set" "(" modifiers parameter ")"
     *           |
     *              "field"
     *        ) functionBody
     *   ;
     */
    /// `parsePropertyComponent(notAllowedKind)`.
    pub(crate) fn parse_property_component(
        &mut self,
        not_allowed_kind: &mut PropertyComponentKindCollector,
    ) -> Option<PropertyComponentKind> {
        const ACCESSOR_BODY_RECOVERY_SET: TokenSet =
            TokenSet::or_set(&[ACCESSOR_FIRST_OR_PROPERTY_END, TokenSet::create(&[LBRACE, LPAR, EQ])]);
        const SETTER_PARAMETER_LIST_RECOVERY_SET: TokenSet =
            TokenSet::create(&[RPAR, COLON, LBRACE, RBRACE, EQ, EOL_OR_SEMICOLON]);

        let property_component = self.mark();

        self.parse_modifier_list(&TokenSet::EMPTY);

        let property_component_kind = if self.at(GET_KEYWORD) {
            PropertyComponentKind::GET
        } else if self.at(SET_KEYWORD) {
            PropertyComponentKind::SET
        } else if self.at(FIELD_KEYWORD) {
            PropertyComponentKind::FIELD
        } else {
            property_component.rollback_to(self.my_builder());
            return None;
        };

        if not_allowed_kind.contains(property_component_kind) {
            property_component.rollback_to(self.my_builder());
            return None;
        }

        self.advance(); // GET_KEYWORD, SET_KEYWORD or FIELD_KEYWORD

        if !self.at(LPAR) && property_component_kind != PropertyComponentKind::FIELD {
            // Account for Jet-114 (val a : int get {...})
            if !self.at_set(&ACCESSOR_FIRST_OR_PROPERTY_END) {
                self.error_until("Accessor body expected", &ACCESSOR_BODY_RECOVERY_SET);
            } else {
                self.close_declaration_with_comment_binders(property_component, PROPERTY_ACCESSOR, true);
                return Some(property_component_kind);
            }
        }

        self.my_builder().disable_newlines();

        if property_component_kind != PropertyComponentKind::FIELD {
            let parameter_list = self.mark();
            self.expect_with_recovery(LPAR, "Expecting '('", Some(&RPAR_IDENTIFIER_COLON_LBRACE_EQ_SET));
            if property_component_kind == PropertyComponentKind::SET {
                let setter_parameter = self.mark();
                self.parse_modifier_list(&COMMA_COLON_RPAR_SET);
                self.expect_with_recovery(IDENTIFIER, "Expecting parameter name", Some(&RPAR_COLON_LBRACE_EQ_SET));

                if self.at(COLON) {
                    self.advance(); // COLON
                    self.parse_type_ref();
                }
                setter_parameter.done(self.my_builder(), VALUE_PARAMETER);
                if self.at(COMMA) {
                    self.advance(); // COMMA
                }
            }
            if !self.at(RPAR) {
                self.error_until("Expecting ')'", &SETTER_PARAMETER_LIST_RECOVERY_SET);
            }
            if self.at(RPAR) {
                self.advance();
            }
            parameter_list.done(self.my_builder(), VALUE_PARAMETER_LIST);
        }
        self.my_builder().restore_newlines_state();

        if self.at(COLON) {
            self.advance();

            self.parse_type_ref();
        }

        if property_component_kind != PropertyComponentKind::FIELD {
            self.parse_function_contract();
            self.parse_function_body();
        } else if self.at(EQ) {
            self.advance();
            self.my_expression_parsing().parse_expression();
            self.consume_if(SEMICOLON);
        }

        if property_component_kind == PropertyComponentKind::FIELD {
            self.close_declaration_with_comment_binders(property_component, BACKING_FIELD, true);
        } else {
            self.close_declaration_with_comment_binders(property_component, PROPERTY_ACCESSOR, true);
        }

        Some(property_component_kind)
    }

    /// `parseFunction()`: never `None`.
    pub(crate) fn parse_function(&mut self) -> Option<NodeKind> {
        self.parse_function_with(false)
    }

    /*
     * function
     *   : modifiers "fun" typeParameters?
     *       (type ".")?
     *       SimpleName
     *       typeParameters? functionParameters (":" type)?
     *       typeConstraints
     *       functionBody?
     *   ;
     */
    /// `parseFunction(failIfIdentifierExists)`: `None` only when `fail_if_identifier_exists` and a name follows.
    pub fn parse_function_with(&mut self, fail_if_identifier_exists: bool) -> Option<NodeKind> {
        debug_assert!(self._at(FUN_KEYWORD));

        self.advance(); // FUN_KEYWORD

        // Recovery for the case of class A { fun| }
        if self.at(RBRACE) {
            self.error("Function body expected");
            return Some(FUN);
        }

        let mut type_parameter_list_occurred = false;
        if self.at(LT) {
            self.parse_type_parameter_list(&LBRACKET_LBRACE_RBRACE_LPAR_SET);
            type_parameter_list_occurred = true;
        }

        self.my_builder().disable_joining_complex_tokens();

        let receiver_found = self.parse_receiver_type("function", &FUNCTION_NAME_FOLLOW_SET);

        if self.at(IDENTIFIER) && fail_if_identifier_exists {
            self.my_builder().restore_joining_complex_tokens_state();
            return None;
        }

        // function as expression has no name
        self.parse_function_or_property_name(
            receiver_found,
            "function",
            &FUNCTION_NAME_FOLLOW_SET,
            &FUNCTION_NAME_RECOVERY_SET,
            /*nameRequired = */ false,
        );

        self.my_builder().restore_joining_complex_tokens_state();

        if self.at(LT) {
            let mut error = self.mark();
            self.parse_type_parameter_list(&LPAR_VALUE_PARAMETERS_FOLLOW_SET);
            if type_parameter_list_occurred {
                let finish_index = self.my_builder().raw_token_index();
                error.rollback_to(self.my_builder());
                error = self.mark();
                let advance_token_count = finish_index - self.my_builder().raw_token_index();
                self.advance_n(advance_token_count);
                error.error(self.my_builder(), "Only one type parameter list is allowed for a function");
            } else {
                error.drop(self.my_builder());
            }
            type_parameter_list_occurred = true;
        }

        if self.at(LPAR) {
            self.parse_value_parameter_list(false, /* typeRequired  = */ false, &VALUE_PARAMETERS_FOLLOW_SET);
        } else {
            self.error("Expecting '('");
        }

        if self.at(COLON) {
            self.advance(); // COLON

            self.parse_type_ref();
        }

        let function_contract_occurred = self.parse_function_contract();

        self.parse_type_constraints_guarded(type_parameter_list_occurred);

        if !function_contract_occurred {
            self.parse_function_contract();
        }

        if self.at(SEMICOLON) {
            self.advance(); // SEMICOLON
        } else if self.at(EQ) || self.at(LBRACE) {
            self.parse_function_body();
        }

        Some(FUN)
    }

    /*
     *   (type "." | annotations)?
     */
    /// `parseReceiverType(title, nameFollow)`.
    pub(crate) fn parse_receiver_type(&mut self, title: &'static str, name_follow: &TokenSet) -> bool {
        let annotations = self.mark();
        let annotations_present = self.parse_annotations(AnnotationParsingMode::DEFAULT);
        let last_dot = self.last_dot_after_receiver();
        let receiver_present = last_dot != -1;
        if annotations_present {
            if receiver_present {
                annotations.rollback_to(self.my_builder());
            } else {
                annotations.error(self.my_builder(), "Annotations are not allowed in this position");
            }
        } else {
            annotations.drop(self.my_builder());
        }

        if !receiver_present {
            return false;
        }

        self.create_truncated_builder(last_dot).parse_type_ref_without_intersections();

        if self.at_set(&RECEIVER_TYPE_TERMINATORS) {
            self.advance(); // expectation
        } else {
            self.error_with_recovery(format!("Expecting '.' before a {title} name"), Some(name_follow));
        }
        true
    }

    /// `lastDotAfterReceiver()`. The Java pattern fields are reset before each use, so a fresh pattern is the same.
    pub(crate) fn last_dot_after_receiver(&mut self) -> i32 {
        if self.at(LPAR) {
            let mut pattern = Self::last_dot_after_receiver_l_par_pattern();
            pattern.reset();
            self.match_token_stream_predicate(&mut pattern)
        } else {
            let mut pattern = Self::last_dot_after_receiver_not_l_par_pattern();
            pattern.reset();
            self.match_token_stream_predicate(&mut pattern)
        }
    }

    /*
     * IDENTIFIER
     */
    /// `parseFunctionOrPropertyName(receiverFound, title, nameFollow, recoverySet, nameRequired)`.
    pub(crate) fn parse_function_or_property_name(
        &mut self,
        receiver_found: bool,
        title: &'static str,
        name_follow: &TokenSet,
        recovery_set: &TokenSet,
        name_required: bool,
    ) {
        if !name_required && self.at_set(name_follow) {
            return; // no name
        }

        if self.expect(IDENTIFIER) {
            return;
        }

        self.error_with_recovery(
            format!("Expecting {title} name{}", if !receiver_found { " or receiver type" } else { "" }),
            Some(recovery_set),
        );
    }

    /*
     * functionBody
     *   : block
     *   : "=" element
     *   ;
     */
    /// `parseFunctionBody()`.
    pub(crate) fn parse_function_body(&mut self) {
        if self.at(LBRACE) {
            self.parse_block();
        } else if self.at(EQ) {
            self.advance(); // EQ
            self.my_expression_parsing().parse_expression();
            self.consume_if(SEMICOLON);
        } else {
            self.error("Expecting function body");
        }
    }

    /*
     * block
     *   : "{" (expressions)* "}"
     *   ;
     */
    /// `parseBlock()`.
    pub fn parse_block(&mut self) {
        self.parse_block_with(/*collapse*/ true);
    }

    /// `parseBlock(collapse)`.
    pub(crate) fn parse_block_with(&mut self, collapse: bool) {
        let lazy_block = self.mark();

        self.my_builder().enable_newlines();

        let has_opening_brace = self.expect_with(LBRACE, "Expecting '{' to open a block");
        let can_collapse = collapse && has_opening_brace && self.is_lazy;

        if can_collapse {
            self.advance_balanced_block();
        } else {
            self.my_expression_parsing().parse_statements();
            self.expect_with(RBRACE, "Expecting '}'");
        }

        self.my_builder().restore_newlines_state();

        if can_collapse {
            lazy_block.collapse(self.my_builder(), BLOCK);
        } else {
            lazy_block.done(self.my_builder(), BLOCK);
        }
    }

    /// `advanceBalancedBlock()`. Without a truncation the builder skips the block in one scan of its raw tokens
    /// (`SemanticWhitespaceAwarePsiBuilder::skip_balanced_block` says why that is exact); the loop below is the Java.
    pub fn advance_balanced_block(&mut self) {
        if self.my_builder().skip_balanced_block() {
            return;
        }
        let mut brace_count = 1;
        while !self.eof() {
            if self._at(LBRACE) {
                brace_count += 1;
            } else if self._at(RBRACE) {
                brace_count -= 1;
            }

            self.advance();
            crate::counters::add(crate::counters::Counter::BalancedSkipTokens, 1);

            if brace_count == 0 {
                break;
            }
        }
    }

    /*
     * delegationSpecifier{","}
     */
    /// `parseDelegationSpecifierList()`.
    pub(crate) fn parse_delegation_specifier_list(&mut self) {
        let list = self.mark();

        loop {
            if self.at(COMMA) {
                self.error_and_advance("Expecting a delegation specifier");
                continue;
            }
            self.parse_delegation_specifier();
            if !self.at(COMMA) {
                break;
            }
            self.advance(); // COMMA
        }

        list.done(self.my_builder(), SUPER_TYPE_LIST);
    }

    /*
     * delegationSpecifier
     *   : constructorInvocation // type and constructor arguments
     *   : userType
     *   : explicitDelegation
     *   ;
     *
     * explicitDelegation
     *   : userType "by" element
     *   ;
     */
    /// `parseDelegationSpecifier()`.
    pub(crate) fn parse_delegation_specifier(&mut self) {
        let delegator = self.mark();
        let reference = self.mark();
        self.parse_type_ref();

        if self.at(BY_KEYWORD) {
            reference.drop(self.my_builder());
            self.advance(); // BY_KEYWORD
            KotlinParsing::create_for_by_clause(self.my_builder.reborrow(), self.is_lazy)
                .my_expression_parsing()
                .parse_expression();
            delegator.done(self.my_builder(), DELEGATED_SUPER_TYPE_ENTRY);
        } else if self.at(LPAR) {
            reference.done(self.my_builder(), CONSTRUCTOR_CALLEE);
            self.my_expression_parsing().parse_value_argument_list();
            delegator.done(self.my_builder(), SUPER_TYPE_CALL_ENTRY);
        } else {
            reference.drop(self.my_builder());
            delegator.done(self.my_builder(), SUPER_TYPE_ENTRY);
        }
    }

    /*
     * typeParameters
     *   : ("<" typeParameter{","} ">"
     *   ;
     */
    /// `parseTypeParameterList(recoverySet)`.
    pub(crate) fn parse_type_parameter_list(&mut self, recovery_set: &TokenSet) -> bool {
        let mut result = false;
        if self.at(LT) {
            let list = self.mark();

            self.my_builder().disable_newlines();
            self.advance(); // LT

            loop {
                if self.at(COMMA) {
                    self.error_and_advance("Expecting type parameter declaration");
                }
                self.parse_type_parameter();

                if !self.at(COMMA) {
                    break;
                }
                self.advance(); // COMMA
                if self.at(GT) {
                    break;
                }
            }

            self.expect_with_recovery(GT, "Missing '>'", Some(recovery_set));
            self.my_builder().restore_newlines_state();
            result = true;

            list.done(self.my_builder(), TYPE_PARAMETER_LIST);
        }
        result
    }

    /*
     * typeConstraints
     *   : ("where" typeConstraint{","})?
     *   ;
     */
    /// `parseTypeConstraintsGuarded(typeParameterListOccurred)`.
    pub(crate) fn parse_type_constraints_guarded(&mut self, type_parameter_list_occurred: bool) {
        let error = self.mark();
        let constraints = self.parse_type_constraints();
        self.error_if(
            error,
            constraints && !type_parameter_list_occurred,
            "Type constraints are not allowed when no type parameters declared",
        );
    }

    /// `parseTypeConstraints()`.
    pub(crate) fn parse_type_constraints(&mut self) -> bool {
        if self.at(WHERE_KEYWORD) {
            self.parse_type_constraint_list();
            return true;
        }
        false
    }

    /*
     * typeConstraint{","}
     */
    /// `parseTypeConstraintList()`.
    pub(crate) fn parse_type_constraint_list(&mut self) {
        debug_assert!(self._at(WHERE_KEYWORD));

        self.advance(); // WHERE_KEYWORD

        let list = self.mark();

        loop {
            if self.at(COMMA) {
                self.error_and_advance("Type constraint expected");
            }
            self.parse_type_constraint();
            if !self.at(COMMA) {
                break;
            }
            self.advance(); // COMMA
        }

        list.done(self.my_builder(), TYPE_CONSTRAINT_LIST);
    }

    /*
     * typeConstraint
     *   : annotations SimpleName ":" type
     *   ;
     */
    /// `parseTypeConstraint()`.
    pub(crate) fn parse_type_constraint(&mut self) {
        let constraint = self.mark();

        self.parse_annotations(AnnotationParsingMode::DEFAULT);

        let reference = self.mark();
        if self.expect_with_recovery(
            IDENTIFIER,
            "Expecting type parameter name",
            Some(&COLON_COMMA_LBRACE_RBRACE_TYPE_REF_FIRST_SET),
        ) {
            reference.done(self.my_builder(), REFERENCE_EXPRESSION);
        } else {
            reference.drop(self.my_builder());
        }

        self.expect_with_recovery(
            COLON,
            "Expecting ':' before the upper bound",
            Some(&LBRACE_RBRACE_TYPE_REF_FIRST_SET),
        );

        self.parse_type_ref();

        constraint.done(self.my_builder(), TYPE_CONSTRAINT);
    }

    /// `parseFunctionContract()`.
    pub(crate) fn parse_function_contract(&mut self) -> bool {
        if self.at(CONTRACT_KEYWORD) {
            self.my_expression_parsing().parse_contract_description_block();
            return true;
        }
        false
    }

    /*
     * typeParameter
     *   : modifiers SimpleName (":" userType)?
     *   ;
     */
    /// `parseTypeParameter()`.
    pub(crate) fn parse_type_parameter(&mut self) {
        if self.at_set(&TYPE_PARAMETER_GT_RECOVERY_SET) {
            self.error("Type parameter declaration expected");
            return;
        }

        let mark = self.mark();

        self.parse_modifier_list(&GT_COMMA_COLON_SET);

        self.expect_with_recovery(IDENTIFIER, "Type parameter name expected", Some(&TokenSet::EMPTY));

        if self.at(COLON) {
            self.advance(); // COLON
            self.parse_type_ref();
        }

        mark.done(self.my_builder(), TYPE_PARAMETER);
    }

    /*
     * type
     *   : typeModifiers typeReference
     *   ;
     *
     * typeReference
     *   : functionType
     *   : userType
     *   : nullableType
     *   : "dynamic"
     *   ;
     *
     * nullableType
     *   : typeReference "?"
     *   ;
     */
    /// `parseTypeRef()`.
    pub fn parse_type_ref(&mut self) {
        self.parse_type_ref_with(&TokenSet::EMPTY);
    }

    /// `parseTypeRefWithoutIntersections()`.
    pub fn parse_type_ref_without_intersections(&mut self) {
        self.parse_type_ref_with_intersections(&TokenSet::EMPTY, /* allowSimpleIntersectionTypes */ false);
    }

    /// `parseTypeRef(extraRecoverySet)`.
    pub fn parse_type_ref_with(&mut self, extra_recovery_set: &TokenSet) {
        self.parse_type_ref_with_intersections(extra_recovery_set, /* allowSimpleIntersectionTypes */ true);
    }

    /// `parseTypeRef(extraRecoverySet, allowSimpleIntersectionTypes)`.
    pub(crate) fn parse_type_ref_with_intersections(
        &mut self,
        extra_recovery_set: &TokenSet,
        allow_simple_intersection_types: bool,
    ) {
        let type_ref_marker = self.parse_type_ref_contents(extra_recovery_set, allow_simple_intersection_types);
        type_ref_marker.done(self.my_builder(), TYPE_REFERENCE);
    }

    /// `parseTypeRefContents(extraRecoverySet, allowSimpleIntersectionTypes)`.
    ///
    /// The extraRecoverySet is needed for the foo(bar<x, 1, y>(z)) case, to tell whether we should stop
    /// on expression-indicating symbols or not
    pub(crate) fn parse_type_ref_contents(
        &mut self,
        extra_recovery_set: &TokenSet,
        allow_simple_intersection_types: bool,
    ) -> Marker {
        const TYPE_RECOVERY_SET: TokenSet = TokenSet::or_set(&[
            TOP_LEVEL_DECLARATION_FIRST,
            TokenSet::create(&[EQ, COMMA, GT, RBRACKET, DOT, RPAR, RBRACE, LBRACE, SEMICOLON]),
        ]);

        let type_ref_marker = self.mark();

        self.parse_type_modifier_list();

        let lookahead = self.lookahead(1);
        let lookahead2 = self.lookahead(2);
        let mut type_before_dot = true;
        let with_context_receiver = self.at(CONTEXT_KEYWORD) && lookahead == LPAR;
        let mut was_function_type_parsed = false;

        let context_receivers_start = self.mark();

        if with_context_receiver {
            self.parse_context_parameter_or_receiver_list(true);
        }

        let mut type_element_marker = self.mark();

        if self.at(IDENTIFIER)
            && !(lookahead == DOT && lookahead2 == IDENTIFIER)
            && lookahead != LT
            && self.at(DYNAMIC_KEYWORD)
        {
            let dynamic_type = self.mark();
            self.advance(); // DYNAMIC_KEYWORD
            dynamic_type.done(self.my_builder(), DYNAMIC_TYPE);
        } else if self.at(IDENTIFIER) || self.at(PACKAGE_KEYWORD) || self.at_parenthesized_mutable_for_platform_types(0)
        {
            self.parse_user_type();
        } else if self.at(LPAR) {
            let function_or_parenthesized_type = self.mark();

            // This may be a function parameter list or just a parenthesized type
            self.advance(); // LPAR
            self.parse_type_ref_contents(&TokenSet::EMPTY, /* allowSimpleIntersectionTypes */ true)
                .drop(self.my_builder()); // parenthesized types, no reference element around it is needed

            if self.at(RPAR) && self.lookahead(1) != ARROW {
                // It's a parenthesized type
                //    (A)
                self.advance();
                function_or_parenthesized_type.drop(self.my_builder());
            } else {
                // This must be a function type
                //   (A, B) -> C
                // or
                //   (a : A) -> C
                function_or_parenthesized_type.rollback_to(self.my_builder());
                let function_type = context_receivers_start.precede(self.my_builder());
                self.parse_function_type(function_type);
                was_function_type_parsed = true;
            }
        } else {
            self.error_with_recovery(
                "Type expected",
                Some(&TokenSet::or_set(&[TYPE_RECOVERY_SET, *extra_recovery_set])),
            );
            type_before_dot = false;
        }

        // Disabling token merge is required for cases like
        //    Int?.(Foo) -> Bar
        self.my_builder().disable_joining_complex_tokens();
        type_element_marker = self.parse_nullable_type_suffix(type_element_marker);
        self.my_builder().restore_joining_complex_tokens_state();

        let mut was_intersection = false;
        if allow_simple_intersection_types && self.at(AND) {
            let left_type_ref = type_element_marker;

            type_element_marker = type_element_marker.precede(self.my_builder());
            let intersection_type = left_type_ref.precede(self.my_builder());

            left_type_ref.done(self.my_builder(), TYPE_REFERENCE);

            self.advance(); // &
            self.parse_type_ref_with_intersections(extra_recovery_set, /* allowSimpleIntersectionTypes */ true);

            intersection_type.done(self.my_builder(), INTERSECTION_TYPE);
            was_intersection = true;
        }

        if type_before_dot && self.at(DOT) && !was_intersection && !was_function_type_parsed {
            // This is a receiver for a function type
            //  A.(B) -> C
            //   ^

            let function_type = context_receivers_start.precede(self.my_builder());

            let receiver_type_ref = type_element_marker.precede(self.my_builder());
            let receiver_type = receiver_type_ref.precede(self.my_builder());
            receiver_type_ref.done(self.my_builder(), TYPE_REFERENCE);
            receiver_type.done(self.my_builder(), FUNCTION_TYPE_RECEIVER);

            self.advance(); // DOT

            if self.at(LPAR) {
                self.parse_function_type(function_type);
            } else {
                function_type.drop(self.my_builder());
                self.error("Expecting function type");
            }

            was_function_type_parsed = true;
        }

        if with_context_receiver && !was_function_type_parsed {
            self.error_with_recovery(
                "Function type expected expected",
                Some(&TokenSet::or_set(&[TYPE_RECOVERY_SET, *extra_recovery_set])),
            );
        }

        type_element_marker.drop(self.my_builder());
        context_receivers_start.drop(self.my_builder());
        type_ref_marker
    }

    /// `parseNullableTypeSuffix(typeElementMarker)`.
    pub(crate) fn parse_nullable_type_suffix(&mut self, type_element_marker: Marker) -> Marker {
        let mut type_element_marker = type_element_marker;
        // ?: is joined regardless of joining state
        while self.at(QUEST) && self.my_builder().raw_lookup(1) != COLON {
            let precede = type_element_marker.precede(self.my_builder());
            self.advance(); // QUEST
            type_element_marker.done(self.my_builder(), NULLABLE_TYPE);
            type_element_marker = precede;
        }
        type_element_marker
    }

    /*
     * userType
     *   : simpleUserType{"."}
     *   ;
     *
     *   recovers on platform types:
     *    - Foo!
     *    - (Mutable)List<Foo>!
     *    - Array<(out) Foo>!
     */
    /// `parseUserType()`.
    pub(crate) fn parse_user_type(&mut self) {
        const IDENTIFIER_LBRACE_RBRACE_SET: TokenSet = TokenSet::create(&[IDENTIFIER, LBRACE, RBRACE]);

        let mut user_type = self.mark();

        if self.at(PACKAGE_KEYWORD) {
            let keyword = self.mark();
            self.advance(); // PACKAGE_KEYWORD
            keyword.error(self.my_builder(), "Expecting an element");
            self.expect_with_recovery(DOT, "Expecting '.'", Some(&IDENTIFIER_LBRACE_RBRACE_SET));
        }

        let mut reference = self.mark();
        loop {
            self.recover_on_parenthesized_word_for_platform_types(0, "Mutable", true);

            if self.expect_with_recovery(IDENTIFIER, "Expecting type name", Some(&USER_TYPE_NAME_RECOVERY_SET)) {
                reference.done(self.my_builder(), REFERENCE_EXPRESSION);
            } else {
                reference.drop(self.my_builder());
                break;
            }

            self.parse_type_argument_list();

            self.recover_on_platform_type_suffix();

            if !self.at(DOT) {
                break;
            }
            if self.lookahead(1) == LPAR && !self.at_parenthesized_mutable_for_platform_types(1) {
                // This may be a receiver for a function type
                //   Int.(Int) -> Int
                break;
            }

            let precede = user_type.precede(self.my_builder());
            user_type.done(self.my_builder(), USER_TYPE);
            user_type = precede;

            self.advance(); // DOT
            reference = self.mark();
        }

        user_type.done(self.my_builder(), USER_TYPE);
    }

    /// `atParenthesizedMutableForPlatformTypes(offset)`.
    pub(crate) fn at_parenthesized_mutable_for_platform_types(&mut self, offset: i32) -> bool {
        self.recover_on_parenthesized_word_for_platform_types(offset, "Mutable", false)
    }

    /// `recoverOnParenthesizedWordForPlatformTypes(offset, word, consume)`.
    pub(crate) fn recover_on_parenthesized_word_for_platform_types(
        &mut self,
        offset: i32,
        word: &'static str,
        consume: bool,
    ) -> bool {
        // Array<(out) Foo>! or (Mutable)List<Bar>!
        if self.lookahead(offset) == LPAR
            && self.lookahead(offset + 1) == IDENTIFIER
            && self.lookahead(offset + 2) == RPAR
            && self.lookahead(offset + 3) == IDENTIFIER
        {
            let error = self.mark();

            self.advance_n(offset);

            self.advance(); // LPAR
            if self.my_builder().get_token_bytes() != Some(word.as_bytes()) {
                // something other than "out" / "Mutable"
                error.rollback_to(self.my_builder());
                return false;
            } else {
                self.advance(); // IDENTIFIER('out')
                self.advance(); // RPAR

                if consume {
                    error.error(self.my_builder(), "Unexpected tokens");
                } else {
                    error.rollback_to(self.my_builder());
                }

                return true;
            }
        }
        false
    }

    /// `recoverOnPlatformTypeSuffix()`.
    pub(crate) fn recover_on_platform_type_suffix(&mut self) {
        // Recovery for platform types
        if self.at(EXCL) {
            let error = self.mark();
            self.advance(); // EXCL
            error.error(self.my_builder(), "Unexpected token");
        }
    }

    /*
     *  (optionalProjection type){","}
     */
    /// `parseTypeArgumentList()`.
    pub(crate) fn parse_type_argument_list(&mut self) {
        if !self.at(LT) {
            return;
        }

        let list = self.mark();

        self.try_parse_type_argument_list(&TokenSet::EMPTY);

        list.done(self.my_builder(), TYPE_ARGUMENT_LIST);
    }

    /// `tryParseTypeArgumentList(extraRecoverySet)`.
    pub fn try_parse_type_argument_list(&mut self, extra_recovery_set: &TokenSet) -> bool {
        self.my_builder().disable_newlines();
        self.advance(); // LT

        loop {
            let projection = self.mark();

            self.recover_on_parenthesized_word_for_platform_types(0, "out", true);

            // Currently we do not allow annotations on star projections and probably we should not
            // Annotations on other kinds of type arguments should be parsed as common type annotations (within parseTypeRef call)
            self.parse_type_argument_modifier_list();

            if self.at(MUL) {
                self.advance(); // MUL
            } else {
                self.parse_type_ref_with(extra_recovery_set);
            }
            projection.done(self.my_builder(), TYPE_PROJECTION);
            if !self.at(COMMA) {
                break;
            }
            self.advance(); // COMMA
            if self.at(GT) {
                break;
            }
        }

        let at_gt = self.at(GT);
        if !at_gt {
            self.error("Expecting a '>'");
        } else {
            self.advance(); // GT
        }
        self.my_builder().restore_newlines_state();
        at_gt
    }

    /*
     * functionType
     *   : (type ".")? "(" parameter{","}? ")" "->" type?
     *   ;
     */
    /// `parseFunctionType(functionType)`.
    pub(crate) fn parse_function_type(&mut self, function_type: Marker) {
        self.parse_function_type_contents(function_type).done(self.my_builder(), FUNCTION_TYPE);
    }

    /// `parseFunctionTypeContents(functionType)`.
    pub(crate) fn parse_function_type_contents(&mut self, function_type: Marker) -> Marker {
        debug_assert!(self._at(LPAR), "{:?}", self.tt());

        self.parse_value_parameter_list(true, /* typeRequired  = */ true, &TokenSet::EMPTY);

        self.expect_with_recovery(
            ARROW,
            "Expecting '->' to specify return type of a function type",
            Some(&TYPE_REF_FIRST),
        );
        self.parse_type_ref();

        function_type
    }

    /*
     * functionParameters
     *   : "(" functionParameter{","}? ")"
     *   ;
     *
     * functionParameter
     *   : modifiers functionParameterRest
     *   ;
     *
     * functionParameterRest
     *   : parameter ("=" element)?
     *   ;
     */
    /// `parseValueParameterList(isFunctionTypeContents, typeRequired, recoverySet)`.
    pub(crate) fn parse_value_parameter_list(
        &mut self,
        is_function_type_contents: bool,
        type_required: bool,
        recovery_set: &TokenSet,
    ) {
        debug_assert!(self._at(LPAR));
        let parameters = self.mark();

        self.my_builder().disable_newlines();

        self.value_parameter_loop(is_function_type_contents, recovery_set, &mut |p| {
            if is_function_type_contents {
                if !p.try_parse_value_parameter(type_required) {
                    let value_parameter = p.mark();
                    p.parse_function_type_value_parameter_modifier_list();
                    p.parse_type_ref();
                    p.close_declaration_with_comment_binders(value_parameter, VALUE_PARAMETER, false);
                }
            } else {
                p.parse_value_parameter(type_required);
            }
            true
        });

        self.my_builder().restore_newlines_state();

        parameters.done(self.my_builder(), VALUE_PARAMETER_LIST);
    }

    /// `valueParameterLoop(inFunctionTypeContext, recoverySet, parseParameter)`: `parse_parameter` returns `true` if
    /// internal parsing is correct; the result is `true` if the parsing of the entire parameter loop is correct.
    pub(crate) fn value_parameter_loop(
        &mut self,
        in_function_type_context: bool,
        recovery_set: &TokenSet,
        parse_parameter: &mut dyn FnMut(&mut Self) -> bool,
    ) -> bool {
        self.advance(); // LPAR

        let mut no_error = true;

        if !self.at(RPAR) && !self.at_set(recovery_set) {
            loop {
                let offset_before = self.my_builder().get_current_offset();
                if self.at(RPAR) {
                    break;
                }

                no_error = parse_parameter(self) && no_error;

                if self.at(COMMA) {
                    self.advance(); // COMMA
                } else if self.at(COLON) {
                    // recovery for the case "fun bar(x: Array<Int> : Int)" when we've just parsed "x: Array<Int>"
                    // error should be reported in the `parseValueParameter` call
                    continue;
                } else {
                    if !self.at(RPAR) {
                        self.error("Expecting comma or ')'");
                        no_error = false;
                    }
                    if !self.at_set(if in_function_type_context {
                        &LAMBDA_VALUE_PARAMETER_FIRST
                    } else {
                        &VALUE_PARAMETER_FIRST
                    }) {
                        break;
                    }
                    if offset_before == self.my_builder().get_current_offset() {
                        break;
                    }
                }
            }
        }

        self.expect_with_recovery(RPAR, "Expecting ')'", Some(recovery_set)) && no_error
    }

    /*
     * functionParameter
     *   : modifiers ("val" | "var")? parameter ("=" element)?
     *   ;
     */
    /// `tryParseValueParameter(typeRequired)`.
    pub(crate) fn try_parse_value_parameter(&mut self, type_required: bool) -> bool {
        self.parse_value_parameter_with(true, type_required)
    }

    /// `parseValueParameter(typeRequired)`.
    pub fn parse_value_parameter(&mut self, type_required: bool) {
        self.parse_value_parameter_with(false, type_required);
    }

    /// `parseValueParameter(rollbackOnFailure, typeRequired)`.
    pub(crate) fn parse_value_parameter_with(&mut self, rollback_on_failure: bool, type_required: bool) -> bool {
        let parameter = self.mark();

        self.parse_modifier_list(&NO_MODIFIER_BEFORE_FOR_VALUE_PARAMETER);

        if self.at(VAR_KEYWORD) || self.at(VAL_KEYWORD) {
            self.advance(); // VAR_KEYWORD | VAL_KEYWORD
        }

        if !self.parse_function_parameter_rest(type_required) && rollback_on_failure {
            parameter.rollback_to(self.my_builder());
            return false;
        }

        self.close_declaration_with_comment_binders(parameter, VALUE_PARAMETER, false);
        true
    }

    /*
     * functionParameterRest
     *   : parameter ("=" element)?
     *   ;
     */
    /// `parseFunctionParameterRest(typeRequired)`.
    pub(crate) fn parse_function_parameter_rest(&mut self, type_required: bool) -> bool {
        let mut no_errors = true;

        if self.at(COMMA) || self.at(RPAR) {
            self.error("Expecting a parameter declaration");
            no_errors = false;
        } else if self.at(IDENTIFIER) && self.lookahead(1) == LT {
            // Recovery for the case 'fun foo(Array<String>) {}'
            self.error("Parameter name expected");
            no_errors = false;
            self.parse_type_ref();
        } else if self.at(COLON) {
            // Recovery for the case 'fun foo(: Int) {}'
            self.error("Parameter name expected");
            // We keep noErrors == true so that unnamed parameters starting with ":" are not rolled back during parsing of functional types
            self.advance(); // COLON
            self.parse_type_ref();
        } else {
            self.expect_with_recovery(IDENTIFIER, "Parameter name expected", Some(&PARAMETER_NAME_RECOVERY_SET));

            if self.at(COLON) {
                self.advance(); // COLON

                if self.at(IDENTIFIER) && self.lookahead(1) == COLON {
                    // recovery for the case "fun foo(x: y: Int)" when we're at "y: " it's likely that this is a name of the next parameter,
                    // not a type reference of the current one
                    self.error("Type reference expected");
                    return false;
                }

                self.parse_type_ref();
            } else if type_required {
                self.error_with_recovery("Parameters must have type annotation", Some(&PARAMETER_NAME_RECOVERY_SET));
                no_errors = false;
            }
        }

        if self.at(EQ) {
            self.advance(); // EQ
            self.my_expression_parsing().parse_expression();
        }

        no_errors
    }
}
