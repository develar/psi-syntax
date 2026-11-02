//! `DeclarationParser.kt`: classes, enums, records, members, modifier lists, parameter lists, annotations.

use psi_builder::Marker;

use crate::builder::{Nesting, SyntaxTreeBuilder};
use crate::bundle::message;
use crate::language_level::{JavaFeature, LanguageLevel};
use crate::node_kind::NodeKind;
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{
    self, done, empty_element, empty_element_before, error, error_with, expect_or_error, too_deeply_nested,
};
use crate::parser::reference_parser::ReferenceParser;
use crate::syntax_element_types::{CLASS_KEYWORD_BIT_SET, KEYWORD_BIT_SET, MODIFIER_BIT_SET, PRIMITIVE_TYPE_BIT_SET};
use crate::tokens::{TokenKind, TokenSet};

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

    /// `parseClassBodyWithBraces(builder, isAnnotation, isEnum)` (DeclarationParser.kt:37). The Kotlin `require`s the
    /// current token to be `{` (every caller checks it; the class-body test entries are run only on bodies that have
    /// one): without it the port parses nothing.
    pub fn parse_class_body_with_braces(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        is_annotation: bool,
        is_enum: bool,
    ) {
        if builder.token_type() != Some(TokenKind::LBRACE) {
            return;
        }
        builder.advance_lexer();

        let mut builder_wrapper = java_parser_util::brace_matching_builder(builder);
        if is_enum {
            self.parse_enum_constants(&mut builder_wrapper);
        }
        self.parse_class_body_declarations(&mut builder_wrapper, is_annotation);

        expect_or_error(builder, Some(TokenKind::RBRACE), "expected.rbrace");
    }

    /// `parseClassFromKeyword(builder, declaration, isAnnotation, context)` (DeclarationParser.kt:50).
    fn parse_class_from_keyword(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        declaration: Marker,
        is_annotation: bool,
        context: Option<Context>,
    ) -> Option<Marker> {
        let mut keyword_token_type = builder.token_type();
        let is_record = self.is_record_token(builder, keyword_token_type);
        if is_record {
            builder.remap_current_token(TokenKind::RECORD_KEYWORD);
            if builder.look_ahead(1) != Some(TokenKind::IDENTIFIER) {
                builder.advance_lexer();
                error(builder, message("expected.identifier"));
                declaration.drop(builder);
                return None;
            }
            let after_ident = builder.look_ahead(2);
            // No parser recovery for local records without < or ( to support light stubs
            // (look at com.intellij.psi.impl.source.JavaLightStubBuilder.CodeBlockVisitor.visit)
            if context == Some(Context::CODE_BLOCK)
                && after_ident != Some(TokenKind::LPARENTH)
                && after_ident != Some(TokenKind::LT)
            {
                // skipping record kw and identifier
                builder.advance(2);
                error(builder, message("expected.lt.or.lparen"));
                declaration.drop(builder);
                return None;
            }
            keyword_token_type = Some(TokenKind::RECORD_KEYWORD);
        }
        // `require(CLASS_KEYWORD_BIT_SET.contains(keywordTokenType))`: both callers check it.
        debug_assert!(CLASS_KEYWORD_BIT_SET.contains(keyword_token_type), "{keyword_token_type:?}");
        builder.advance_lexer();
        let is_enum = keyword_token_type == Some(TokenKind::ENUM_KEYWORD);

        if !builder.expect(Some(TokenKind::IDENTIFIER)) {
            error(builder, message("expected.identifier"));
            declaration.drop(builder);
            return None;
        }

        let ref_parser = self.my_parser.reference_parser();
        ref_parser.parse_type_parameters(builder);

        if builder.token_type() == Some(TokenKind::LPARENTH) {
            self.parse_element_list(builder, ListType::RECORD_COMPONENTS);
        }

        ref_parser.parse_reference_list(
            builder,
            Some(TokenKind::EXTENDS_KEYWORD),
            Some(NodeKind::EXTENDS_LIST),
            Some(TokenKind::COMMA),
        );
        ref_parser.parse_reference_list(
            builder,
            Some(TokenKind::IMPLEMENTS_KEYWORD),
            Some(NodeKind::IMPLEMENTS_LIST),
            Some(TokenKind::COMMA),
        );

        if builder.token_type() == Some(TokenKind::IDENTIFIER)
            && builder.token_text() == Some(java_keywords::PERMITS.as_bytes())
        {
            builder.remap_current_token(TokenKind::PERMITS_KEYWORD);
        }
        if builder.token_type() == Some(TokenKind::PERMITS_KEYWORD) {
            ref_parser.parse_reference_list(
                builder,
                Some(TokenKind::PERMITS_KEYWORD),
                Some(NodeKind::PERMITS_LIST),
                Some(TokenKind::COMMA),
            );
        }

        if builder.token_type() != Some(TokenKind::LBRACE) {
            let error = builder.mark();
            while BEFORE_LBRACE_ELEMENTS_SET.contains(builder.token_type()) {
                builder.advance_lexer();
            }
            error.error(builder, message("expected.lbrace"));
        }

        if builder.token_type() == Some(TokenKind::LBRACE) {
            self.parse_class_body_with_braces(builder, is_annotation, is_enum);
        }

        done(builder, declaration, NodeKind::CLASS, self.language_level());
        Some(declaration)
    }

    /// `parseEnumConstants(builder)` (DeclarationParser.kt:121).
    fn parse_enum_constants(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let mut first = true;
        while builder.token_type().is_some() {
            if builder.expect(Some(TokenKind::SEMICOLON)) {
                return;
            }

            if builder.token_type() == Some(TokenKind::PRIVATE_KEYWORD)
                || builder.token_type() == Some(TokenKind::PROTECTED_KEYWORD)
            {
                error(builder, message("expected.semicolon"));
                return;
            }

            let enum_constant = self.parse_enum_constant(builder);
            if enum_constant.is_none() && builder.token_type() == Some(TokenKind::COMMA) && first {
                let next = builder.look_ahead(1);
                if next != Some(TokenKind::SEMICOLON) && next != Some(TokenKind::RBRACE) {
                    error(builder, message("expected.identifier"));
                }
            }

            first = false;

            let mut comma_count = 0;
            while builder.token_type() == Some(TokenKind::COMMA) {
                if comma_count > 0 {
                    error(builder, message("expected.identifier"));
                }
                builder.advance_lexer();
                comma_count += 1;
            }
            if comma_count == 0 && builder.token_type().is_some() && builder.token_type() != Some(TokenKind::SEMICOLON)
            {
                error(builder, message("expected.comma.or.semicolon"));
                return;
            }
        }
    }

    /// `parseEnumConstant(builder)` (DeclarationParser.kt:158).
    pub fn parse_enum_constant(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let constant = builder.mark();

        self.parse_modifier_list(builder);

        if builder.expect(Some(TokenKind::IDENTIFIER)) {
            if builder.token_type() == Some(TokenKind::LPARENTH) {
                self.my_parser.expression_parser().parse_argument_list(builder);
            } else {
                empty_element(builder, NodeKind::EXPRESSION_LIST);
            }

            if builder.token_type() == Some(TokenKind::LBRACE) {
                let constant_init = builder.mark();
                self.parse_class_body_with_braces(builder, false, false);
                done(builder, constant_init, NodeKind::ENUM_CONSTANT_INITIALIZER, self.language_level());
            }

            done(builder, constant, NodeKind::ENUM_CONSTANT, self.language_level());
            Some(constant)
        } else {
            constant.rollback_to(builder);
            None
        }
    }

    /// `parseClassBodyDeclarations(builder, isAnnotation)` (DeclarationParser.kt:186).
    pub fn parse_class_body_declarations(&self, builder: &mut SyntaxTreeBuilder<'_>, is_annotation: bool) {
        let context = if is_annotation { Context::ANNOTATION_INTERFACE } else { Context::CLASS };

        let mut invalid_elements: Option<Marker> = None;
        loop {
            let token_type = builder.token_type();
            if token_type.is_none() || token_type == Some(TokenKind::RBRACE) {
                break;
            }

            if token_type == Some(TokenKind::SEMICOLON) {
                if let Some(invalid) = invalid_elements.take() {
                    invalid.error(builder, message("unexpected.token"));
                }
                builder.advance_lexer();
                continue;
            }

            let declaration = self.parse(builder, Some(context));
            if let Some(declaration) = declaration {
                if let Some(invalid) = invalid_elements.take() {
                    invalid.error_before(builder, message("unexpected.token"), declaration);
                }
                continue;
            }

            if invalid_elements.is_none() {
                invalid_elements = Some(builder.mark());
            }

            // adding a reference, not simple tokens allows "Browse ..." to work well
            let reference =
                self.my_parser.reference_parser().parse_java_code_reference(builder, true, true, false, false);
            if reference.is_none() {
                builder.advance_lexer();
            }
        }

        if let Some(invalid) = invalid_elements {
            invalid.error(builder, message("unexpected.token"));
        }
    }

    /// `parse(builder, context)` (DeclarationParser.kt:226).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>, context: Option<Context>) -> Option<Marker> {
        let token_type = builder.token_type()?;

        if token_type == TokenKind::LBRACE {
            if context == Some(Context::FILE) || context == Some(Context::CODE_BLOCK) {
                return None;
            }
        } else if !self.is_record_token(builder, Some(token_type))
            && !self.is_sealed_token(builder, Some(token_type))
            && !is_non_sealed_token(builder, Some(token_type), self.language_level())
            && (!TYPE_START.contains(token_type) || token_type == TokenKind::AT)
            && !MODIFIER_BIT_SET.contains(token_type)
            && !CLASS_KEYWORD_BIT_SET.contains(token_type)
            && token_type != TokenKind::AT
            && (context == Some(Context::CODE_BLOCK) || token_type != TokenKind::LT)
        {
            return None;
        }

        // Not in the Kotlin: a declaration (class members in class bodies, local and anonymous classes) counts a level
        // of nesting; past the limit it is the error element "Code is too deeply nested".
        if !builder.enter_nesting(Nesting::Level) {
            return Some(too_deeply_nested(builder));
        }
        let declaration = self.parse_nested(builder, context);
        builder.exit_nesting(Nesting::Level);
        declaration
    }

    /// The rest of `parse(builder, context)` once the current token may start a declaration, inside its nesting.
    fn parse_nested(&self, builder: &mut SyntaxTreeBuilder<'_>, context: Option<Context>) -> Option<Marker> {
        let declaration = builder.mark();
        let declaration_start = builder.current_offset();
        let mod_list = self.parse_modifier_list(builder);

        if builder.expect(Some(TokenKind::AT)) {
            if builder.token_type() == Some(TokenKind::INTERFACE_KEYWORD) {
                return self.parse_class_from_keyword(builder, declaration, true, context).or(Some(mod_list));
            } else {
                declaration.rollback_to(builder);
                return None;
            }
        }

        let token_type = builder.token_type();
        if CLASS_KEYWORD_BIT_SET.contains(token_type) || self.is_record_token(builder, token_type) {
            return self.parse_class_from_keyword(builder, declaration, false, context).or(Some(mod_list));
        }

        let type_params = if builder.token_type() == Some(TokenKind::LT) && context != Some(Context::CODE_BLOCK) {
            Some(self.my_parser.reference_parser().parse_type_parameters(builder))
        } else {
            None
        };

        if builder.token_type() == Some(TokenKind::LBRACE) {
            if context == Some(Context::CODE_BLOCK) {
                error_with(builder, message("expected.identifier.or.type"), None);
                declaration.drop(builder);
                return Some(mod_list);
            }

            // `checkNotNull`: a code block is parsed at `{`.
            let code_block = self.my_parser.statement_parser().parse_code_block(builder);
            if let (Some(type_params), Some(code_block)) = (type_params, code_block) {
                let error = type_params.precede(builder);
                error.error_before(builder, message("unexpected.token"), code_block);
            }

            done(builder, declaration, NodeKind::CLASS_INITIALIZER, self.language_level());
            return Some(declaration);
        }

        let mut element_type: Option<crate::parser::reference_parser::TypeInfo> = None;

        if TYPE_START.contains(builder.token_type()) {
            let pos = builder.mark();

            let mut flags = ReferenceParser::EAT_LAST_DOT | ReferenceParser::WILDCARD;
            if context == Some(Context::CODE_BLOCK) || context == Some(Context::JSHELL) {
                flags |= ReferenceParser::VAR_TYPE;
            }
            element_type = self.my_parser.reference_parser().parse_type_info(builder, flags);

            if element_type.is_none() {
                pos.rollback_to(builder);
            } else if builder.token_type() == Some(TokenKind::LPARENTH)
                || builder.token_type() == Some(TokenKind::LBRACE)
                || builder.token_type() == Some(TokenKind::THROWS_KEYWORD)
            {
                // constructor
                if context == Some(Context::CODE_BLOCK) {
                    declaration.rollback_to(builder);
                    return None;
                }

                pos.rollback_to(builder);
                if type_params.is_none() {
                    empty_element(builder, NodeKind::TYPE_PARAMETER_LIST);
                }
                self.parse_annotations(builder);

                if !builder.expect(Some(TokenKind::IDENTIFIER)) {
                    let primitive = builder.mark();
                    builder.advance_lexer();
                    primitive.error(builder, message("expected.identifier"));
                }

                match builder.token_type() {
                    Some(TokenKind::LPARENTH) => {
                        return Some(self.parse_method_from_left_parenth(builder, declaration, false, true));
                    }
                    Some(TokenKind::LBRACE) => {
                        // compact constructor
                        empty_element(builder, NodeKind::THROWS_LIST);
                        return Some(self.parse_method_body(builder, declaration, false));
                    }
                    Some(TokenKind::THROWS_KEYWORD) => {
                        self.my_parser.reference_parser().parse_reference_list(
                            builder,
                            Some(TokenKind::THROWS_KEYWORD),
                            Some(NodeKind::THROWS_LIST),
                            Some(TokenKind::COMMA),
                        );
                        return Some(self.parse_method_body(builder, declaration, false));
                    }
                    _ => {
                        declaration.rollback_to(builder);
                        return None;
                    }
                }
            } else {
                pos.drop(builder);
            }
        }

        let Some(element_type) = element_type else {
            let error = match type_params {
                Some(type_params) => type_params.precede(builder),
                None => builder.mark(),
            };
            error.error(builder, message("expected.identifier.or.type"));
            declaration.drop(builder);
            return Some(mod_list);
        };

        if !builder.expect(Some(TokenKind::IDENTIFIER)) {
            if context != Some(Context::CODE_BLOCK)
                || builder.marker_start_offset(mod_list) != builder.marker_end_offset(mod_list)
                || (element_type.is_primitive && builder.token_type() != Some(TokenKind::DOT))
            {
                if let (Some(type_params), Some(type_marker)) = (type_params, element_type.marker) {
                    type_params.precede(builder).error_before(builder, message("unexpected.token"), type_marker);
                }
                builder.error(message("expected.identifier"));
                declaration.drop(builder);
                return Some(mod_list);
            } else {
                declaration.rollback_to(builder);
                return None;
            }
        }

        if builder.token_type() == Some(TokenKind::LPARENTH)
            && matches!(context, Some(Context::CLASS | Context::ANNOTATION_INTERFACE | Context::FILE | Context::JSHELL))
        {
            // method
            if type_params.is_none()
                && let Some(type_marker) = element_type.marker
            {
                empty_element_before(builder, type_marker, NodeKind::TYPE_PARAMETER_LIST);
            }
            return Some(self.parse_method_from_left_parenth(
                builder,
                declaration,
                context == Some(Context::ANNOTATION_INTERFACE),
                false,
            ));
        }

        if let (Some(type_params), Some(type_marker)) = (type_params, element_type.marker) {
            type_params.precede(builder).error_before(builder, message("unexpected.token"), type_marker);
        }
        self.parse_field_or_local_variable(builder, declaration, declaration_start, context)
    }

    /// `isRecordToken(builder, tokenType)` (DeclarationParser.kt:383).
    pub fn is_record_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        if token_type == Some(TokenKind::IDENTIFIER) && builder.token_text() == Some(java_keywords::RECORD.as_bytes()) {
            let next_token = builder.look_ahead(1);
            if next_token == Some(TokenKind::IDENTIFIER)
                // The following tokens cannot be part of a valid record declaration,
                // but we assume it to be a malformed record, rather than a malformed type.
                || MODIFIER_BIT_SET.contains(next_token)
                || CLASS_KEYWORD_BIT_SET.contains(next_token)
                || TYPE_START.contains(next_token)
                || next_token == Some(TokenKind::AT)
                || next_token == Some(TokenKind::LBRACE)
                || next_token == Some(TokenKind::RBRACE)
            {
                let level = self.language_level();
                return JavaFeature::RECORDS.is_sufficient(level);
            }
        }
        false
    }

    /// `isSealedToken(builder, tokenType)` (DeclarationParser.kt:402).
    fn is_sealed_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        JavaFeature::SEALED_CLASSES.is_sufficient(self.language_level())
            && token_type == Some(TokenKind::IDENTIFIER)
            && builder.token_text() == Some(java_keywords::SEALED.as_bytes())
    }

    /// `isValueToken(builder, tokenType)` (DeclarationParser.kt:408).
    fn is_value_token(&self, builder: &mut SyntaxTreeBuilder<'_>, token_type: Option<TokenKind>) -> bool {
        JavaFeature::VALHALLA_VALUE_CLASSES.is_sufficient(self.language_level())
            && token_type == Some(TokenKind::IDENTIFIER)
            && builder.token_text() == Some(java_keywords::VALUE.as_bytes())
    }

    /// `parseModifierList(builder, modifiers)` with `modifiers` at its default, MODIFIER_BIT_SET (DeclarationParser.kt:415).
    pub fn parse_modifier_list(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_modifier_list_with(builder, MODIFIER_BIT_SET)
    }

    /// `parseModifierList(builder, modifiers)` (DeclarationParser.kt:415).
    pub fn parse_modifier_list_with(&self, builder: &mut SyntaxTreeBuilder<'_>, modifiers: TokenSet) -> Marker {
        let mod_list = builder.mark();

        while let Some(mut token_type) = builder.token_type() {
            if self.is_value_token(builder, Some(token_type)) && modifiers.contains(TokenKind::VALUE_KEYWORD) {
                builder.remap_current_token(TokenKind::VALUE_KEYWORD);
                token_type = TokenKind::VALUE_KEYWORD;
            } else if self.is_sealed_token(builder, Some(token_type)) && modifiers.contains(TokenKind::SEALED_KEYWORD) {
                builder.remap_current_token(TokenKind::SEALED_KEYWORD);
                token_type = TokenKind::SEALED_KEYWORD;
            }
            if is_non_sealed_token(builder, Some(token_type), self.language_level()) {
                let non_sealed = builder.mark();
                builder.advance(3);
                non_sealed.collapse(builder, TokenKind::NON_SEALED_KEYWORD);
            } else if modifiers.contains(token_type) {
                builder.advance_lexer();
            } else if token_type == TokenKind::AT {
                if KEYWORD_BIT_SET.contains(builder.look_ahead(1)) {
                    break;
                }
                self.parse_annotation(builder);
            } else {
                break;
            }
        }

        done(builder, mod_list, NodeKind::MODIFIER_LIST, self.language_level());
        mod_list
    }

    /// `parseMethodFromLeftParenth(builder, declaration, anno, constructor)` (DeclarationParser.kt:451).
    fn parse_method_from_left_parenth(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        declaration: Marker,
        anno: bool,
        constructor: bool,
    ) -> Marker {
        self.parse_parameter_list(builder);

        self.eat_brackets(builder, if constructor { Some("expected.lbrace") } else { None });

        self.my_parser.reference_parser().parse_reference_list(
            builder,
            Some(TokenKind::THROWS_KEYWORD),
            Some(NodeKind::THROWS_LIST),
            Some(TokenKind::COMMA),
        );

        if anno && builder.expect(Some(TokenKind::DEFAULT_KEYWORD)) && self.parse_annotation_value(builder).is_none() {
            error(builder, message("expected.value"));
        }

        self.parse_method_body(builder, declaration, anno)
    }

    /// `parseMethodBody(builder, declaration, anno)` (DeclarationParser.kt:471).
    fn parse_method_body(&self, builder: &mut SyntaxTreeBuilder<'_>, declaration: Marker, anno: bool) -> Marker {
        let token_type = builder.token_type();
        if token_type != Some(TokenKind::SEMICOLON) && token_type != Some(TokenKind::LBRACE) {
            let error = builder.mark();
            // heuristic: going to next line obviously means method signature is over, starting new method (actually, another one completion hack)
            'outer: loop {
                let current_offset = builder.current_offset();
                let text = builder.text();
                for i in (0..current_offset.max(0) as usize).rev() {
                    let ch = text.get(i).copied().unwrap_or(0);
                    if ch == b'\n' {
                        break 'outer;
                    } else if ch != b' ' && ch != b'\t' {
                        break;
                    }
                }
                if !builder.expect_set(APPEND_TO_METHOD_SET) {
                    break;
                }
            }
            error.error(builder, message("expected.lbrace.or.semicolon"));
        }

        if !builder.expect(Some(TokenKind::SEMICOLON)) && builder.token_type() == Some(TokenKind::LBRACE) {
            self.my_parser.statement_parser().parse_code_block(builder);
        }

        done(
            builder,
            declaration,
            if anno { NodeKind::ANNOTATION_METHOD } else { NodeKind::METHOD },
            self.language_level(),
        );
        declaration
    }

    /// `parseParameterList(builder)` (DeclarationParser.kt:509).
    pub fn parse_parameter_list(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        self.parse_element_list(builder, ListType::METHOD);
    }

    /// `parseResourceList(builder)` (DeclarationParser.kt:513).
    pub fn parse_resource_list(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        self.parse_element_list(builder, ListType::RESOURCE);
    }

    /// `parseLambdaParameterList(builder, typed)` (DeclarationParser.kt:517).
    pub fn parse_lambda_parameter_list(&self, builder: &mut SyntaxTreeBuilder<'_>, typed: bool) {
        self.parse_element_list(builder, if typed { ListType::LAMBDA_TYPED } else { ListType::LAMBDA_UNTYPED });
    }

    /// `parseElementList(builder, type)` (DeclarationParser.kt:521). The local `ErrorState(marker, errorMessage)` is a
    /// pair.
    fn parse_element_list(&self, builder: &mut SyntaxTreeBuilder<'_>, element_type: ListType) {
        let lambda = element_type == ListType::LAMBDA_TYPED || element_type == ListType::LAMBDA_UNTYPED;
        let resources = element_type == ListType::RESOURCE;
        let element_list = builder.mark();
        let left_parenth = builder.expect(Some(TokenKind::LPARENTH));
        // `require(lambda || leftParenth)`: every caller is at `(` unless it parses a lambda's parameters.
        debug_assert!(lambda || left_parenth, "{:?}", builder.token_type());

        let delimiter = if resources { TokenKind::SEMICOLON } else { TokenKind::COMMA };
        let no_delimiter_msg = if resources { "expected.semicolon" } else { "expected.comma" };
        let no_element_msg = if resources { "expected.resource" } else { "expected.parameter" };

        let mut error_state: Option<(Marker, &'static str)> = None;
        let mut delimiter_expected = false;
        let mut no_elements = true;
        loop {
            let token_type = builder.token_type();
            if token_type.is_none() || element_type.stopper_types().contains(token_type) {
                let no_last_element = !delimiter_expected && (!no_elements && !resources || no_elements && resources);
                if no_last_element {
                    let key = if lambda { "expected.parameter" } else { "expected.identifier.or.type" };
                    error(builder, message(key));
                }
                if token_type == Some(TokenKind::RPARENTH) {
                    if let Some((invalid_elements, error_message)) = error_state.take() {
                        invalid_elements.error(builder, error_message);
                    }
                    builder.advance_lexer();
                } else if !no_last_element || resources {
                    if let Some((invalid_elements, error_message)) = error_state.take() {
                        invalid_elements.error(builder, error_message);
                    }
                    if left_parenth {
                        error(builder, message("expected.rparen"));
                    }
                }
                break;
            }

            if delimiter_expected {
                if builder.token_type() == Some(delimiter) {
                    delimiter_expected = false;
                    if let Some((invalid_elements, error_message)) = error_state.take() {
                        invalid_elements.error(builder, error_message);
                    }
                    builder.advance_lexer();
                    continue;
                }
            } else {
                let list_element = if element_type == ListType::RECORD_COMPONENTS {
                    self.parse_parameter_or_record_component(builder, true, false, false, false)
                } else if resources {
                    self.parse_resource(builder)
                } else if lambda {
                    self.parse_lambda_parameter(builder, element_type == ListType::LAMBDA_TYPED)
                } else {
                    self.parse_parameter(builder, true, false, false)
                };
                if let Some(list_element) = list_element {
                    delimiter_expected = true;
                    if let Some((invalid_elements, error_message)) = error_state.take() {
                        invalid_elements.error_before(builder, error_message, list_element);
                    }
                    no_elements = false;
                    continue;
                }
            }

            if error_state.is_none() {
                if builder.token_type() == Some(delimiter) {
                    error(builder, message(no_element_msg));
                    builder.advance_lexer();
                    if no_elements && resources {
                        no_elements = false;
                    }
                    continue;
                } else {
                    error_state = Some((
                        builder.mark(),
                        message(if delimiter_expected { no_delimiter_msg } else { no_element_msg }),
                    ));
                }
            }

            // adding a reference, not simple tokens allows "Browse .." to work well
            let reference =
                self.my_parser.reference_parser().parse_java_code_reference(builder, true, true, false, false);
            if reference.is_none() && builder.token_type().is_some() {
                builder.advance_lexer();
            }
        }

        if let Some((marker, error)) = error_state {
            marker.error(builder, error);
        }

        done(builder, element_list, element_type.node_type(), self.language_level());
    }

    /// `parseParameter(builder, ellipsis, disjunctiveType, varType)` (DeclarationParser.kt:628).
    pub fn parse_parameter(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        ellipsis: bool,
        disjunctive_type: bool,
        var_type: bool,
    ) -> Option<Marker> {
        self.parse_parameter_or_record_component(builder, ellipsis, disjunctive_type, var_type, true)
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
        let mut type_flags = 0;
        if ellipsis {
            type_flags |= ReferenceParser::ELLIPSIS;
        }
        if disjunctive_type {
            type_flags |= ReferenceParser::DISJUNCTIONS;
        }
        if var_type {
            type_flags |= ReferenceParser::VAR_TYPE;
        }
        self.parse_list_element(
            builder,
            true,
            type_flags,
            Some(if is_parameter { NodeKind::PARAMETER } else { NodeKind::RECORD_COMPONENT }),
        )
    }

    /// `parseResource(builder)` (DeclarationParser.kt:654).
    pub fn parse_resource(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let marker = builder.mark();

        let expr = self.my_parser.expression_parser().parse(builder);
        if expr.is_some()
            && RESOURCE_EXPRESSIONS.contains(java_parser_util::expr_type(builder, expr))
            && builder.token_type() != Some(TokenKind::IDENTIFIER)
        {
            marker.done(builder, NodeKind::RESOURCE_EXPRESSION);
            return Some(marker);
        }

        marker.rollback_to(builder);

        self.parse_list_element(builder, true, ReferenceParser::VAR_TYPE, Some(NodeKind::RESOURCE_VARIABLE))
    }

    /// `parseLambdaParameter(builder, typed)` (DeclarationParser.kt:671).
    pub fn parse_lambda_parameter(&self, builder: &mut SyntaxTreeBuilder<'_>, typed: bool) -> Option<Marker> {
        let mut flags = ReferenceParser::ELLIPSIS;
        if JavaFeature::VAR_LAMBDA_PARAMETER.is_sufficient(self.language_level()) {
            flags |= ReferenceParser::VAR_TYPE;
        }
        self.parse_list_element(builder, typed, flags, Some(NodeKind::PARAMETER))
    }

    /// `parseListElement(builder, typed, typeFlags, type)` (DeclarationParser.kt:683).
    fn parse_list_element(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        typed: bool,
        type_flags: i32,
        element_type: Option<NodeKind>,
    ) -> Option<Marker> {
        let param = builder.mark();

        let mod_list: Marker;
        if typed {
            mod_list = self.parse_modifier_list(builder);
            let flags = ReferenceParser::EAT_LAST_DOT | ReferenceParser::WILDCARD | type_flags;

            if self.my_parser.reference_parser().parse_type_info(builder, flags).is_none() {
                if builder.marker_start_offset(mod_list) == builder.marker_end_offset(mod_list) {
                    param.rollback_to(builder);
                    return None;
                } else {
                    error(builder, message("expected.type"));
                    empty_element(builder, NodeKind::TYPE);
                }
            }
            let token_type = builder.token_type();
            if token_type == Some(TokenKind::THIS_KEYWORD)
                || token_type == Some(TokenKind::IDENTIFIER) && builder.look_ahead(1) == Some(TokenKind::DOT)
            {
                let mark = builder.mark();

                let expr = self.my_parser.expression_parser().parse(builder);
                if expr.is_some()
                    && java_parser_util::expr_type(builder, expr) == Some(NodeKind::THIS_EXPRESSION.into())
                {
                    mark.drop(builder);
                    done(builder, param, NodeKind::RECEIVER_PARAMETER, self.language_level());
                    return Some(param);
                }

                mark.rollback_to(builder);
            }
        } else {
            mod_list = builder.mark(); // modifier list is empty for lambda expression with untyped parameters
            mod_list.done(builder, NodeKind::MODIFIER_LIST);
        }

        if builder.expect(Some(TokenKind::IDENTIFIER)) {
            if let Some(element_type @ (NodeKind::PARAMETER | NodeKind::RECORD_COMPONENT)) = element_type {
                self.eat_brackets(builder, None);
                done(builder, param, element_type, self.language_level());
                return Some(param);
            }
        } else {
            error(builder, message("expected.identifier"));
            param.drop(builder);
            return Some(mod_list);
        }

        if expect_or_error(builder, Some(TokenKind::EQ), "expected.eq")
            && self.my_parser.expression_parser().parse(builder).is_none()
        {
            error(builder, message("expected.expression"));
        }

        done(builder, param, NodeKind::RESOURCE_VARIABLE, self.language_level());
        Some(param)
    }

    /// `parseFieldOrLocalVariable(builder, declaration, declarationStart, context)` (DeclarationParser.kt:747).
    fn parse_field_or_local_variable(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        declaration: Marker,
        declaration_start: i32,
        context: Option<Context>,
    ) -> Option<Marker> {
        let var_type = match context {
            Some(Context::CLASS | Context::ANNOTATION_INTERFACE | Context::FILE | Context::JSHELL) => NodeKind::FIELD,
            Some(Context::CODE_BLOCK) => NodeKind::LOCAL_VARIABLE,
            None => {
                declaration.drop(builder);
                // `require(false) { "Unexpected context: $context" }`: every caller passes a context.
                debug_assert!(false, "Unexpected context: {context:?}");
                return None;
            }
        };

        let mut variable = declaration;
        let mut unclosed = false;
        let mut eat_semicolon = true;
        let mut should_rollback: bool;
        let mut open_marker = true;
        loop {
            should_rollback = true;

            if !self.eat_brackets(builder, None) {
                unclosed = true;
            }

            if builder.expect(Some(TokenKind::EQ)) {
                let expr = self.my_parser.expression_parser().parse(builder);
                if expr.is_some() {
                    should_rollback = false;
                } else {
                    error(builder, message("expected.expression"));
                    unclosed = true;
                    break;
                }
            }

            if builder.token_type() != Some(TokenKind::COMMA) {
                break;
            }
            done(builder, variable, var_type, self.language_level());
            builder.advance_lexer();

            if builder.token_type() != Some(TokenKind::IDENTIFIER) {
                error(builder, message("expected.identifier"));
                unclosed = true;
                eat_semicolon = false;
                open_marker = false;
                break;
            }

            variable = builder.mark();
            builder.advance_lexer();
        }

        if builder.token_type() == Some(TokenKind::SEMICOLON) && eat_semicolon {
            builder.advance_lexer();
        } else {
            // special treatment (see DeclarationParserTest.testMultiLineUnclosed())
            if !builder.eof() && should_rollback {
                let space_end = builder.current_offset();
                let text = builder.text();
                let space_start = shift_backward(text, space_end - 1, WHITESPACES);
                let line_start = shift_backward_until(text, space_end, LINE_ENDS);

                if declaration_start < line_start && line_start < space_start {
                    let new_buffer_end = shift_forward(text, WHITESPACES, line_start);
                    declaration.rollback_to(builder);
                    return self.parse(&mut java_parser_util::stopping_builder(builder, new_buffer_end), context);
                }
            }

            if !unclosed {
                error(builder, message("expected.semicolon"));
            }
        }

        if open_marker {
            done(builder, variable, var_type, self.language_level());
        }

        Some(declaration)
    }

    /// `eatBrackets(builder, errorKey)` (DeclarationParser.kt:835).
    fn eat_brackets(&self, builder: &mut SyntaxTreeBuilder<'_>, error_key: Option<&'static str>) -> bool {
        let token_type = builder.token_type();
        if token_type != Some(TokenKind::LBRACKET) && token_type != Some(TokenKind::AT) {
            return true;
        }

        let marker = builder.mark();

        let mut count = 0;
        loop {
            self.parse_annotations(builder);
            if !builder.expect(Some(TokenKind::LBRACKET)) {
                break;
            }
            count += 1;
            if !builder.expect(Some(TokenKind::RBRACKET)) {
                break;
            }
            count += 1;
        }

        if count == 0 {
            // just annotation, most probably belongs to a next declaration
            marker.rollback_to(builder);
            return true;
        }

        match error_key {
            Some(error_key) => marker.error(builder, message(error_key)),
            None => marker.drop(builder),
        }

        let paired = count % 2 == 0;
        if !paired {
            error(builder, message("expected.rbracket"));
        }
        paired
    }

    /// `parseAnnotations(builder)` (DeclarationParser.kt:874).
    pub fn parse_annotations(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let mut first_anno: Option<Marker> = None;

        while builder.token_type() == Some(TokenKind::AT) {
            let anno = self.parse_annotation(builder);
            if first_anno.is_none() {
                first_anno = Some(anno);
            }
        }

        first_anno
    }

    /// `parseAnnotation(builder)` (DeclarationParser.kt:885).
    ///
    /// Not in the Kotlin: an annotation counts as `Nesting::Reparsed`. Expressions are tried as types and
    /// declarations, casts and lambdas first, each rolled back, so an annotation in an expression is parsed several
    /// times over and an expression in its arguments again each time: `@A(f(@A(f(…))))` took 3 times the work per
    /// level. Past the limit the annotation is the error element "Code is too deeply nested".
    pub fn parse_annotation(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // `require(builder.tokenType === AT)`: every caller is at `@`.
        debug_assert_eq!(builder.token_type(), Some(TokenKind::AT));
        if !builder.enter_nesting(Nesting::Reparsed) {
            return too_deeply_nested(builder);
        }
        let anno = self.parse_annotation_nested(builder);
        builder.exit_nesting(Nesting::Reparsed);
        anno
    }

    /// The body of `parseAnnotation(builder)`, inside its nesting.
    fn parse_annotation_nested(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let anno = builder.mark();
        builder.advance_lexer();

        let class_ref = if builder.token_type() == Some(TokenKind::IDENTIFIER) {
            self.my_parser.reference_parser().parse_java_code_reference(builder, true, false, false, false)
        } else {
            None
        };

        if class_ref.is_none() {
            error(builder, message("expected.class.reference"));
        }

        self.parse_annotation_parameter_list(builder);

        done(builder, anno, NodeKind::ANNOTATION, self.language_level());
        anno
    }

    /// `parseAnnotationParameterList(builder)` (DeclarationParser.kt:905).
    fn parse_annotation_parameter_list(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let list = builder.mark();

        if !builder.expect(Some(TokenKind::LPARENTH)) || builder.expect(Some(TokenKind::RPARENTH)) {
            done(builder, list, NodeKind::ANNOTATION_PARAMETER_LIST, self.language_level());
            return;
        }

        if builder.token_type().is_none() {
            error(builder, message("expected.parameter.or.rparen"));
            done(builder, list, NodeKind::ANNOTATION_PARAMETER_LIST, self.language_level());
            return;
        }
        let mut element_marker = self.parse_annotation_element(builder);
        loop {
            let mut token_type = builder.token_type();
            if token_type.is_none() {
                error(
                    builder,
                    message(if element_marker.is_none() {
                        "expected.parameter.or.rparen"
                    } else {
                        "expected.comma.or.rparen"
                    }),
                );
                break;
            } else if builder.expect(Some(TokenKind::RPARENTH)) {
                break;
            } else if token_type == Some(TokenKind::COMMA) {
                builder.advance_lexer();
                element_marker = self.parse_annotation_element(builder);
                if element_marker.is_none() {
                    error(builder, message("annotation.name.is.missing"));
                    token_type = builder.token_type();
                    if token_type != Some(TokenKind::COMMA) && token_type != Some(TokenKind::RPARENTH) {
                        break;
                    }
                }
            } else {
                error(
                    builder,
                    message(if element_marker.is_none() {
                        "expected.parameter.or.rparen"
                    } else {
                        "expected.comma.or.rparen"
                    }),
                );
                token_type = builder.look_ahead(1);
                if token_type != Some(TokenKind::COMMA) && token_type != Some(TokenKind::RPARENTH) {
                    break;
                }
                builder.advance_lexer();
            }
        }

        done(builder, list, NodeKind::ANNOTATION_PARAMETER_LIST, self.language_level());
    }

    /// `parseAnnotationElement(builder)` (DeclarationParser.kt:957).
    fn parse_annotation_element(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let mut pair = builder.mark();

        let mut value_marker = self.parse_annotation_value(builder);
        if value_marker.is_none() && builder.token_type() != Some(TokenKind::EQ) {
            pair.drop(builder);
            return None;
        }
        if builder.token_type() != Some(TokenKind::EQ) {
            done(builder, pair, NodeKind::NAME_VALUE_PAIR, self.language_level());
            return Some(pair);
        }

        pair.rollback_to(builder);
        pair = builder.mark();

        expect_or_error(builder, Some(TokenKind::IDENTIFIER), "expected.identifier");
        builder.expect(Some(TokenKind::EQ));
        value_marker = self.parse_annotation_value(builder);
        if value_marker.is_none() {
            error(builder, message("expected.value"));
        }

        done(builder, pair, NodeKind::NAME_VALUE_PAIR, self.language_level());
        Some(pair)
    }

    /// `parseAnnotationValue(builder)` (DeclarationParser.kt:982).
    pub fn parse_annotation_value(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let token_type = builder.token_type();
        match token_type {
            Some(TokenKind::AT) => Some(self.parse_annotation(builder)),
            Some(TokenKind::LBRACE) => Some(self.my_parser.expression_parser().parse_array_initializer(
                builder,
                NodeKind::ANNOTATION_ARRAY_INITIALIZER,
                &|builder| self.parse_annotation_value(builder),
                "expected.value",
            )),
            _ => self.my_parser.expression_parser().parse_conditional(builder),
        }
    }
}

