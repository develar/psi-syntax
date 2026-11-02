//! `ReferenceParser.kt`: types, code references, type argument and type parameter lists, reference lists.

use psi_builder::Marker;

use crate::builder::{Nesting, SyntaxTreeBuilder};
use crate::bundle::message;
use crate::language_level::JavaFeature;
use crate::node_kind::NodeKind;
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{empty_element, error, expect_or_error, too_deeply_nested};
use crate::syntax_element_types::PRIMITIVE_TYPE_BIT_SET;
use crate::tokens::{TokenKind, TokenSet};

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

/// `BitUtil.isSet(flags, mask)`.
#[inline]
fn is_set(flags: i32, mask: i32) -> bool {
    (flags & mask) == mask
}

/// `BitUtil.set(value, mask, setBit)`.
#[inline]
fn set(value: i32, mask: i32, set_bit: bool) -> i32 {
    if set_bit { value | mask } else { value & !mask }
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
        let type_info = self.parse_type_info(builder, flags);
        type_info.and_then(|info| info.marker)
    }

    /// `parseTypeInfo(builder, flags)` (ReferenceParser.kt:35).
    pub fn parse_type_info(&self, builder: &mut SyntaxTreeBuilder<'_>, flags: i32) -> Option<TypeInfo> {
        let mut type_info = self.parse_type_info_with(builder, flags, false);

        if let Some(type_info) = &mut type_info {
            // `require(!isSet(flags, DISJUNCTIONS) || !isSet(flags, CONJUNCTIONS))`: no caller sets both.
            debug_assert!(
                !is_set(flags, Self::DISJUNCTIONS) || !is_set(flags, Self::CONJUNCTIONS),
                "don't set both flags simultaneously"
            );
            let operator = if is_set(flags, Self::DISJUNCTIONS) {
                Some(TokenKind::OR)
            } else if is_set(flags, Self::CONJUNCTIONS) {
                Some(TokenKind::AND)
            } else {
                None
            };

            if let (Some(operator), Some(marker)) = (operator, type_info.marker)
                && builder.token_type() == Some(operator)
            {
                let marker = marker.precede(builder);
                type_info.marker = Some(marker);

                while builder.token_type() == Some(operator) {
                    builder.advance_lexer();
                    let token_type = builder.token_type();
                    if token_type != Some(TokenKind::IDENTIFIER) && token_type != Some(TokenKind::AT) {
                        error(builder, message("expected.identifier"));
                    }
                    self.parse_type_info_with(builder, flags, false);
                }

                marker.done(builder, NodeKind::TYPE);
            }
        }

        type_info
    }

    /// `parseTypeInfo(builder, flags, badWildcard)` (ReferenceParser.kt:62).
    ///
    /// Not in the Kotlin: every recursion of a type passes here, so it counts a level of nesting (`Nesting::Level`);
    /// past the limit the type is the error element "Code is too deeply nested", a type with errors.
    fn parse_type_info_with(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        flags: i32,
        bad_wildcard: bool,
    ) -> Option<TypeInfo> {
        builder.token_type()?;
        if !builder.enter_nesting(Nesting::Level) {
            let marker = Some(too_deeply_nested(builder));
            return Some(TypeInfo { marker, has_errors: true, ..TypeInfo::default() });
        }
        let type_info = self.parse_type_info_nested(builder, flags, bad_wildcard);
        builder.exit_nesting(Nesting::Level);
        type_info
    }

    /// The body of `parseTypeInfo(builder, flags, badWildcard)` once a token is there, inside its nesting.
    fn parse_type_info_nested(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        flags: i32,
        bad_wildcard: bool,
    ) -> Option<TypeInfo> {
        let mut type_info = TypeInfo::default();

        let mut element_type = builder.mark();
        let anno = self.my_parser.declaration_parser().parse_annotations(builder);

        let mut token_type = builder.token_type();
        if token_type == Some(TokenKind::IDENTIFIER)
            && is_set(flags, Self::VAR_TYPE)
            && builder.look_ahead(1) != Some(TokenKind::DOT)
            && builder.look_ahead(1) != Some(TokenKind::COLON)
            && builder.token_text() == Some(java_keywords::VAR.as_bytes())
            && JavaFeature::LVTI.is_sufficient(self.my_parser.language_level)
        {
            token_type = Some(TokenKind::VAR_KEYWORD);
            builder.remap_current_token(TokenKind::VAR_KEYWORD);
        } else if token_type == Some(TokenKind::VAR_KEYWORD) && !is_set(flags, Self::VAR_TYPE) {
            token_type = Some(TokenKind::IDENTIFIER);
            builder.remap_current_token(TokenKind::IDENTIFIER);
        }

        if builder.expect_set(PRIMITIVE_TYPE_BIT_SET) {
            type_info.is_primitive = true;
        } else if (is_set(flags, Self::WILDCARD) || bad_wildcard) && token_type == Some(TokenKind::QUEST) {
            builder.advance_lexer();
            self.complete_wildcard_type(builder, is_set(flags, Self::WILDCARD), element_type);
            type_info.marker = Some(element_type);
            return Some(type_info);
        } else if token_type == Some(TokenKind::IDENTIFIER) {
            self.parse_java_code_reference_with(
                builder,
                is_set(flags, Self::EAT_LAST_DOT),
                true,
                false,
                false,
                false,
                is_set(flags, Self::DIAMONDS),
                &mut type_info,
            );
        } else if token_type == Some(TokenKind::VAR_KEYWORD) {
            builder.advance_lexer();
            element_type.done(builder, NodeKind::TYPE);
            type_info.marker = Some(element_type);
            return Some(type_info);
        } else if is_set(flags, Self::DIAMONDS) && token_type == Some(TokenKind::GT) {
            if anno.is_none() {
                empty_element(builder, NodeKind::DIAMOND_TYPE);
            } else {
                error(builder, message("expected.identifier"));
                type_info.has_errors = true;
            }
            element_type.done(builder, NodeKind::TYPE);
            type_info.marker = Some(element_type);
            return Some(type_info);
        } else {
            element_type.drop(builder);
            if anno.is_some() && is_set(flags, Self::INCOMPLETE_ANNO) {
                error(builder, message("expected.type"));
                type_info.marker = anno;
                type_info.has_errors = true;
                return Some(type_info);
            }
            return None;
        }

        element_type.done(builder, NodeKind::TYPE);
        loop {
            self.my_parser.declaration_parser().parse_annotations(builder);

            let bracket = builder.mark();
            if !builder.expect(Some(TokenKind::LBRACKET)) {
                bracket.drop(builder);
                break;
            }
            if !builder.expect(Some(TokenKind::RBRACKET)) {
                bracket.rollback_to(builder);
                break;
            }
            bracket.drop(builder);
            type_info.is_array = true;
        }

        if is_set(flags, Self::ELLIPSIS) && builder.token_type() == Some(TokenKind::ELLIPSIS) {
            builder.advance_lexer();
            type_info.is_var_arg = true;
        }

        if type_info.is_var_arg || type_info.is_array {
            element_type = element_type.precede(builder);
            element_type.done(builder, NodeKind::TYPE);
        }

        type_info.marker = Some(element_type);
        Some(type_info)
    }

    /// `completeWildcardType(builder, wildcard, type)` (ReferenceParser.kt:164).
    fn complete_wildcard_type(&self, builder: &mut SyntaxTreeBuilder<'_>, wildcard: bool, element_type: Marker) {
        if builder.expect_set(WILDCARD_KEYWORD_SET) && self.parse_type_info(builder, Self::EAT_LAST_DOT).is_none() {
            error(builder, message("expected.type"));
        }

        if wildcard {
            element_type.done(builder, NodeKind::TYPE);
        } else {
            element_type.error(builder, message("error.message.wildcard.not.expected"));
        }
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
        self.parse_java_code_reference_with(
            builder,
            eat_last_dot,
            parameter_list,
            false,
            false,
            is_new,
            diamonds,
            &mut TypeInfo::default(),
        )
    }

    /// `parseImportCodeReference(builder, isStatic)` (ReferenceParser.kt:197).
    pub fn parse_import_code_reference(&self, builder: &mut SyntaxTreeBuilder<'_>, is_static: bool) -> bool {
        let mut type_info = TypeInfo::default();
        self.parse_java_code_reference_with(builder, true, false, true, is_static, false, false, &mut type_info);
        !type_info.has_errors
    }

    /// `parseJavaCodeReference(builder, eatLastDot, parameterList, isImport, isStaticImport, isNew, diamonds, typeInfo)` (ReferenceParser.kt:211).
    #[allow(clippy::too_many_arguments)] // the Kotlin method's parameters
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
        let mut ref_element = builder.mark();

        self.my_parser.declaration_parser().parse_annotations(builder);

        if !builder.expect(Some(TokenKind::IDENTIFIER)) {
            ref_element.rollback_to(builder);
            if is_import {
                error(builder, message("expected.identifier"));
            }
            type_info.has_errors = true;
            return None;
        }

        if parameter_list {
            type_info.is_parameterized = self.parse_reference_parameter_list(builder, true, diamonds);
        } else if !is_static_import {
            empty_element(builder, NodeKind::REFERENCE_PARAMETER_LIST);
        }

        while builder.token_type() == Some(TokenKind::DOT) {
            ref_element.done(builder, NodeKind::JAVA_CODE_REFERENCE);

            if is_new && !diamonds && type_info.is_parameterized {
                return Some(ref_element);
            }

            let dot_pos = builder.mark();
            builder.advance_lexer();

            self.my_parser.declaration_parser().parse_annotations(builder);

            if is_import && builder.expect(Some(TokenKind::ASTERISK)) {
                dot_pos.drop(builder);
                return Some(ref_element);
            } else if !builder.expect(Some(TokenKind::IDENTIFIER)) {
                if !eat_last_dot {
                    dot_pos.rollback_to(builder);
                    return Some(ref_element);
                }

                type_info.has_errors = true;
                if is_import {
                    error(builder, message("import.statement.identifier.or.asterisk.expected."));
                } else {
                    error(builder, message("expected.identifier"));
                }
                dot_pos.drop(builder);
                return Some(ref_element);
            }
            dot_pos.drop(builder);

            ref_element = ref_element.precede(builder);

            if parameter_list {
                type_info.is_parameterized = self.parse_reference_parameter_list(builder, true, diamonds);
            } else {
                empty_element(builder, NodeKind::REFERENCE_PARAMETER_LIST);
            }
        }

        if is_static_import {
            ref_element.done(builder, NodeKind::IMPORT_STATIC_REFERENCE);
        } else {
            ref_element.done(builder, NodeKind::JAVA_CODE_REFERENCE);
        }
        Some(ref_element)
    }

    /// `parseReferenceParameterList(builder, wildcard, diamonds)` (ReferenceParser.kt:294).
    ///
    /// Not in the Kotlin: a list counts as `Nesting::TypeArguments` (an expression `a < b < …` tries a type with
    /// type arguments at every `a` and rolls it back); past a limit its `<` and the rest is the error element "Code is
    /// too deeply nested" in the list, which is not ok.
    pub fn parse_reference_parameter_list(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        wildcard: bool,
        diamonds: bool,
    ) -> bool {
        let list = builder.mark();
        if builder.token_type() != Some(TokenKind::LT) {
            list.done(builder, NodeKind::REFERENCE_PARAMETER_LIST);
            return false;
        }
        if !builder.enter_nesting(Nesting::TypeArguments) {
            too_deeply_nested(builder);
            list.done(builder, NodeKind::REFERENCE_PARAMETER_LIST);
            return false;
        }
        let is_ok = self.parse_reference_parameter_list_nested(builder, list, wildcard, diamonds);
        builder.exit_nesting(Nesting::TypeArguments);
        is_ok
    }

    /// The rest of `parseReferenceParameterList(builder, wildcard, diamonds)` at its `<`, inside its nesting.
    fn parse_reference_parameter_list_nested(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        list: Marker,
        wildcard: bool,
        diamonds: bool,
    ) -> bool {
        builder.advance_lexer();

        let mut flags = set(set(Self::EAT_LAST_DOT, Self::WILDCARD, wildcard), Self::DIAMONDS, diamonds);
        let mut is_ok = true;
        loop {
            if self.parse_type_info_with(builder, flags, true).is_none() {
                error(builder, message("expected.identifier"));
            } else {
                let token_type = builder.token_type();
                if WILDCARD_KEYWORD_SET.contains(token_type) {
                    self.parse_reference_list(builder, token_type, None, Some(TokenKind::AND));
                }
            }

            if builder.expect(Some(TokenKind::GT)) {
                break;
            } else if !expect_or_error(builder, Some(TokenKind::COMMA), "expected.gt.or.comma") {
                is_ok = false;
                break;
            }
            flags = set(flags, Self::DIAMONDS, false);
        }

        list.done(builder, NodeKind::REFERENCE_PARAMETER_LIST);
        is_ok
    }

    /// `parseTypeParameters(builder)` (ReferenceParser.kt:328).
    pub fn parse_type_parameters(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let list = builder.mark();
        if !builder.expect(Some(TokenKind::LT)) {
            list.done(builder, NodeKind::TYPE_PARAMETER_LIST);
            return list;
        }

        loop {
            let param = self.parse_type_parameter(builder);
            if param.is_none() {
                error(builder, message("expected.type.parameter"));
            }
            if !builder.expect(Some(TokenKind::COMMA)) {
                break;
            }
        }

        if !builder.expect(Some(TokenKind::GT)) {
            // hack for completion
            if builder.token_type() == Some(TokenKind::IDENTIFIER) {
                if builder.look_ahead(1) == Some(TokenKind::GT) {
                    let error_element = builder.mark();
                    builder.advance_lexer();
                    error_element.error(builder, message("unexpected.identifier"));
                    builder.advance_lexer();
                } else {
                    error(builder, message("expected.gt"));
                }
            } else {
                error(builder, message("expected.gt"));
            }
        }

        list.done(builder, NodeKind::TYPE_PARAMETER_LIST);
        list
    }

    /// `parseTypeParameter(builder)` (ReferenceParser.kt:365).
    pub fn parse_type_parameter(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let param = builder.mark();

        self.my_parser.declaration_parser().parse_annotations(builder);

        let wild = builder.expect(Some(TokenKind::QUEST));
        if !wild && !builder.expect(Some(TokenKind::IDENTIFIER)) {
            param.rollback_to(builder);
            return None;
        }

        self.parse_reference_list(
            builder,
            Some(TokenKind::EXTENDS_KEYWORD),
            Some(NodeKind::EXTENDS_BOUND_LIST),
            Some(TokenKind::AND),
        );

        if !wild {
            param.done(builder, NodeKind::TYPE_PARAMETER);
        } else {
            param.error(builder, message("error.message.wildcard.not.expected"));
        }
        Some(param)
    }

    /// `parseReferenceList(builder, start, type, delimiter)` (ReferenceParser.kt:387).
    pub fn parse_reference_list(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        start: Option<TokenKind>,
        element_type: Option<NodeKind>,
        delimiter: Option<TokenKind>,
    ) -> bool {
        let element = builder.mark();

        let mut ends_with_error = false;
        if builder.expect(start) {
            loop {
                ends_with_error = false;
                let class_reference = self.parse_java_code_reference(builder, false, true, false, false);
                if class_reference.is_none() {
                    error(builder, message("expected.identifier"));
                    ends_with_error = true;
                }
                if !builder.expect(delimiter) {
                    break;
                }
            }
        }

        match element_type {
            Some(element_type) => element.done(builder, element_type),
            None => element.error(builder, message("bound.not.expected")),
        }
        ends_with_error
    }
}

/// `WILDCARD_KEYWORD_SET`.
const WILDCARD_KEYWORD_SET: TokenSet = TokenSet::create(&[TokenKind::EXTENDS_KEYWORD, TokenKind::SUPER_KEYWORD]);
