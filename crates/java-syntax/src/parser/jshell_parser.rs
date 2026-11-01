//! `JShellParser.kt`: JShell snippets (a `JavaParser` whose file parser reads imports, declarations, statements
//! and expressions in any order). Not reached by `.java` files.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
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
        // TODO(port): JShellParser.kt:76
        todo!("JShellParser.parse (JShellParser.kt:76)")
    }

    /// `isParsed(parsedMarker, builder, cond)` (JShellParser.kt:83).
    fn is_parsed(
        &self,
        parsed_marker: Option<Marker>,
        builder: &mut SyntaxTreeBuilder<'_>,
        cond: impl Fn(ElementType) -> bool,
    ) -> bool {
        // TODO(port): JShellParser.kt:83
        todo!("JShellParser.isParsed (JShellParser.kt:83)")
    }

    /// `fileParser.parse(builder)`: the `FileParser` the JShell parser overrides (JShellParser.kt:19).
    pub fn file_parser_parse(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): JShellParser.kt:19
        todo!("JShellParser.parse (JShellParser.kt:19)")
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
