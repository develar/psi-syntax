//! `ReferenceParser.kt`: types, code references, type argument and type parameter lists, reference lists.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `ReferenceParser.TypeInfo`: what `parseTypeInfo` learned of a type.
#[derive(Clone, Debug, Default)]
pub struct TypeInfo {
    pub is_primitive: bool,
    pub is_parameterized: bool,
    pub is_array: bool,
    pub is_var_arg: bool,
    pub has_errors: bool,
    /// `lateinit var marker`: set before `parseTypeInfo` returns the info.
    pub marker: Option<Marker>,
}

/// `ReferenceParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::reference_parser`.
#[derive(Copy, Clone)]
pub struct ReferenceParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> ReferenceParser<'p> {
    /// `EAT_LAST_DOT`.
    pub const EAT_LAST_DOT: i32 = 0x01;
    /// `ELLIPSIS`.
    pub const ELLIPSIS: i32 = 0x02;
    /// `WILDCARD`.
    pub const WILDCARD: i32 = 0x04;
    /// `DIAMONDS`.
    pub const DIAMONDS: i32 = 0x08;
    /// `DISJUNCTIONS`.
    pub const DISJUNCTIONS: i32 = 0x10;
    /// `CONJUNCTIONS`.
    pub const CONJUNCTIONS: i32 = 0x20;
    /// `INCOMPLETE_ANNO`.
    pub const INCOMPLETE_ANNO: i32 = 0x40;
    /// `VAR_TYPE`.
    pub const VAR_TYPE: i32 = 0x80;

    /// `parseType(builder, flags)` (ReferenceParser.kt:30).
    pub fn parse_type(&self, builder: &mut SyntaxTreeBuilder<'_>, flags: i32) -> Option<Marker> {
        // TODO(port): ReferenceParser.kt:30
        todo!("ReferenceParser.parseType (ReferenceParser.kt:30)")
    }

    /// `parseTypeInfo(builder, flags)` (ReferenceParser.kt:35).
    pub fn parse_type_info(&self, builder: &mut SyntaxTreeBuilder<'_>, flags: i32) -> Option<TypeInfo> {
        // TODO(port): ReferenceParser.kt:35
        todo!("ReferenceParser.parseTypeInfo (ReferenceParser.kt:35)")
    }

    /// `parseTypeInfo(builder, flags, badWildcard)` (ReferenceParser.kt:62).
    fn parse_type_info_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        flags: i32,
        bad_wildcard: bool,
    ) -> Option<TypeInfo> {
        // TODO(port): ReferenceParser.kt:62
        todo!("ReferenceParser.parseTypeInfo (ReferenceParser.kt:62)")
    }

    /// `completeWildcardType(builder, wildcard, type)` (ReferenceParser.kt:164).
    fn complete_wildcard_type(&self, builder: &mut SyntaxTreeBuilder<'_>, wildcard: bool, element_type: Marker) {
        // TODO(port): ReferenceParser.kt:164
        todo!("ReferenceParser.completeWildcardType (ReferenceParser.kt:164)")
    }

    /// `parseJavaCodeReference(builder, eatLastDot, parameterList, isNew, diamonds)` (ReferenceParser.kt:179).
    pub fn parse_java_code_reference(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        eat_last_dot: bool,
        parameter_list: bool,
        is_new: bool,
        diamonds: bool,
    ) -> Option<Marker> {
        // TODO(port): ReferenceParser.kt:179
        todo!("ReferenceParser.parseJavaCodeReference (ReferenceParser.kt:179)")
    }

    /// `parseImportCodeReference(builder, isStatic)` (ReferenceParser.kt:197).
    pub fn parse_import_code_reference(&self, builder: &mut SyntaxTreeBuilder<'_>, is_static: bool) -> bool {
        // TODO(port): ReferenceParser.kt:197
        todo!("ReferenceParser.parseImportCodeReference (ReferenceParser.kt:197)")
    }

    /// `parseJavaCodeReference(builder, eatLastDot, parameterList, isImport, isStaticImport, isNew, diamonds, typeInfo)` (ReferenceParser.kt:211).
    fn parse_java_code_reference_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        eat_last_dot: bool,
        parameter_list: bool,
        is_import: bool,
        is_static_import: bool,
        is_new: bool,
        diamonds: bool,
        type_info: &mut TypeInfo,
    ) -> Option<Marker> {
        // TODO(port): ReferenceParser.kt:211
        todo!("ReferenceParser.parseJavaCodeReference (ReferenceParser.kt:211)")
    }

    /// `parseReferenceParameterList(builder, wildcard, diamonds)` (ReferenceParser.kt:294).
    pub fn parse_reference_parameter_list(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        wildcard: bool,
        diamonds: bool,
    ) -> bool {
        // TODO(port): ReferenceParser.kt:294
        todo!("ReferenceParser.parseReferenceParameterList (ReferenceParser.kt:294)")
    }

    /// `parseTypeParameters(builder)` (ReferenceParser.kt:328).
    pub fn parse_type_parameters(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): ReferenceParser.kt:328
        todo!("ReferenceParser.parseTypeParameters (ReferenceParser.kt:328)")
    }

    /// `parseTypeParameter(builder)` (ReferenceParser.kt:365).
    pub fn parse_type_parameter(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): ReferenceParser.kt:365
        todo!("ReferenceParser.parseTypeParameter (ReferenceParser.kt:365)")
    }

    /// `parseReferenceList(builder, start, type, delimiter)` (ReferenceParser.kt:387).
    pub fn parse_reference_list(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        start: Option<TokenKind>,
        element_type: Option<NodeKind>,
        delimiter: Option<TokenKind>,
    ) -> bool {
        // TODO(port): ReferenceParser.kt:387
        todo!("ReferenceParser.parseReferenceList (ReferenceParser.kt:387)")
    }
}

/// `WILDCARD_KEYWORD_SET`.
const WILDCARD_KEYWORD_SET: TokenSet = TokenSet::create(&[TokenKind::EXTENDS_KEYWORD, TokenKind::SUPER_KEYWORD]);
