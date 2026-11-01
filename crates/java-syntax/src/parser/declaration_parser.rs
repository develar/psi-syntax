//! `DeclarationParser.kt`: classes, enums, records, members, modifier lists, parameter lists, annotations.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::syntax_element_types::{MODIFIER_BIT_SET, PRIMITIVE_TYPE_BIT_SET};
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `DeclarationParser.Context`.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Context {
    FILE,
    CLASS,
    CODE_BLOCK,
    ANNOTATION_INTERFACE,
    JSHELL,
}

/// `DeclarationParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::declaration_parser`.
#[derive(Copy, Clone)]
pub struct DeclarationParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> DeclarationParser<'p> {
    /// `languageLevel`.
    fn language_level(&self) -> LanguageLevel {
        self.my_parser.language_level
    }

    /// `parseClassBodyWithBraces(builder, isAnnotation, isEnum)` (DeclarationParser.kt:37).
    pub fn parse_class_body_with_braces(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        is_annotation: bool,
        is_enum: bool,
    ) {
        // TODO(port): DeclarationParser.kt:37
        todo!("DeclarationParser.parseClassBodyWithBraces (DeclarationParser.kt:37)")
    }

    /// `parseClassFromKeyword(builder, declaration, isAnnotation, context)` (DeclarationParser.kt:50).
    fn parse_class_from_keyword(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        declaration: Marker,
        is_annotation: bool,
        context: Option<Context>,
    ) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:50
        todo!("DeclarationParser.parseClassFromKeyword (DeclarationParser.kt:50)")
    }

    /// `parseEnumConstants(builder)` (DeclarationParser.kt:121).
    fn parse_enum_constants(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): DeclarationParser.kt:121
        todo!("DeclarationParser.parseEnumConstants (DeclarationParser.kt:121)")
    }

    /// `parseEnumConstant(builder)` (DeclarationParser.kt:158).
    pub fn parse_enum_constant(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:158
        todo!("DeclarationParser.parseEnumConstant (DeclarationParser.kt:158)")
    }

    /// `parseClassBodyDeclarations(builder, isAnnotation)` (DeclarationParser.kt:186).
    pub fn parse_class_body_declarations(&self, builder: &mut SyntaxTreeBuilder<'_>, is_annotation: bool) {
        // TODO(port): DeclarationParser.kt:186
        todo!("DeclarationParser.parseClassBodyDeclarations (DeclarationParser.kt:186)")
    }

    /// `parse(builder, context)` (DeclarationParser.kt:226).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>, context: Option<Context>) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:226
        todo!("DeclarationParser.parse (DeclarationParser.kt:226)")
    }

    /// `isRecordToken(builder, tokenType)` (DeclarationParser.kt:383).
    pub fn is_record_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        // TODO(port): DeclarationParser.kt:383
        todo!("DeclarationParser.isRecordToken (DeclarationParser.kt:383)")
    }

    /// `isSealedToken(builder, tokenType)` (DeclarationParser.kt:402).
    fn is_sealed_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        // TODO(port): DeclarationParser.kt:402
        todo!("DeclarationParser.isSealedToken (DeclarationParser.kt:402)")
    }

    /// `isValueToken(builder, tokenType)` (DeclarationParser.kt:408).
    fn is_value_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        // TODO(port): DeclarationParser.kt:408
        todo!("DeclarationParser.isValueToken (DeclarationParser.kt:408)")
    }

    /// `parseModifierList(builder, modifiers)` with `modifiers` at its default, MODIFIER_BIT_SET (DeclarationParser.kt:415).
    pub fn parse_modifier_list(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_modifier_list_with(builder, MODIFIER_BIT_SET)
    }

    /// `parseModifierList(builder, modifiers)` (DeclarationParser.kt:415).
    pub fn parse_modifier_list_with(&self, builder: &mut SyntaxTreeBuilder<'_>, modifiers: TokenSet) -> Marker {
        // TODO(port): DeclarationParser.kt:415
        todo!("DeclarationParser.parseModifierList (DeclarationParser.kt:415)")
    }

    /// `parseMethodFromLeftParenth(builder, declaration, anno, constructor)` (DeclarationParser.kt:451).
    fn parse_method_from_left_parenth(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        declaration: Marker,
        anno: bool,
        constructor: bool,
    ) -> Marker {
        // TODO(port): DeclarationParser.kt:451
        todo!("DeclarationParser.parseMethodFromLeftParenth (DeclarationParser.kt:451)")
    }

    /// `parseMethodBody(builder, declaration, anno)` (DeclarationParser.kt:471).
    fn parse_method_body(&self, builder: &mut SyntaxTreeBuilder<'_>, declaration: Marker, anno: bool) -> Marker {
        // TODO(port): DeclarationParser.kt:471
        todo!("DeclarationParser.parseMethodBody (DeclarationParser.kt:471)")
    }

    /// `parseParameterList(builder)` (DeclarationParser.kt:509).
    pub fn parse_parameter_list(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): DeclarationParser.kt:509
        todo!("DeclarationParser.parseParameterList (DeclarationParser.kt:509)")
    }

    /// `parseResourceList(builder)` (DeclarationParser.kt:513).
    pub fn parse_resource_list(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): DeclarationParser.kt:513
        todo!("DeclarationParser.parseResourceList (DeclarationParser.kt:513)")
    }

    /// `parseLambdaParameterList(builder, typed)` (DeclarationParser.kt:517).
    pub fn parse_lambda_parameter_list(&self, builder: &mut SyntaxTreeBuilder<'_>, typed: bool) {
        // TODO(port): DeclarationParser.kt:517
        todo!("DeclarationParser.parseLambdaParameterList (DeclarationParser.kt:517)")
    }

    /// `parseElementList(builder, type)` (DeclarationParser.kt:521).
    fn parse_element_list(&self, builder: &mut SyntaxTreeBuilder<'_>, element_type: ListType) {
        // TODO(port): DeclarationParser.kt:521
        todo!("DeclarationParser.parseElementList (DeclarationParser.kt:521)")
    }

    /// `parseParameter(builder, ellipsis, disjunctiveType, varType)` (DeclarationParser.kt:628).
    pub fn parse_parameter(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        ellipsis: bool,
        disjunctive_type: bool,
        var_type: bool,
    ) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:628
        todo!("DeclarationParser.parseParameter (DeclarationParser.kt:628)")
    }

    /// `parseParameterOrRecordComponent(builder, ellipsis, disjunctiveType, varType, isParameter)` (DeclarationParser.kt:637).
    pub fn parse_parameter_or_record_component(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        ellipsis: bool,
        disjunctive_type: bool,
        var_type: bool,
        is_parameter: bool,
    ) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:637
        todo!("DeclarationParser.parseParameterOrRecordComponent (DeclarationParser.kt:637)")
    }

    /// `parseResource(builder)` (DeclarationParser.kt:654).
    pub fn parse_resource(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:654
        todo!("DeclarationParser.parseResource (DeclarationParser.kt:654)")
    }

    /// `parseLambdaParameter(builder, typed)` (DeclarationParser.kt:671).
    pub fn parse_lambda_parameter(&self, builder: &mut SyntaxTreeBuilder<'_>, typed: bool) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:671
        todo!("DeclarationParser.parseLambdaParameter (DeclarationParser.kt:671)")
    }

    /// `parseListElement(builder, typed, typeFlags, type)` (DeclarationParser.kt:683).
    fn parse_list_element(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        typed: bool,
        type_flags: i32,
        element_type: Option<NodeKind>,
    ) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:683
        todo!("DeclarationParser.parseListElement (DeclarationParser.kt:683)")
    }

    /// `parseFieldOrLocalVariable(builder, declaration, declarationStart, context)` (DeclarationParser.kt:747).
    fn parse_field_or_local_variable(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        declaration: Marker,
        declaration_start: i32,
        context: Option<Context>,
    ) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:747
        todo!("DeclarationParser.parseFieldOrLocalVariable (DeclarationParser.kt:747)")
    }

    /// `eatBrackets(builder, errorKey)` (DeclarationParser.kt:835).
    fn eat_brackets(&self, builder: &mut SyntaxTreeBuilder<'_>, error_key: Option<&'static str>) -> bool {
        // TODO(port): DeclarationParser.kt:835
        todo!("DeclarationParser.eatBrackets (DeclarationParser.kt:835)")
    }

    /// `parseAnnotations(builder)` (DeclarationParser.kt:874).
    pub fn parse_annotations(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:874
        todo!("DeclarationParser.parseAnnotations (DeclarationParser.kt:874)")
    }

    /// `parseAnnotation(builder)` (DeclarationParser.kt:885).
    pub fn parse_annotation(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): DeclarationParser.kt:885
        todo!("DeclarationParser.parseAnnotation (DeclarationParser.kt:885)")
    }

    /// `parseAnnotationParameterList(builder)` (DeclarationParser.kt:905).
    fn parse_annotation_parameter_list(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): DeclarationParser.kt:905
        todo!("DeclarationParser.parseAnnotationParameterList (DeclarationParser.kt:905)")
    }

    /// `parseAnnotationElement(builder)` (DeclarationParser.kt:957).
    fn parse_annotation_element(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:957
        todo!("DeclarationParser.parseAnnotationElement (DeclarationParser.kt:957)")
    }

    /// `parseAnnotationValue(builder)` (DeclarationParser.kt:982).
    pub fn parse_annotation_value(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): DeclarationParser.kt:982
        todo!("DeclarationParser.parseAnnotationValue (DeclarationParser.kt:982)")
    }
}

