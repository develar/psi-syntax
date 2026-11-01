//! `ModuleParser.kt`: a module declaration (`module-info.java`).

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::bundle::message;
use crate::node_kind::NodeKind;
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{self, done};
use crate::syntax_element_types::MODIFIER_BIT_SET;
use crate::tokens::TokenKind;

/// `text == keyword` for a token text that may be absent.
#[inline]
fn is(text: Option<&[u8]>, keyword: &str) -> bool {
    text == Some(keyword.as_bytes())
}

/// `ModuleParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::module_parser`.
#[derive(Copy, Clone)]
pub struct ModuleParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> ModuleParser<'p> {
    /// `parse(builder)` (ModuleParser.kt:14).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let module = builder.mark();

        let first_annotation = self.my_parser.declaration_parser().parse_annotations(builder);

        let element_type = builder.token_type();
        // `text`: the identifier's text, compared only with `open` and `module`.
        let text = if element_type == Some(TokenKind::IDENTIFIER) { builder.token_text() } else { None };
        let is_open = is(text, java_keywords::OPEN);
        let mut is_module = is(text, java_keywords::MODULE);
        if !(is_open || is_module) {
            module.rollback_to(builder);
            return None;
        }

        let modifier_list = match first_annotation {
            Some(first_annotation) => first_annotation.precede(builder),
            None => builder.mark(),
        };
        if is_open {
            self.map_and_advance(builder, TokenKind::OPEN_KEYWORD);
            is_module = is(builder.token_text(), java_keywords::MODULE);
        }
        done(builder, modifier_list, NodeKind::MODIFIER_LIST, self.my_parser.language_level);

        if is_module {
            self.map_and_advance(builder, TokenKind::MODULE_KEYWORD);
        } else {
            module.drop(builder);
            self.parse_extras(builder, message("expected.module.declaration"));
            return Some(module);
        }

        if self.parse_name(builder).is_none() {
            module.drop(builder);
            if builder.token_type().is_some() {
                self.parse_extras(builder, message("expected.module.declaration"));
            } else {
                java_parser_util::error(builder, message("expected.identifier"));
            }
            return Some(module);
        }

        if !builder.expect(Some(TokenKind::LBRACE)) {
            if builder.token_type().is_some() {
                self.parse_extras(builder, message("expected.module.declaration"));
            } else {
                java_parser_util::error(builder, message("expected.lbrace"));
            }
        } else {
            self.parse_module_content(builder);
        }

        done(builder, module, NodeKind::MODULE, self.my_parser.language_level);

        if builder.token_type().is_some() {
            self.parse_extras(builder, message("unexpected.tokens"));
        }

        Some(module)
    }

    /// `parseName(builder)` (ModuleParser.kt:74).
    pub fn parse_name(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let name_element = builder.mark();
        let mut empty = true;

        let mut id_expected = true;
        loop {
            let t = builder.token_type();
            if t == Some(TokenKind::IDENTIFIER) {
                if !id_expected {
                    java_parser_util::error(builder, message("expected.dot"));
                }
                id_expected = false;
            } else if t == Some(TokenKind::DOT) {
                if id_expected {
                    java_parser_util::error(builder, message("expected.identifier"));
                }
                id_expected = true;
            } else {
                break;
            }
            builder.advance_lexer();
            empty = false;
        }

        if !empty {
            if id_expected {
                java_parser_util::error(builder, message("expected.identifier"));
            }
            name_element.done(builder, NodeKind::MODULE_REFERENCE);
            Some(name_element)
        } else {
            name_element.drop(builder);
            None
        }
    }

    /// `parseModuleContent(builder)` (ModuleParser.kt:105).
    fn parse_module_content(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let mut invalid: Option<Marker> = None;

        while let Some(token) = builder.token_type() {
            if token == TokenKind::RBRACE {
                break;
            }

            if token == TokenKind::SEMICOLON {
                if let Some(invalid) = invalid.take() {
                    invalid.error(builder, message("expected.module.statement"));
                }
                builder.advance_lexer();
                continue;
            }

            let statement = self.parse_statement(builder);
            match statement {
                None => {
                    if invalid.is_none() {
                        invalid = Some(builder.mark());
                    }
                    builder.advance_lexer();
                }
                Some(statement) => {
                    if let Some(invalid) = invalid.take() {
                        invalid.error_before(builder, message("expected.module.statement"), statement);
                    }
                }
            }
        }

        if let Some(invalid) = invalid {
            invalid.error(builder, message("expected.module.statement"));
        }

        if !builder.expect(Some(TokenKind::RBRACE)) && invalid.is_none() {
            java_parser_util::error(builder, message("expected.rbrace"));
        }
    }

    /// `parseStatement(builder)` (ModuleParser.kt:141).
    fn parse_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let kw = builder.token_text();
        if is(kw, java_keywords::REQUIRES) {
            return Some(self.parse_requires_statement(builder));
        }
        if is(kw, java_keywords::EXPORTS) {
            return Some(self.parse_exports_statement(builder));
        }
        if is(kw, java_keywords::OPENS) {
            return Some(self.parse_opens_statement(builder));
        }
        if is(kw, java_keywords::USES) {
            return Some(self.parse_uses_statement(builder));
        }
        if is(kw, java_keywords::PROVIDES) {
            return Some(self.parse_provides_statement(builder));
        }
        None
    }

    /// `parseRequiresStatement(builder)` (ModuleParser.kt:151).
    fn parse_requires_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        self.map_and_advance(builder, TokenKind::REQUIRES_KEYWORD);

        let modifier_list = builder.mark();
        loop {
            if builder.expect_set(MODIFIER_BIT_SET) {
                continue;
            }
            if builder.token_type() == Some(TokenKind::IDENTIFIER)
                && is(builder.token_text(), java_keywords::TRANSITIVE)
            {
                self.map_and_advance(builder, TokenKind::TRANSITIVE_KEYWORD);
                continue;
            }
            break;
        }
        done(builder, modifier_list, NodeKind::MODIFIER_LIST, self.my_parser.language_level);

        if self.parse_name_ref(builder).is_some() {
            java_parser_util::semicolon(builder);
        } else {
            builder.expect(Some(TokenKind::SEMICOLON));
        }

        statement.done(builder, NodeKind::REQUIRES_STATEMENT);
        statement
    }

    /// `parseExportsStatement(builder)` (ModuleParser.kt:177).
    fn parse_exports_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        self.map_and_advance(builder, TokenKind::EXPORTS_KEYWORD);
        self.parse_package_statement(builder, statement, NodeKind::EXPORTS_STATEMENT)
    }

    /// `parseOpensStatement(builder)` (ModuleParser.kt:183).
    fn parse_opens_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        self.map_and_advance(builder, TokenKind::OPENS_KEYWORD);
        self.parse_package_statement(builder, statement, NodeKind::OPENS_STATEMENT)
    }

    /// `parsePackageStatement(builder, statement, type)` (ModuleParser.kt:189).
    fn parse_package_statement(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        statement: Marker,
        element_type: NodeKind,
    ) -> Marker {
        let mut has_error = false;

        if self.parse_class_or_package_ref(builder).is_some() {
            if is(builder.token_text(), java_keywords::TO) {
                self.map_and_advance(builder, TokenKind::TO_KEYWORD);

                loop {
                    let reference = self.parse_name_ref(builder);
                    if !builder.expect(Some(TokenKind::COMMA)) {
                        if reference.is_none() {
                            has_error = true;
                        }
                        break;
                    }
                }
            }
        } else {
            java_parser_util::error(builder, message("expected.package.reference"));
            has_error = true;
        }

        if !has_error {
            java_parser_util::semicolon(builder);
        } else {
            builder.expect(Some(TokenKind::SEMICOLON));
        }

        statement.done(builder, element_type);
        statement
    }

    /// `parseUsesStatement(builder)` (ModuleParser.kt:225).
    fn parse_uses_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        self.map_and_advance(builder, TokenKind::USES_KEYWORD);

        if self.parse_class_or_package_ref(builder).is_some() {
            java_parser_util::semicolon(builder);
        } else {
            java_parser_util::error(builder, message("expected.class.reference"));
            builder.expect(Some(TokenKind::SEMICOLON));
        }

        statement.done(builder, NodeKind::USES_STATEMENT);
        statement
    }

    /// `parseProvidesStatement(builder)` (ModuleParser.kt:241).
    fn parse_provides_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        let statement = builder.mark();
        let mut has_error = false;
        self.map_and_advance(builder, TokenKind::PROVIDES_KEYWORD);

        if self.parse_class_or_package_ref(builder).is_none() {
            java_parser_util::error(builder, message("expected.class.reference"));
            has_error = true;
        }

        if is(builder.token_text(), java_keywords::WITH) {
            builder.remap_current_token(TokenKind::WITH_KEYWORD);
            has_error = self.my_parser.reference_parser().parse_reference_list(
                builder,
                Some(TokenKind::WITH_KEYWORD),
                Some(NodeKind::PROVIDES_WITH_LIST),
                Some(TokenKind::COMMA),
            );
        } else if !has_error {
            let next = builder.token_type();
            if next == Some(TokenKind::IDENTIFIER) && !STATEMENT_KEYWORDS.iter().any(|kw| is(builder.token_text(), kw))
            {
                let marker = builder.mark();
                builder.advance_lexer();
                marker.error(builder, message("expected.with"));
            } else {
                java_parser_util::error(builder, message("expected.with"));
            }
            has_error = true;
        }

        if !has_error {
            java_parser_util::semicolon(builder);
        } else {
            builder.expect(Some(TokenKind::SEMICOLON));
        }

        statement.done(builder, NodeKind::PROVIDES_STATEMENT);
        statement
    }

    /// `parseNameRef(builder)` (ModuleParser.kt:280).
    fn parse_name_ref(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let name = self.parse_name(builder);
        if name.is_none() {
            java_parser_util::error(builder, message("expected.identifier"));
        }
        name
    }

    /// `parseClassOrPackageRef(builder)` (ModuleParser.kt:288).
    fn parse_class_or_package_ref(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.my_parser.reference_parser().parse_java_code_reference(builder, true, false, false, false)
    }

    /// `mapAndAdvance(builder, keyword)` (ModuleParser.kt:292).
    fn map_and_advance(&self, builder: &mut SyntaxTreeBuilder<'_>, keyword: TokenKind) {
        builder.remap_current_token(keyword);
        builder.advance_lexer();
    }

    /// `parseExtras(builder, message)` (ModuleParser.kt:297).
    fn parse_extras(&self, builder: &mut SyntaxTreeBuilder<'_>, message: impl Into<Cow<'static, str>>) {
        let extras = builder.mark();
        while builder.token_type().is_some() {
            builder.advance_lexer();
        }
        extras.error(builder, message);
    }
}

/// `STATEMENT_KEYWORDS`.
const STATEMENT_KEYWORDS: [&str; 4] =
    [java_keywords::REQUIRES, java_keywords::EXPORTS, java_keywords::USES, java_keywords::PROVIDES];
