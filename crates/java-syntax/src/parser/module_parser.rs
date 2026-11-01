//! `ModuleParser.kt`: a module declaration (`module-info.java`).
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `ModuleParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::module_parser`.
#[derive(Copy, Clone)]
pub struct ModuleParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> ModuleParser<'p> {
    /// `parse(builder)` (ModuleParser.kt:14).
    pub fn parse(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): ModuleParser.kt:14
        todo!("ModuleParser.parse (ModuleParser.kt:14)")
    }

    /// `parseName(builder)` (ModuleParser.kt:74).
    pub fn parse_name(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): ModuleParser.kt:74
        todo!("ModuleParser.parseName (ModuleParser.kt:74)")
    }

    /// `parseModuleContent(builder)` (ModuleParser.kt:105).
    fn parse_module_content(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): ModuleParser.kt:105
        todo!("ModuleParser.parseModuleContent (ModuleParser.kt:105)")
    }

    /// `parseStatement(builder)` (ModuleParser.kt:141).
    fn parse_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): ModuleParser.kt:141
        todo!("ModuleParser.parseStatement (ModuleParser.kt:141)")
    }

    /// `parseRequiresStatement(builder)` (ModuleParser.kt:151).
    fn parse_requires_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): ModuleParser.kt:151
        todo!("ModuleParser.parseRequiresStatement (ModuleParser.kt:151)")
    }

    /// `parseExportsStatement(builder)` (ModuleParser.kt:177).
    fn parse_exports_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): ModuleParser.kt:177
        todo!("ModuleParser.parseExportsStatement (ModuleParser.kt:177)")
    }

    /// `parseOpensStatement(builder)` (ModuleParser.kt:183).
    fn parse_opens_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): ModuleParser.kt:183
        todo!("ModuleParser.parseOpensStatement (ModuleParser.kt:183)")
    }

    /// `parsePackageStatement(builder, statement, type)` (ModuleParser.kt:189).
    fn parse_package_statement(
        &self,
        builder: &mut SyntaxTreeBuilder<'_>,
        statement: Marker,
        element_type: NodeKind,
    ) -> Marker {
        // TODO(port): ModuleParser.kt:189
        todo!("ModuleParser.parsePackageStatement (ModuleParser.kt:189)")
    }

    /// `parseUsesStatement(builder)` (ModuleParser.kt:225).
    fn parse_uses_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): ModuleParser.kt:225
        todo!("ModuleParser.parseUsesStatement (ModuleParser.kt:225)")
    }

    /// `parseProvidesStatement(builder)` (ModuleParser.kt:241).
    fn parse_provides_statement(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): ModuleParser.kt:241
        todo!("ModuleParser.parseProvidesStatement (ModuleParser.kt:241)")
    }

    /// `parseNameRef(builder)` (ModuleParser.kt:280).
    fn parse_name_ref(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): ModuleParser.kt:280
        todo!("ModuleParser.parseNameRef (ModuleParser.kt:280)")
    }

    /// `parseClassOrPackageRef(builder)` (ModuleParser.kt:288).
    fn parse_class_or_package_ref(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): ModuleParser.kt:288
        todo!("ModuleParser.parseClassOrPackageRef (ModuleParser.kt:288)")
    }

    /// `mapAndAdvance(builder, keyword)` (ModuleParser.kt:292).
    fn map_and_advance(&self, builder: &mut SyntaxTreeBuilder<'_>, keyword: TokenKind) {
        // TODO(port): ModuleParser.kt:292
        todo!("ModuleParser.mapAndAdvance (ModuleParser.kt:292)")
    }

    /// `parseExtras(builder, message)` (ModuleParser.kt:297).
    fn parse_extras(&self, builder: &mut SyntaxTreeBuilder<'_>, message: impl Into<Cow<'static, str>>) {
        // TODO(port): ModuleParser.kt:297
        todo!("ModuleParser.parseExtras (ModuleParser.kt:297)")
    }
}

/// `STATEMENT_KEYWORDS`.
const STATEMENT_KEYWORDS: [&str; 4] =
    [java_keywords::REQUIRES, java_keywords::EXPORTS, java_keywords::USES, java_keywords::PROVIDES];