/// `isNonSealedToken(builder, tokenType, level)` (DeclarationParser.kt:998).
pub fn is_non_sealed_token(
    builder: &mut SyntaxTreeBuilder<'_>,
    token_type: Option<TokenKind>,
    level: LanguageLevel,
) -> bool {
    // TODO(port): DeclarationParser.kt:998
    todo!("DeclarationParser.isNonSealedToken (DeclarationParser.kt:998)")
}

/// `ListType`.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum ListType {
    METHOD,
    RESOURCE,
    LAMBDA_TYPED,
    LAMBDA_UNTYPED,
    RECORD_COMPONENTS,
}

impl ListType {
    /// `nodeType`.
    pub(crate) fn node_type(self) -> NodeKind {
        match self {
            ListType::RESOURCE => NodeKind::RESOURCE_LIST,
            ListType::RECORD_COMPONENTS => NodeKind::RECORD_HEADER,
            _ => NodeKind::PARAMETER_LIST,
        }
    }

    /// `stopperTypes`.
    pub(crate) fn stopper_types(self) -> TokenSet {
        match self {
            ListType::METHOD => METHOD_PARAM_LIST_STOPPERS,
            _ => PARAM_LIST_STOPPERS,
        }
    }
}

/// `TYPE_START`.
const TYPE_START: TokenSet = TokenSet::or_set(&[
    PRIMITIVE_TYPE_BIT_SET,
    TokenSet::create(&[TokenKind::IDENTIFIER, TokenKind::AT, TokenKind::VAR_KEYWORD]),
]);

