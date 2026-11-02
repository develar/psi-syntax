//! `LanguageParser.cs` 6163-8247: type parameters and constraints, type arguments and their scan, `ScanType`,
//! `ParseType`, tuples and function pointer types (lane A). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TypeParameterListSyntax ParseTypeParameterList()` (LanguageParser.cs:6163).
    pub(crate) fn parse_type_parameter_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:6163
        Default::default()
    }

    /// `private bool IsStartOfTypeParameter()` (LanguageParser.cs:6201).
    pub(crate) fn is_start_of_type_parameter(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6201
        Default::default()
    }

    /// `private TypeParameterSyntax ParseTypeParameter()` (LanguageParser.cs:6214).
    pub(crate) fn parse_type_parameter(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:6214
        Default::default()
    }

    /// `private SimpleNameSyntax ParseSimpleName(NameOptions options = NameOptions.None)` (LanguageParser.cs:6241).
    pub(crate) fn parse_simple_name(&mut self, options: NameOptions) -> Green {
        // TODO(port): LanguageParser.cs:6241
        Default::default()
    }
}

/// `ScanTypeArgumentListKind` (LanguageParser.cs:6279).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum ScanTypeArgumentListKind {
    #[default]
    NotTypeArgumentList,
    PossibleTypeArgumentList,
    DefiniteTypeArgumentList,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ScanTypeArgumentListKind ScanTypeArgumentList(NameOptions options)` (LanguageParser.cs:6286).
    pub(crate) fn scan_type_argument_list(&mut self, options: NameOptions) -> ScanTypeArgumentListKind {
        // TODO(port): LanguageParser.cs:6286
        Default::default()
    }

    /// `private ScanTypeFlags ScanPossibleTypeArgumentList( out SyntaxToken greaterThanToken, out bool isDefinitelyTypeArgumentList)` (LanguageParser.cs:6415).
    pub(crate) fn scan_possible_type_argument_list(
        &mut self,
        greater_than_token: &mut Green,
        is_definitely_type_argument_list: &mut bool,
    ) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:6415
        Default::default()
    }

    /// `private void ParseTypeArgumentList(out SyntaxToken open, SeparatedSyntaxListBuilder<TypeSyntax> types, out SyntaxToken close)` (LanguageParser.cs:6604).
    pub(crate) fn parse_type_argument_list(
        &mut self,
        open: &mut Green,
        types: &mut SyntaxListBuilder,
        close: &mut Green,
    ) {
        // TODO(port): LanguageParser.cs:6604
    }

    /// `private PostSkipAction SkipBadTypeArgumentListTokens(SeparatedSyntaxListBuilder<TypeSyntax> list, SyntaxKind expected)` (LanguageParser.cs:6729).
    pub(crate) fn skip_bad_type_argument_list_tokens(
        &mut self,
        list: &mut SyntaxListBuilder,
        expected: SyntaxKind,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:6729
        Default::default()
    }

    /// `private TypeSyntax ParseTypeArgument()` (LanguageParser.cs:6740).
    pub(crate) fn parse_type_argument(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:6740
        Default::default()
    }

    /// `private bool IsEndOfTypeArgumentList()` (LanguageParser.cs:6807).
    pub(crate) fn is_end_of_type_argument_list(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6807
        Default::default()
    }

    /// `private bool IsOpenName()` (LanguageParser.cs:6810).
    pub(crate) fn is_open_name(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6810
        Default::default()
    }

    /// `private void ParseMemberName( out ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, out SyntaxToken identifierOrThisOpt, out TypeParameterListSyntax typeParameterListOpt, bool isEvent)` (LanguageParser.cs:6820).
    pub(crate) fn parse_member_name(
        &mut self,
        explicit_interface_opt: &mut Green,
        identifier_or_this_opt: &mut Green,
        type_parameter_list_opt: &mut Green,
        is_event: bool,
    ) {
        // TODO(port): LanguageParser.cs:6820
    }

    /// `private void AccumulateExplicitInterfaceName(ref NameSyntax explicitInterfaceName, ref SyntaxToken separator)` (LanguageParser.cs:6958).
    pub(crate) fn accumulate_explicit_interface_name(
        &mut self,
        explicit_interface_name: &mut Green,
        separator: &mut Green,
    ) {
        // TODO(port): LanguageParser.cs:6958
    }

    /// `private bool IsOperatorStart(out ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, bool advanceParser = true)` (LanguageParser.cs:7005).
    pub(crate) fn is_operator_start(&mut self, explicit_interface_opt: &mut Green, advance_parser: bool) -> bool {
        // TODO(port): LanguageParser.cs:7005
        Default::default()
    }

    /// `private NameSyntax ParseAliasQualifiedName(NameOptions allowedParts = NameOptions.None)` (LanguageParser.cs:7087).
    pub(crate) fn parse_alias_qualified_name(&mut self, allowed_parts: NameOptions) -> Green {
        // TODO(port): LanguageParser.cs:7087
        Default::default()
    }

    /// `private NameSyntax ParseQualifiedName(NameOptions options = NameOptions.None)` (LanguageParser.cs:7095).
    pub(crate) fn parse_qualified_name(&mut self, options: NameOptions) -> Green {
        // TODO(port): LanguageParser.cs:7095
        Default::default()
    }

    /// `private NameSyntax ParseQualifiedNameRight( NameOptions options, NameSyntax left, SyntaxToken separator)` (LanguageParser.cs:7114).
    pub(crate) fn parse_qualified_name_right(&mut self, options: NameOptions, left: Green, separator: Green) -> Green {
        // TODO(port): LanguageParser.cs:7114
        Default::default()
    }

    /// `private SyntaxToken ConvertToMissingWithTrailingTrivia(SyntaxToken token, SyntaxKind expectedKind)` (LanguageParser.cs:7159).
    pub(crate) fn convert_to_missing_with_trailing_trivia(&mut self, token: Green, expected_kind: SyntaxKind) -> Green {
        // TODO(port): LanguageParser.cs:7159
        Default::default()
    }
}

/// `ScanTypeFlags` (LanguageParser.cs:7166).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct ScanTypeFlags(pub(crate) u32);

impl ScanTypeFlags {
    pub(crate) const NOT_TYPE: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const MUST_BE_TYPE: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const GENERIC_TYPE_OR_METHOD: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const GENERIC_TYPE_OR_EXPRESSION: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const NON_GENERIC_TYPE_OR_EXPRESSION: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const ALIAS_QUALIFIED_NAME: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const NULLABLE_TYPE: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const POINTER_OR_MULTIPLICATION: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const TUPLE_TYPE: ScanTypeFlags = ScanTypeFlags(0);
}

impl ScanTypeFlags {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: ScanTypeFlags) -> bool {
        self.0 & flag.0 != 0
    }

    /// `value & ~flag`.
    #[inline]
    pub(crate) fn without(self, flag: ScanTypeFlags) -> ScanTypeFlags {
        ScanTypeFlags(self.0 & !flag.0)
    }
}

impl std::ops::BitOr for ScanTypeFlags {
    type Output = ScanTypeFlags;

    #[inline]
    fn bitor(self, other: ScanTypeFlags) -> ScanTypeFlags {
        ScanTypeFlags(self.0 | other.0)
    }
}

impl std::ops::BitAnd for ScanTypeFlags {
    type Output = ScanTypeFlags;

    #[inline]
    fn bitand(self, other: ScanTypeFlags) -> ScanTypeFlags {
        ScanTypeFlags(self.0 & other.0)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsPossibleType()` (LanguageParser.cs:7218).
    pub(crate) fn is_possible_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:7218
        Default::default()
    }

    /// `private ScanTypeFlags ScanType(bool forPattern = false)` (LanguageParser.cs:7224).
    pub(crate) fn scan_type(&mut self, for_pattern: bool) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:7224
        Default::default()
    }

    /// `private ScanTypeFlags ScanType(out SyntaxToken lastTokenOfType, bool forPattern = false)` (LanguageParser.cs:7229).
    pub(crate) fn scan_type_with_for_pattern(
        &mut self,
        last_token_of_type: &mut Green,
        for_pattern: bool,
    ) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:7229
        Default::default()
    }

    /// `private void ScanNamedTypePart()` (LanguageParser.cs:7234).
    pub(crate) fn scan_named_type_part(&mut self) {
        // TODO(port): LanguageParser.cs:7234
    }

    /// `private ScanTypeFlags ScanNamedTypePart(out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7239).
    pub(crate) fn scan_named_type_part_with_last_token_of_type(
        &mut self,
        last_token_of_type: &mut Green,
    ) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:7239
        Default::default()
    }

    /// `private ScanTypeFlags ScanType(ParseTypeMode mode, out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7258).
    pub(crate) fn scan_type_with_last_token_of_type(
        &mut self,
        mode: ParseTypeMode,
        last_token_of_type: &mut Green,
    ) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:7258
        Default::default()
    }

    /// `private ScanTypeFlags ScanTupleType(out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7432).
    pub(crate) fn scan_tuple_type(&mut self, last_token_of_type: &mut Green) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:7432
        Default::default()
    }

    /// `private ScanTypeFlags ScanFunctionPointerType(out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7476).
    pub(crate) fn scan_function_pointer_type(&mut self, last_token_of_type: &mut Green) -> ScanTypeFlags {
        // TODO(port): LanguageParser.cs:7476
        Default::default()
    }

    /// `private static bool IsPredefinedType(SyntaxKind keyword)` (LanguageParser.cs:7595).
    pub(crate) fn is_predefined_type(keyword: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:7595
        Default::default()
    }

    /// `public TypeSyntax ParseTypeName()` (LanguageParser.cs:7600).
    pub(crate) fn parse_type_name(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:7600
        Default::default()
    }

    /// `private TypeSyntax ParseTypeOrVoid()` (LanguageParser.cs:7605).
    pub(crate) fn parse_type_or_void(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:7605
        Default::default()
    }
}

