//! `JShellParser.kt`: JShell snippets (a `JavaParser` whose file parser reads imports, declarations, statements
//! and expressions in any order). Not reached by `.java` files.

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::bundle;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::declaration_parser::Context;
use crate::parser::file_parser::IMPORT_LIST_STOPPER_SET;
use crate::parser::java_parser::JavaParser;
use crate::syntax_element_types::{
    EXPRESSION_BIT_SET, JAVA_COMMENT_BIT_SET, JAVA_STATEMENT_BIT_SET, MODIFIER_BIT_SET, PRIMITIVE_TYPE_BIT_SET,
};
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `JShellParser(languageLevel)`: a `JavaParser` with its own file parser.
#[derive(Copy, Clone, Debug)]
pub struct JShellParser {
    pub java_parser: JavaParser,
}

impl JShellParser {
    pub fn new(language_level: LanguageLevel) -> Self {
        JShellParser { java_parser: JavaParser::new(language_level) }
    }

    /// `parse(builder)` (JShellParser.kt:76).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let root = builder.mark();
        self.file_parser_parse(builder);
        root.done(builder, NodeKind::JSHELL_FILE);
    }

    /// `isParsed(parsedMarker, builder, cond)` (JShellParser.kt:83).
    fn is_parsed(
        &self,
        parsed_marker: Option<Marker>,
        builder: &mut SyntaxTreeBuilder<'_>,
        cond: impl Fn(ElementType) -> bool,
    ) -> bool {
        if parsed_marker.is_none() {
            return false;
        }
        let Some(last_done) = builder.last_done_marker() else { return false };
        builder.marker_node_type(last_done).is_some_and(cond)
    }

    /// `fileParser.parse(builder)`: the `FileParser` the JShell parser overrides (JShellParser.kt:19). Its
    /// `try`/`finally` closes the root class however the loop ends; nothing here throws.
    pub fn file_parser_parse(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let file_parser = self.java_parser.file_parser();
        file_parser.parse_import_list(builder, &|b| IMPORT_PARSING_STOP_LIST.contains(b.token_type()));

        let root_class: Marker = builder.mark();
        while !builder.eof() {
            let mut wrapper: Option<Marker> = Some(builder.mark());
            let mut wrapper_type: Option<NodeKind> = None;

            let mut marker: Option<Marker> = file_parser.parse_import_statement(builder);
            if self.is_parsed(marker, builder, |token_type| token_type == NodeKind::IMPORT_STATEMENT) {
                wrapper_type = Some(NodeKind::JSHELL_IMPORT_HOLDER);
            } else {
                if let Some(marker) = marker {
                    marker.rollback_to(builder);
                }
                marker = self.java_parser.declaration_parser().parse(builder, Some(Context::JSHELL));
                if self.is_parsed(marker, builder, |token_type| TOP_LEVEL_DECLARATIONS.contains(token_type))
                    && marker.is_some_and(|marker| !builder.has_errors_after(marker))
                {
                    // don't need wrapper for top-level declaration
                    if let Some(wrapper) = wrapper.take() {
                        wrapper.drop(builder);
                    }
                } else {
                    if let Some(marker) = marker {
                        marker.rollback_to(builder);
                    }
                    marker = self.java_parser.statement_parser().parse_statement(builder);
                    if marker.is_some_and(|marker| !builder.has_errors_after(marker)) {
                        wrapper_type = Some(NodeKind::JSHELL_STATEMENTS_HOLDER);
                    } else {
                        if let Some(marker) = marker {
                            marker.rollback_to(builder);
                        }
                        marker = self.java_parser.expression_parser().parse(builder);
                        wrapper_type = if marker.is_some() { Some(NodeKind::JSHELL_STATEMENTS_HOLDER) } else { None };
                    }
                }
            }

            if marker.is_none() {
                if let Some(wrapper) = wrapper {
                    wrapper.drop(builder);
                }
                break;
            }

            if let (Some(wrapper), Some(wrapper_type)) = (wrapper, wrapper_type) {
                wrapper.done(builder, wrapper_type);
            }
        }

        if !builder.eof() {
            builder.mark().error(builder, bundle::message("unexpected.token"));
            while !builder.eof() {
                builder.advance_lexer();
            }
        }

        root_class.done(builder, NodeKind::JSHELL_ROOT_CLASS);
    }
}

/// `TOP_LEVEL_DECLARATIONS`.
const TOP_LEVEL_DECLARATIONS: TokenSet = TokenSet::create_nodes(&[NodeKind::FIELD, NodeKind::METHOD, NodeKind::CLASS]);

/// `IMPORT_PARSING_STOP_LIST`.
const IMPORT_PARSING_STOP_LIST: TokenSet = TokenSet::or_set(&[
    IMPORT_LIST_STOPPER_SET,
    MODIFIER_BIT_SET,
    JAVA_COMMENT_BIT_SET,
    EXPRESSION_BIT_SET,
    JAVA_STATEMENT_BIT_SET,
    PRIMITIVE_TYPE_BIT_SET,
    TokenSet::create_nodes(&[NodeKind::JSHELL_ROOT_CLASS]),
    TokenSet::create(&[TokenKind::IDENTIFIER]),
]);
