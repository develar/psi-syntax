//! `FileParser.kt`: a file's package statement, imports, module declaration and top-level declarations.

use psi_builder::Marker;

use crate::builder::{SyntaxTreeBuilder, get_preceding_comment_binder, get_special_preceding_comment_binder};
use crate::bundle;
use crate::node_kind::NodeKind;
use crate::parser::declaration_parser::Context;
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{self, done};
use crate::syntax_element_types::MODIFIER_BIT_SET;
use crate::tokens::{TokenKind, TokenSet};

/// `FileParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::file_parser`.
#[derive(Copy, Clone)]
pub struct FileParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> FileParser<'p> {
    /// `parse(builder)` (FileParser.kt:17).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        self.parse_file(builder, &|b| self.stop_import_list_parsing(b), bundle::message, "expected.class.or.interface");
    }

    /// `parseFile(builder, importListStopper, bundle, errorMessageKey)` (FileParser.kt:25).
    pub fn parse_file(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        import_list_stopper: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> bool,
        bundle: fn(&'static str) -> &'static str,
        error_message_key: &'static str,
    ) {
        self.parse_package_statement(builder);

        let imp_list_info = self.parse_import_list(builder, import_list_stopper); // (importList, isEmpty)
        let mut first_declaration_ok: Option<bool> = None;
        let mut first_declaration: Option<Marker> = None;

        let mut invalid_elements: Option<Marker> = None;
        let mut is_implicit_class = false;
        while !builder.eof() {
            if builder.token_type() == Some(TokenKind::SEMICOLON) {
                builder.advance_lexer();
                continue;
            }

            let declaration = match self.my_parser.module_parser().parse(builder) {
                Some(module) => Some(module),
                None => self.parse_initial(builder),
            };
            if let Some(declaration) = declaration {
                if let Some(invalid) = invalid_elements.take() {
                    invalid.error_before(builder, bundle(error_message_key), declaration);
                }
                if first_declaration_ok.is_none() {
                    first_declaration_ok =
                        Some(java_parser_util::expr_type(builder, Some(declaration)) != NodeKind::MODIFIER_LIST);
                }
                if first_declaration.is_none() {
                    first_declaration = Some(declaration);
                }
                if !is_implicit_class
                    && IMPLICIT_CLASS_INDICATORS.contains(java_parser_util::expr_type(builder, Some(declaration)))
                {
                    is_implicit_class = true;
                }
                continue;
            }

            if invalid_elements.is_none() {
                invalid_elements = Some(builder.mark());
            }
            builder.advance_lexer();
            if first_declaration_ok.is_none() {
                first_declaration_ok = Some(false);
            }
        }

        if let Some(invalid) = invalid_elements {
            invalid.error(builder, bundle(error_message_key));
        }

        if imp_list_info.1
            && first_declaration_ok == Some(true)
            && let Some(first_declaration) = first_declaration
        {
            // pass comments behind fake import list
            imp_list_info.0.set_custom_edge_token_binders(
                builder,
                Some(get_preceding_comment_binder(self.my_parser.language_level)),
                None,
            );
            first_declaration.set_custom_edge_token_binders(
                builder,
                Some(get_special_preceding_comment_binder(self.my_parser.language_level)),
                None,
            );
        }
        if is_implicit_class && let Some(first_declaration) = first_declaration {
            let before_first = first_declaration.precede(builder);
            done(builder, before_first, NodeKind::IMPLICIT_CLASS, self.my_parser.language_level);
        }
    }

    /// `stopImportListParsing(b)` (FileParser.kt:88).
    fn stop_import_list_parsing(&self, b: &mut SyntaxTreeBuilder<'_>) -> bool {
        let element_type = b.token_type();
        if IMPORT_LIST_STOPPER_SET.contains(element_type)
            || self.my_parser.declaration_parser().is_record_token(b, element_type)
        {
            return true;
        }
        if element_type == Some(TokenKind::IDENTIFIER) {
            let text = b.token_text();
            if text == Some(java_keywords::OPEN.as_bytes()) || text == Some(java_keywords::MODULE.as_bytes()) {
                return true;
            }
        }
        false
    }

    /// `parseInitial(builder)` (FileParser.kt:98).
    pub(crate) fn parse_initial(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        self.my_parser.declaration_parser().parse(builder, Some(Context::FILE))
    }

    /// `parsePackageStatement(builder)` (FileParser.kt:102).
    fn parse_package_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let statement = builder.mark();

        if !builder.expect(Some(TokenKind::PACKAGE_KEYWORD)) {
            let mod_list = builder.mark();
            self.my_parser.declaration_parser().parse_annotations(builder);
            done(builder, mod_list, NodeKind::MODIFIER_LIST, self.my_parser.language_level);
            if !builder.expect(Some(TokenKind::PACKAGE_KEYWORD)) {
                statement.rollback_to(builder);
                return;
            }
        }

        let reference = self.my_parser.reference_parser().parse_java_code_reference(builder, true, false, false, false);
        if reference.is_none() {
            statement.error(builder, bundle::message("expected.class.or.interface"));
            return;
        }

        java_parser_util::semicolon(builder);

        done(builder, statement, NodeKind::PACKAGE_STATEMENT, self.my_parser.language_level);
    }

    /// `parseImportList(builder, stopper)` (FileParser.kt:126).
    pub(crate) fn parse_import_list(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        stopper: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> bool,
    ) -> (Marker, bool) {
        let mut list = builder.mark();

        let mut is_empty = true;
        let mut invalid_elements: Option<Marker> = None;
        while !builder.eof() {
            if stopper(builder) {
                break;
            } else if builder.token_type() == Some(TokenKind::SEMICOLON) {
                builder.advance_lexer();
                continue;
            }

            let statement = self.parse_import_statement(builder);
            if let Some(statement) = statement {
                is_empty = false;
                if let Some(invalid) = invalid_elements.take() {
                    invalid.error_before(builder, bundle::message("unexpected.token"), statement);
                }
                continue;
            }

            if invalid_elements.is_none() {
                invalid_elements = Some(builder.mark());
            }
            builder.advance_lexer();
        }

        if let Some(invalid) = invalid_elements {
            invalid.rollback_to(builder);
        }

        if is_empty {
            let precede = list.precede(builder);
            list.rollback_to(builder);
            list = precede;
        }

        done(builder, list, NodeKind::IMPORT_LIST, self.my_parser.language_level);
        (list, is_empty)
    }

    /// `parseImportStatement(builder)` (FileParser.kt:171).
    pub(crate) fn parse_import_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        if builder.token_type() != Some(TokenKind::IMPORT_KEYWORD) {
            return None;
        }

        let statement = builder.mark();
        builder.advance_lexer();

        let identifier_is_module = builder.token_text() == Some(java_keywords::MODULE.as_bytes());
        let element_type = self.get_import_type(builder);
        let is_static = element_type == NodeKind::IMPORT_STATIC_STATEMENT;
        let is_module = element_type == NodeKind::IMPORT_MODULE_STATEMENT;
        let is_ok = if is_module {
            self.my_parser.module_parser().parse_name(builder).is_some()
        } else {
            self.my_parser.reference_parser().parse_import_code_reference(builder, is_static)
        };

        //if it is `module` we should expect either `;` or `identifier`
        if is_ok
            && !is_module
            && !is_static
            && builder.token_type() != Some(TokenKind::SEMICOLON)
            && identifier_is_module
        {
            java_parser_util::error(builder, bundle::message("expected.identifier.or.semicolon"));
        } else if is_ok {
            java_parser_util::semicolon(builder);
        }

        done(builder, statement, element_type, self.my_parser.language_level);
        Some(statement)
    }

    /// `getImportType(builder)` (FileParser.kt:203).
    fn get_import_type(&self, builder: &mut SyntaxTreeBuilder<'_>) -> NodeKind {
        let element_type = builder.token_type();
        if element_type == Some(TokenKind::STATIC_KEYWORD) {
            builder.advance_lexer();
            return NodeKind::IMPORT_STATIC_STATEMENT;
        }
        if element_type == Some(TokenKind::IDENTIFIER)
            && builder.token_text() == Some(java_keywords::MODULE.as_bytes())
            && builder.look_ahead(1) == Some(TokenKind::IDENTIFIER)
        {
            builder.remap_current_token(TokenKind::MODULE_KEYWORD);
            builder.advance_lexer();
            return NodeKind::IMPORT_MODULE_STATEMENT;
        }
        NodeKind::IMPORT_STATEMENT
    }
}

/// `IMPORT_LIST_STOPPER_SET`.
pub const IMPORT_LIST_STOPPER_SET: TokenSet = TokenSet::or_set(&[
    MODIFIER_BIT_SET,
    TokenSet::create(&[TokenKind::CLASS_KEYWORD, TokenKind::INTERFACE_KEYWORD, TokenKind::ENUM_KEYWORD, TokenKind::AT]),
]);

/// `IMPLICIT_CLASS_INDICATORS`.
const IMPLICIT_CLASS_INDICATORS: TokenSet =
    TokenSet::create_nodes(&[NodeKind::METHOD, NodeKind::FIELD, NodeKind::CLASS_INITIALIZER]);