/// `ParseTypeMode` (LanguageParser.cs:7616).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum ParseTypeMode {
    #[default]
    Normal,
    Parameter,
    AfterIs,
    DefinitePattern,
    AfterOut,
    AfterRef,
    AfterTupleComma,
    AsExpression,
    NewExpression,
    FirstElementOfPossibleTupleLiteral,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TypeSyntax ParseType(ParseTypeMode mode = ParseTypeMode.Normal)` (LanguageParser.cs:7630).
    pub(crate) fn parse_type(&mut self, mode: ParseTypeMode) -> Green {
        // TODO(port): LanguageParser.cs:7630
        Default::default()
    }

    /// `private TypeSyntax ParseTypeCore(ParseTypeMode mode)` (LanguageParser.cs:7643).
    pub(crate) fn parse_type_core(&mut self, mode: ParseTypeMode) -> Green {
        // TODO(port): LanguageParser.cs:7643
        Default::default()
    }

    /// `private SyntaxToken TryEatNullableQualifierIfApplicable( TypeSyntax typeParsedSoFar, ParseTypeMode mode)` (LanguageParser.cs:7746).
    pub(crate) fn try_eat_nullable_qualifier_if_applicable(
        &mut self,
        type_parsed_so_far: Green,
        mode: ParseTypeMode,
    ) -> Green {
        // TODO(port): LanguageParser.cs:7746
        Default::default()
    }

    /// `private bool PointerTypeModsFollowedByRankAndDimensionSpecifier()` (LanguageParser.cs:7907).
    pub(crate) fn pointer_type_mods_followed_by_rank_and_dimension_specifier(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:7907
        Default::default()
    }

    /// `private ArrayRankSpecifierSyntax ParseArrayRankSpecifier(out bool sawNonOmittedSize)` (LanguageParser.cs:7924).
    pub(crate) fn parse_array_rank_specifier(&mut self, saw_non_omitted_size: &mut bool) -> Green {
        // TODO(port): LanguageParser.cs:7924
        Default::default()
    }

    /// `private TupleTypeSyntax ParseTupleType()` (LanguageParser.cs:7984).
    pub(crate) fn parse_tuple_type(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:7984
        Default::default()
    }

    /// `private TupleElementSyntax ParseTupleElement()` (LanguageParser.cs:8018).
    pub(crate) fn parse_tuple_element(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:8018
        Default::default()
    }

    /// `private PostSkipAction SkipBadArrayRankSpecifierTokens(ref SyntaxToken openBracket, SeparatedSyntaxListBuilder<ExpressionSyntax> list, SyntaxKind expected)` (LanguageParser.cs:8025).
    pub(crate) fn skip_bad_array_rank_specifier_tokens(
        &mut self,
        open_bracket: &mut Green,
        list: &mut SyntaxListBuilder,
        expected: SyntaxKind,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:8025
        Default::default()
    }

    /// `private TypeSyntax ParseUnderlyingType(ParseTypeMode mode, NameOptions options = NameOptions.None)` (LanguageParser.cs:8033).
    pub(crate) fn parse_underlying_type(&mut self, mode: ParseTypeMode, options: NameOptions) -> Green {
        // TODO(port): LanguageParser.cs:8033
        Default::default()
    }

    /// `private FunctionPointerTypeSyntax ParseFunctionPointerTypeSyntax()` (LanguageParser.cs:8068).
    pub(crate) fn parse_function_pointer_type_syntax(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:8068
        Default::default()
    }

    /// `private bool IsFunctionPointerStart()` (LanguageParser.cs:8228).
    pub(crate) fn is_function_pointer_start(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8228
        Default::default()
    }

    /// `private static bool IsPossibleFunctionPointerParameterListStart(SyntaxToken token)` (LanguageParser.cs:8231).
    pub(crate) fn is_possible_function_pointer_parameter_list_start(token: Green) -> bool {
        // TODO(port): LanguageParser.cs:8231
        Default::default()
    }

    /// `private TypeSyntax ParsePointerTypeMods(TypeSyntax type)` (LanguageParser.cs:8237).
    pub(crate) fn parse_pointer_type_mods(&mut self, type_: Green) -> Green {
        // TODO(port): LanguageParser.cs:8237
        Default::default()
    }
}