/// `RESOURCE_EXPRESSIONS`.
const RESOURCE_EXPRESSIONS: TokenSet = TokenSet::create_nodes(&[
    NodeKind::REFERENCE_EXPRESSION,
    NodeKind::THIS_EXPRESSION,
    NodeKind::METHOD_CALL_EXPRESSION,
    NodeKind::NEW_EXPRESSION,
]);

/// `BEFORE_LBRACE_ELEMENTS_SET`.
const BEFORE_LBRACE_ELEMENTS_SET: TokenSet = TokenSet::create(&[
    TokenKind::IDENTIFIER,
    TokenKind::COMMA,
    TokenKind::EXTENDS_KEYWORD,
    TokenKind::IMPLEMENTS_KEYWORD,
    TokenKind::LPARENTH,
]);

/// `APPEND_TO_METHOD_SET`.
const APPEND_TO_METHOD_SET: TokenSet =
    TokenSet::create(&[TokenKind::IDENTIFIER, TokenKind::COMMA, TokenKind::THROWS_KEYWORD]);

/// `PARAM_LIST_STOPPERS`.
const PARAM_LIST_STOPPERS: TokenSet = TokenSet::create(&[TokenKind::RPARENTH, TokenKind::LBRACE, TokenKind::ARROW]);

/// `METHOD_PARAM_LIST_STOPPERS`.
const METHOD_PARAM_LIST_STOPPERS: TokenSet =
    TokenSet::create(&[TokenKind::RPARENTH, TokenKind::LBRACE, TokenKind::ARROW, TokenKind::SEMICOLON]);

/// `WHITESPACES`.
const WHITESPACES: &[u8] = b"\n\r \t";
/// `LINE_ENDS`.
const LINE_ENDS: &[u8] = b"\n\r";
