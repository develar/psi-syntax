//! `FileParser.kt`: a file's package statement, imports, module declaration and top-level declarations.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::syntax_element_types::MODIFIER_BIT_SET;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `FileParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::file_parser`.
#[derive(Copy, Clone)]
pub struct FileParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> FileParser<'p> {
    /// `parse(builder)` (FileParser.kt:17).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): FileParser.kt:17
        todo!("FileParser.parse (FileParser.kt:17)")
    }

    /// `parseFile(builder, importListStopper, bundle, errorMessageKey)` (FileParser.kt:25).
    pub fn parse_file(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        import_list_stopper: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> bool,
        bundle: fn(&'static str) -> &'static str,
        error_message_key: &'static str,
    ) {
        // TODO(port): FileParser.kt:25
        todo!("FileParser.parseFile (FileParser.kt:25)")
    }

    /// `stopImportListParsing(b)` (FileParser.kt:88).
    fn stop_import_list_parsing(&self, b: &mut SyntaxTreeBuilder<'_>) -> bool {
        // TODO(port): FileParser.kt:88
        todo!("FileParser.stopImportListParsing (FileParser.kt:88)")
    }

    /// `parseInitial(builder)` (FileParser.kt:98).
    pub(crate) fn parse_initial(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): FileParser.kt:98
        todo!("FileParser.parseInitial (FileParser.kt:98)")
    }

    /// `parsePackageStatement(builder)` (FileParser.kt:102).
    fn parse_package_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): FileParser.kt:102
        todo!("FileParser.parsePackageStatement (FileParser.kt:102)")
    }

    /// `parseImportList(builder, stopper)` (FileParser.kt:126).
    pub(crate) fn parse_import_list(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        stopper: &dyn Fn(&mut SyntaxTreeBuilder<'_>) -> bool,
    ) -> (Marker, bool) {
        // TODO(port): FileParser.kt:126
        todo!("FileParser.parseImportList (FileParser.kt:126)")
    }

    /// `parseImportStatement(builder)` (FileParser.kt:171).
    pub(crate) fn parse_import_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): FileParser.kt:171
        todo!("FileParser.parseImportStatement (FileParser.kt:171)")
    }

    /// `getImportType(builder)` (FileParser.kt:203).
    fn get_import_type(&self, builder: &mut SyntaxTreeBuilder<'_>) -> NodeKind {
        // TODO(port): FileParser.kt:203
        todo!("FileParser.getImportType (FileParser.kt:203)")
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
