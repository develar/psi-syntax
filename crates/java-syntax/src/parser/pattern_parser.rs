//! `PatternParser.kt`: type, record and unnamed patterns.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `PatternParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::pattern_parser`.
#[derive(Copy, Clone)]
pub struct PatternParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> PatternParser<'p> {
    /// `isPattern(builder)` (PatternParser.kt:24).
    pub fn is_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        // TODO(port): PatternParser.kt:24
        todo!("PatternParser.isPattern (PatternParser.kt:24)")
    }

    /// `parseUnnamedPattern(builder)` (PatternParser.kt:30).
    fn parse_unnamed_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        // TODO(port): PatternParser.kt:30
        todo!("PatternParser.parseUnnamedPattern (PatternParser.kt:30)")
    }

    /// `preParsePattern(builder)` (PatternParser.kt:43).
    pub fn pre_parse_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        // TODO(port): PatternParser.kt:43
        todo!("PatternParser.preParsePattern (PatternParser.kt:43)")
    }

    /// `parsePattern(builder)` (PatternParser.kt:60).
    pub fn parse_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        // TODO(port): PatternParser.kt:60
        todo!("PatternParser.parsePattern (PatternParser.kt:60)")
    }

    /// `parsePattern(builder, expectVar)` (PatternParser.kt:64).
    fn parse_pattern_with(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        // TODO(port): PatternParser.kt:64
        todo!("PatternParser.parsePattern (PatternParser.kt:64)")
    }

    /// `parsePrimaryPattern(builder, expectVar)` (PatternParser.kt:68).
    pub fn parse_primary_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        // TODO(port): PatternParser.kt:68
        todo!("PatternParser.parsePrimaryPattern (PatternParser.kt:68)")
    }

    /// `parseRecordStructurePattern(builder)` (PatternParser.kt:72).
    fn parse_record_structure_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        // TODO(port): PatternParser.kt:72
        todo!("PatternParser.parseRecordStructurePattern (PatternParser.kt:72)")
    }

    /// `parseTypeOrRecordPattern(builder, expectVar)` (PatternParser.kt:110).
    fn parse_type_or_record_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        // TODO(port): PatternParser.kt:110
        todo!("PatternParser.parseTypeOrRecordPattern (PatternParser.kt:110)")
    }

    /// `isWhenAsIdentifier(previousIsRecord)` (PatternParser.kt:169).
    fn is_when_as_identifier(&self, previous_is_record: bool) -> bool {
        // TODO(port): PatternParser.kt:169
        todo!("PatternParser.isWhenAsIdentifier (PatternParser.kt:169)")
    }
}

/// `PATTERN_MODIFIERS`.
const PATTERN_MODIFIERS: TokenSet = TokenSet::create(&[TokenKind::FINAL_KEYWORD]);
