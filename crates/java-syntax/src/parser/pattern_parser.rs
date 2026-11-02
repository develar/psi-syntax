//! `PatternParser.kt`: type, record and unnamed patterns.

use psi_builder::Marker;

use crate::builder::{Nesting, SyntaxTreeBuilder};
use crate::bundle::message;
use crate::node_kind::NodeKind;
use crate::parser::java_keywords;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::{done, empty_element, error, expect_or_error, too_deeply_nested};
use crate::parser::reference_parser::ReferenceParser;
use crate::tokens::{TokenKind, TokenSet};

/// `PatternParser`: a view of its `JavaParser` (`myParser`), made by `JavaParser::pattern_parser`.
#[derive(Copy, Clone)]
pub struct PatternParser<'p> {
    pub(crate) my_parser: &'p JavaParser,
}

impl<'p> PatternParser<'p> {
    /// `isPattern(builder)` (PatternParser.kt:24): checks whether given token sequence can be parsed as a pattern.
    /// The result of the method makes sense only for places where pattern is expected (case label and instanceof
    /// expression).
    pub fn is_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        let Some(pattern_start) = self.pre_parse_pattern(builder) else { return false };
        pattern_start.rollback_to(builder);
        true
    }

    /// `parseUnnamedPattern(builder)` (PatternParser.kt:30).
    fn parse_unnamed_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> bool {
        let pattern_start = builder.mark();
        if builder.token_type() == Some(TokenKind::IDENTIFIER) && builder.token_text() == Some(b"_".as_slice()) {
            empty_element(builder, NodeKind::TYPE);
            builder.advance_lexer();
            done(builder, pattern_start, NodeKind::UNNAMED_PATTERN, self.my_parser.language_level);
            return true;
        }
        pattern_start.rollback_to(builder);
        false
    }

    /// `preParsePattern(builder)` (PatternParser.kt:43).
    pub fn pre_parse_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Option<Marker> {
        let pattern_start = builder.mark();
        let mod_list = self.my_parser.declaration_parser().parse_modifier_list_with(builder, PATTERN_MODIFIERS);
        let has_no_modifier = builder.marker_start_offset(mod_list) == builder.marker_end_offset(mod_list);
        let element_type = self
            .my_parser
            .reference_parser()
            .parse_type(builder, ReferenceParser::EAT_LAST_DOT | ReferenceParser::WILDCARD);
        let is_pattern = element_type.is_some()
            && (builder.token_type() == Some(TokenKind::IDENTIFIER)
                || (builder.token_type() == Some(TokenKind::LPARENTH) && has_no_modifier));
        if !is_pattern {
            pattern_start.rollback_to(builder);
            return None;
        }
        Some(pattern_start)
    }

    /// `parsePattern(builder)` (PatternParser.kt:60): must be called only if isPattern returned true.
    pub fn parse_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
        self.parse_pattern_with(builder, false)
    }

    /// `parsePattern(builder, expectVar)` (PatternParser.kt:64).
    fn parse_pattern_with(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        self.parse_primary_pattern(builder, expect_var)
    }

    /// `parsePrimaryPattern(builder, expectVar)` (PatternParser.kt:68).
    pub fn parse_primary_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        self.parse_type_or_record_pattern(builder, expect_var)
    }

    /// `parseRecordStructurePattern(builder)` (PatternParser.kt:72).
    fn parse_record_structure_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>) {
        let record_structure = builder.mark();
        let has_lparen = builder.expect(Some(TokenKind::LPARENTH));
        // `require(hasLparen)`: the only caller is at `(`.
        debug_assert!(has_lparen);

        let mut is_first = true;
        while builder.token_type() != Some(TokenKind::RPARENTH) {
            if !is_first {
                expect_or_error(builder, Some(TokenKind::COMMA), "expected.comma");
            }

            if builder.token_type().is_none() {
                break;
            }

            if self.is_pattern(builder) {
                self.parse_pattern_with(builder, true);
                is_first = false;
            } else if self.parse_unnamed_pattern(builder) {
                is_first = false;
            } else {
                let flags = ReferenceParser::EAT_LAST_DOT | ReferenceParser::WILDCARD | ReferenceParser::VAR_TYPE;
                self.my_parser.reference_parser().parse_type(builder, flags);
                error(builder, message("expected.pattern"));
                if builder.token_type() == Some(TokenKind::RPARENTH) {
                    break;
                }
                builder.advance_lexer();
            }
        }
        if !builder.expect(Some(TokenKind::RPARENTH)) {
            builder.error(message("expected.rparen"));
        }
        record_structure.done(builder, NodeKind::DECONSTRUCTION_LIST);
    }

    /// `parseTypeOrRecordPattern(builder, expectVar)` (PatternParser.kt:110).
    ///
    /// Not in the Kotlin: every recursion of a pattern (a record pattern's components) passes here, so it counts a
    /// level of nesting (`Nesting::Level`); past the limit the pattern is the error element "Code is too deeply
    /// nested".
    fn parse_type_or_record_pattern(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        if !builder.enter_nesting(Nesting::Level) {
            return too_deeply_nested(builder);
        }
        let pattern = self.parse_type_or_record_pattern_nested(builder, expect_var);
        builder.exit_nesting(Nesting::Level);
        pattern
    }

    /// The body of `parseTypeOrRecordPattern(builder, expectVar)`, inside its nesting.
    fn parse_type_or_record_pattern_nested(&self, builder: &mut SyntaxTreeBuilder<'_>, expect_var: bool) -> Marker {
        let pattern = builder.mark();
        let pattern_variable = builder.mark();
        let mod_list = self.my_parser.declaration_parser().parse_modifier_list_with(builder, PATTERN_MODIFIERS);
        let has_no_modifiers = builder.marker_start_offset(mod_list) == builder.marker_end_offset(mod_list);

        let mut flags = ReferenceParser::EAT_LAST_DOT | ReferenceParser::WILDCARD;
        if expect_var {
            flags |= ReferenceParser::VAR_TYPE;
        }
        // `checkNotNull`: a pattern is parsed where `isPattern` found a type.
        self.my_parser.reference_parser().parse_type(builder, flags);
        let mut is_record = false;
        if builder.token_type() == Some(TokenKind::LPARENTH) && has_no_modifiers {
            self.parse_record_structure_pattern(builder);
            is_record = true;
        }

        let has_identifier = if builder.token_type() == Some(TokenKind::IDENTIFIER)
            && (builder.token_text() != Some(java_keywords::WHEN.as_bytes()) || self.is_when_as_identifier(is_record))
        {
            // pattern variable after the record structure pattern
            if is_record {
                let variable = builder.mark();
                builder.advance_lexer();
                variable.done(builder, NodeKind::DECONSTRUCTION_PATTERN_VARIABLE);
            } else {
                builder.advance_lexer();
            }
            true
        } else {
            false
        };

        if is_record {
            pattern_variable.drop(builder);
            done(builder, pattern, NodeKind::DECONSTRUCTION_PATTERN, self.my_parser.language_level);
        } else {
            if has_identifier {
                done(builder, pattern_variable, NodeKind::PATTERN_VARIABLE, self.my_parser.language_level);
            } else {
                pattern_variable.drop(builder);
            }
            done(builder, pattern, NodeKind::TYPE_TEST_PATTERN, self.my_parser.language_level);
        }
        pattern
    }

    // There may be valid code samples:
    // Rec(int i) when  when     when.foo() -> {} //now it is unsupported, let's skip it
    //            ^name ^keyword ^guard expr
    //case When when -> {}
    //            ^name
    //case When(when) when              when ->{}
    //                  ^keyword         ^guard expr
    /// `isWhenAsIdentifier(previousIsRecord)` (PatternParser.kt:169).
    fn is_when_as_identifier(&self, previous_is_record: bool) -> bool {
        !previous_is_record
    }
}

/// `PATTERN_MODIFIERS`.
const PATTERN_MODIFIERS: TokenSet = TokenSet::create(&[TokenKind::FINAL_KEYWORD]);