/// `isNonSealedToken(builder, tokenType, level)` (DeclarationParser.kt:998).
pub fn is_non_sealed_token(
    builder: &mut SyntaxTreeBuilder<'_>,
    token_type: Option<TokenKind>,
    level: LanguageLevel,
) -> bool {
    if !JavaFeature::SEALED_CLASSES.is_sufficient(level)
        || token_type != Some(TokenKind::IDENTIFIER)
        || builder.token_text() != Some(b"non".as_slice())
        || builder.look_ahead(1) != Some(TokenKind::MINUS)
        || builder.look_ahead(2) != Some(TokenKind::IDENTIFIER)
    {
        return false;
    }
    let maybe_non_sealed = builder.mark();
    builder.advance(2);
    let is_non_sealed = builder.token_text() == Some(java_keywords::SEALED.as_bytes());
    maybe_non_sealed.rollback_to(builder);
    is_non_sealed
}

/// `CharArrayUtilKmp.shiftBackward(offset, chars)` (`shiftBackward(0, offset, chars)`): the greatest offset at or
/// before `offset` whose character is not one of `chars` (-1 when there is none), `offset` itself when it is not
/// before the end.
fn shift_backward(text: &[u8], offset: i32, chars: &[u8]) -> i32 {
    if offset >= text.len() as i32 {
        return offset;
    }
    let mut offset = offset;
    while offset >= 0 && chars.contains(&text[offset as usize]) {
        offset -= 1;
    }
    offset
}

/// `CharArrayUtilKmp.shiftBackwardUntil(offset, chars)`: the greatest offset at or before `offset` whose character is
/// one of `chars` (-1 when there is none), `offset` itself when it is not before the end.
fn shift_backward_until(text: &[u8], offset: i32, chars: &[u8]) -> i32 {
    if offset >= text.len() as i32 {
        return offset;
    }
    let mut offset = offset;
    while offset >= 0 && !chars.contains(&text[offset as usize]) {
        offset -= 1;
    }
    offset
}

/// `CharArrayUtilKmp.shiftForward(chars, startOffset)`: the first offset from `start_offset` on whose character is
/// not one of `chars`, the end when there is none.
fn shift_forward(text: &[u8], chars: &[u8], start_offset: i32) -> i32 {
    let mut offset = start_offset.max(0);
    while (offset as usize) < text.len() {
        if !chars.contains(&text[offset as usize]) {
            return offset;
        }
        offset += 1;
    }
    text.len() as i32
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
