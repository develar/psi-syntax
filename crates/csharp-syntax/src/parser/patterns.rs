//! `LanguageParser_Patterns.cs`: patterns, switch expressions (lane C). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private CSharpSyntaxNode ParseTypeOrPatternForIsOperator()` (LanguageParser_Patterns.cs:21).
    pub(crate) fn parse_type_or_pattern_for_is_operator(&mut self) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:21
        Default::default()
    }

    /// `private bool ConvertExpressionToType(ExpressionSyntax expression, [NotNullWhen(true)] out NameSyntax? type)` (LanguageParser_Patterns.cs:33).
    pub(crate) fn convert_expression_to_type(&mut self, expression: Green, type_: &mut Green) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:33
        Default::default()
    }

    /// `private PatternSyntax ParsePattern(Precedence precedence, bool afterIs = false, bool inSwitchArmPattern = false)` (LanguageParser_Patterns.cs:53).
    pub(crate) fn parse_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:53
        Default::default()
    }

    /// `private PatternSyntax ParseDisjunctivePattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:58).
    pub(crate) fn parse_disjunctive_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:58
        Default::default()
    }

    /// `private bool LooksLikeTypeOfPattern()` (LanguageParser_Patterns.cs:76).
    pub(crate) fn looks_like_type_of_pattern(&mut self) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:76
        Default::default()
    }

    /// `private PatternSyntax ParseConjunctivePattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:104).
    pub(crate) fn parse_conjunctive_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:104
        Default::default()
    }

    /// `private bool ScanDesignation(bool permitTuple)` (LanguageParser_Patterns.cs:119).
    pub(crate) fn scan_designation(&mut self, permit_tuple: bool) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:119
        Default::default()
    }

    /// `private PatternSyntax ParseNegatedPattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:158).
    pub(crate) fn parse_negated_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:158
        Default::default()
    }

    /// `private PatternSyntax ParsePrimaryPattern(Precedence precedence, bool afterIs, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:187).
    pub(crate) fn parse_primary_pattern(
        &mut self,
        precedence: Precedence,
        after_is: bool,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:187
        Default::default()
    }

    /// `bool CanTokenFollowTypeInPattern(Precedence precedence)` (LanguageParser_Patterns.cs:257).
    pub(crate) fn can_token_follow_type_in_pattern(&mut self, precedence: Precedence) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:257
        Default::default()
    }

    /// `private PatternSyntax? ParsePatternContinued(TypeSyntax? type, Precedence precedence, bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:284).
    pub(crate) fn parse_pattern_continued(
        &mut self,
        type_: Green,
        precedence: Precedence,
        in_switch_arm_pattern: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:284
        Default::default()
    }

    /// `private VariableDesignationSyntax? TryParseSimpleDesignation(bool whenIsKeyword)` (LanguageParser_Patterns.cs:400).
    pub(crate) fn try_parse_simple_designation(&mut self, when_is_keyword: bool) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:400
        Default::default()
    }

    /// `private bool IsValidPatternDesignation(bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:407).
    pub(crate) fn is_valid_pattern_designation(&mut self, in_switch_arm_pattern: bool) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:407
        Default::default()
    }

    /// `private CSharpSyntaxNode ParseExpressionOrPatternForSwitchStatement()` (LanguageParser_Patterns.cs:459).
    pub(crate) fn parse_expression_or_pattern_for_switch_statement(&mut self) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:459
        Default::default()
    }

    /// `private CSharpSyntaxNode ConvertPatternToExpressionIfPossible(PatternSyntax pattern, bool permitTypeArguments = false)` (LanguageParser_Patterns.cs:468).
    pub(crate) fn convert_pattern_to_expression_if_possible(
        &mut self,
        pattern: Green,
        permit_type_arguments: bool,
    ) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:468
        Default::default()
    }

    /// `private bool ConvertTypeToExpression(TypeSyntax type, [NotNullWhen(true)] out ExpressionSyntax? expr, bool permitTypeArguments = false)` (LanguageParser_Patterns.cs:479).
    pub(crate) fn convert_type_to_expression(
        &mut self,
        type_: Green,
        expr: &mut Green,
        permit_type_arguments: bool,
    ) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:479
        Default::default()
    }

    /// `private bool LooksLikeTupleArrayType()` (LanguageParser_Patterns.cs:500).
    pub(crate) fn looks_like_tuple_array_type(&mut self) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:500
        Default::default()
    }

    /// `private PropertyPatternClauseSyntax ParsePropertyPatternClause()` (LanguageParser_Patterns.cs:509).
    pub(crate) fn parse_property_pattern_clause(&mut self) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:509
        Default::default()
    }

    /// `private SubpatternSyntax ParseSubpatternElement()` (LanguageParser_Patterns.cs:528).
    pub(crate) fn parse_subpattern_element(&mut self) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:528
        Default::default()
    }

    /// `private bool IsPossibleSubpatternElement()` (LanguageParser_Patterns.cs:552).
    pub(crate) fn is_possible_subpattern_element(&mut self) -> bool {
        // TODO(port): LanguageParser_Patterns.cs:552
        Default::default()
    }

    /// `private static PostSkipAction SkipBadPatternListTokens<T>( LanguageParser @this, ref SyntaxToken open, SeparatedSyntaxListBuilder<T> list, SyntaxKind expectedKind, SyntaxKind closeKind) where T : CSharpSyntaxNode` (LanguageParser_Patterns.cs:564).
    pub(crate) fn skip_bad_pattern_list_tokens(
        &mut self,
        open: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser_Patterns.cs:564
        Default::default()
    }

    /// `private SwitchExpressionSyntax ParseSwitchExpression(ExpressionSyntax governingExpression, SyntaxToken switchKeyword)` (LanguageParser_Patterns.cs:593).
    pub(crate) fn parse_switch_expression(&mut self, governing_expression: Green, switch_keyword: Green) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:593
        Default::default()
    }

    /// `private ListPatternSyntax ParseListPattern(bool inSwitchArmPattern)` (LanguageParser_Patterns.cs:660).
    pub(crate) fn parse_list_pattern(&mut self, in_switch_arm_pattern: bool) -> Green {
        // TODO(port): LanguageParser_Patterns.cs:660
        Default::default()
    }
}
