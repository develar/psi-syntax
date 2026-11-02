//! `LanguageParser.cs` 236-6162: namespaces, extern aliases and usings, attributes, type and member declarations,
//! modifiers, accessors, parameter lists, fields, events, variable declarators, enums and names (lane A). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private BaseNamespaceDeclarationSyntax ParseNamespaceDeclaration( SyntaxList<AttributeListSyntax> attributeLists, SyntaxListBuilder modifiers)` (LanguageParser.cs:236).
    pub(crate) fn parse_namespace_declaration(
        &mut self,
        attribute_lists: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // TODO(port): LanguageParser.cs:236
        Default::default()
    }

    /// `private BaseNamespaceDeclarationSyntax ParseNamespaceDeclarationCore( SyntaxList<AttributeListSyntax> attributeLists, SyntaxListBuilder modifiers)` (LanguageParser.cs:247).
    pub(crate) fn parse_namespace_declaration_core(
        &mut self,
        attribute_lists: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // TODO(port): LanguageParser.cs:247
        Default::default()
    }

    /// `private static bool IsPossibleStartOfTypeDeclaration(SyntaxKind kind)` (LanguageParser.cs:331).
    pub(crate) fn is_possible_start_of_type_declaration(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:331
        Default::default()
    }

    /// `private static bool IsTypeModifierOrTypeKeyword(SyntaxKind kind)` (LanguageParser.cs:337).
    pub(crate) fn is_type_modifier_or_type_keyword(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:337
        Default::default()
    }

    /// `private void AddSkippedNamespaceText( ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes, CSharpSyntaxNode skippedSyntax)` (LanguageParser.cs:361).
    pub(crate) fn add_skipped_namespace_text(
        &mut self,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        skipped_syntax: Green,
    ) {
        // TODO(port): LanguageParser.cs:361
    }
}

/// `NamespaceParts` (LanguageParser.cs:399).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum NamespaceParts {
    #[default]
    None = 0,
    ExternAliases = 1,
    Usings = 2,
    GlobalAttributes = 3,
    MembersAndStatements = 4,
    TypesAndNamespaces = 5,
    TopLevelStatementsAfterTypesAndNamespaces = 6,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private void ParseNamespaceBody( [NotNullIfNotNull(nameof(openBraceOrSemicolon))] ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes, SyntaxKind parentKind)` (LanguageParser.cs:410).
    pub(crate) fn parse_namespace_body(
        &mut self,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        parent_kind: SyntaxKind,
    ) {
        // TODO(port): LanguageParser.cs:410
        // Until then every token up to the end of file is skipped into `initial_bad_nodes` (the compilation unit puts
        // them before its end of file as skipped tokens), so the tree stays lossless.
        while self.current_kind() != SyntaxKind::EndOfFileToken {
            let token = self.eat_token();
            let pool = &mut self.pool;
            initial_bad_nodes.get_or_insert_with(|| pool.allocate()).push(token);
        }
    }

    /// `private static bool IsMemberDeclarationOnlyValidWithinTypeDeclaration(MemberDeclarationSyntax? memberDeclaration)` (LanguageParser.cs:547).
    pub(crate) fn is_member_declaration_only_valid_within_type_declaration(member_declaration: Green) -> bool {
        // TODO(port): LanguageParser.cs:547
        Default::default()
    }

    /// `private void ParseNamespaceBodyWorker( [NotNullIfNotNull(nameof(openBraceOrSemicolon))] ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes, SyntaxKind parentKind, out bool sawMemberDeclarationOnlyValidWithinTypeDeclaration)` (LanguageParser.cs:562).
    pub(crate) fn parse_namespace_body_worker(
        &mut self,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        parent_kind: SyntaxKind,
        saw_member_declaration_only_valid_within_type_declaration: &mut bool,
    ) {
        // TODO(port): LanguageParser.cs:562
    }

    /// `private static void AddIncompleteMembers(ref SyntaxListBuilder<MemberDeclarationSyntax> incompleteMembers, ref NamespaceBodyBuilder body)` (LanguageParser.cs:848).
    pub(crate) fn add_incomplete_members(incomplete_members: &mut SyntaxListBuilder, body: &mut NamespaceBodyBuilder) {
        // TODO(port): LanguageParser.cs:848
    }

    /// `private void ReduceIncompleteMembers( ref SyntaxListBuilder<MemberDeclarationSyntax> incompleteMembers, ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes)` (LanguageParser.cs:857).
    pub(crate) fn reduce_incomplete_members(
        &mut self,
        incomplete_members: &mut SyntaxListBuilder,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
    ) {
        // TODO(port): LanguageParser.cs:857
    }

    /// `private bool IsPossibleNamespaceMemberDeclaration()` (LanguageParser.cs:869).
    pub(crate) fn is_possible_namespace_member_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:869
        Default::default()
    }

    /// `private bool IsPartialInNamespaceMemberDeclaration()` (LanguageParser.cs:884).
    pub(crate) fn is_partial_in_namespace_member_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:884
        Default::default()
    }

    /// `public bool IsEndOfNamespace()` (LanguageParser.cs:901).
    pub(crate) fn is_end_of_namespace(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:901
        Default::default()
    }

    /// `public bool IsGobalAttributesTerminator()` (LanguageParser.cs:906).
    pub(crate) fn is_gobal_attributes_terminator(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:906
        Default::default()
    }

    /// `private bool IsNamespaceMemberStartOrStop()` (LanguageParser.cs:912).
    pub(crate) fn is_namespace_member_start_or_stop(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:912
        Default::default()
    }

    /// `private bool ScanExternAliasDirective()` (LanguageParser.cs:921).
    pub(crate) fn scan_extern_alias_directive(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:921
        Default::default()
    }

    /// `private ExternAliasDirectiveSyntax ParseExternAliasDirective()` (LanguageParser.cs:934).
    pub(crate) fn parse_extern_alias_directive(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:934
        Default::default()
    }

    /// `private NameEqualsSyntax ParseNameEquals()` (LanguageParser.cs:950).
    pub(crate) fn parse_name_equals(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:950
        Default::default()
    }

    /// `private UsingDirectiveSyntax ParseUsingDirective()` (LanguageParser.cs:958).
    pub(crate) fn parse_using_directive(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:958
        Default::default()
    }

    /// `private bool IsPossibleGlobalAttributeDeclaration()` (LanguageParser.cs:1028).
    pub(crate) fn is_possible_global_attribute_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1028
        Default::default()
    }

    /// `private static bool IsGlobalAttributeTarget(SyntaxToken token)` (LanguageParser.cs:1035).
    pub(crate) fn is_global_attribute_target(token: Green) -> bool {
        // TODO(port): LanguageParser.cs:1035
        Default::default()
    }

    /// `private bool IsPossibleAttributeDeclaration()` (LanguageParser.cs:1047).
    pub(crate) fn is_possible_attribute_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1047
        Default::default()
    }

    /// `private SyntaxList<AttributeListSyntax> ParseAttributeDeclarations(bool inExpressionContext)` (LanguageParser.cs:1075).
    pub(crate) fn parse_attribute_declarations(&mut self, in_expression_context: bool) -> Green {
        // TODO(port): LanguageParser.cs:1075
        Default::default()
    }

    /// `private bool IsAttributeDeclarationTerminator()` (LanguageParser.cs:1110).
    pub(crate) fn is_attribute_declaration_terminator(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1110
        Default::default()
    }

    /// `private bool IsAttributeTarget()` (LanguageParser.cs:1116).
    pub(crate) fn is_attribute_target(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1116
        Default::default()
    }

    /// `private AttributeListSyntax? TryParseAttributeDeclaration(bool inExpressionContext)` (LanguageParser.cs:1119).
    pub(crate) fn try_parse_attribute_declaration(&mut self, in_expression_context: bool) -> Green {
        // TODO(port): LanguageParser.cs:1119
        Default::default()
    }

    /// `private bool IsPossibleAttribute()` (LanguageParser.cs:1192).
    pub(crate) fn is_possible_attribute(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1192
        Default::default()
    }

    /// `private AttributeSyntax ParseAttribute()` (LanguageParser.cs:1197).
    pub(crate) fn parse_attribute(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:1197
        Default::default()
    }

    /// `internal AttributeArgumentListSyntax? ParseAttributeArgumentList()` (LanguageParser.cs:1209).
    pub(crate) fn parse_attribute_argument_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:1209
        Default::default()
    }

    /// `private bool IsPossibleAttributeArgument()` (LanguageParser.cs:1264).
    pub(crate) fn is_possible_attribute_argument(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1264
        Default::default()
    }

    /// `private AttributeArgumentSyntax ParseAttributeArgument()` (LanguageParser.cs:1269).
    pub(crate) fn parse_attribute_argument(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:1269
        Default::default()
    }

    /// `private static DeclarationModifiers GetModifierExcludingScoped(SyntaxToken token)` (LanguageParser.cs:1299).
    pub(crate) fn get_modifier_excluding_scoped(token: Green) -> DeclarationModifiers {
        // TODO(port): LanguageParser.cs:1299
        Default::default()
    }

    /// `internal static DeclarationModifiers GetModifierExcludingScoped(SyntaxKind kind, SyntaxKind contextualKind)` (LanguageParser.cs:1302).
    pub(crate) fn get_modifier_excluding_scoped_with_contextual_kind(
        kind: SyntaxKind,
        contextual_kind: SyntaxKind,
    ) -> DeclarationModifiers {
        // TODO(port): LanguageParser.cs:1302
        Default::default()
    }

    /// `private void ParseModifiers(SyntaxListBuilder tokens, bool forAccessors, bool forTopLevelStatements, out bool isPossibleTypeDeclaration)` (LanguageParser.cs:1363).
    pub(crate) fn parse_modifiers(
        &mut self,
        tokens: &mut SyntaxListBuilder,
        for_accessors: bool,
        for_top_level_statements: bool,
        is_possible_type_declaration: &mut bool,
    ) {
        // TODO(port): LanguageParser.cs:1363
    }

    /// `private bool ShouldContextualKeywordBeTreatedAsModifier(bool parsingStatementNotDeclaration)` (LanguageParser.cs:1546).
    pub(crate) fn should_contextual_keyword_be_treated_as_modifier(
        &mut self,
        parsing_statement_not_declaration: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:1546
        Default::default()
    }

    /// `private static bool IsNonContextualModifier(SyntaxToken nextToken)` (LanguageParser.cs:1659).
    pub(crate) fn is_non_contextual_modifier(next_token: Green) -> bool {
        // TODO(port): LanguageParser.cs:1659
        Default::default()
    }

    /// `private bool IsPartialType()` (LanguageParser.cs:1664).
    pub(crate) fn is_partial_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1664
        Default::default()
    }

    /// `private bool IsPartialMember()` (LanguageParser.cs:1700).
    pub(crate) fn is_partial_member(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1700
        Default::default()
    }

    /// `private bool IsPossibleMemberName()` (LanguageParser.cs:1733).
    pub(crate) fn is_possible_member_name(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:1733
        Default::default()
    }

    /// `private MemberDeclarationSyntax ParseTypeDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:1751).
    pub(crate) fn parse_type_declaration(&mut self, attributes: Green, modifiers: &mut SyntaxListBuilder) -> Green {
        // TODO(port): LanguageParser.cs:1751
        Default::default()
    }

    /// `private TypeDeclarationSyntax ParseMainTypeDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:1784).
    pub(crate) fn parse_main_type_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // TODO(port): LanguageParser.cs:1784
        Default::default()
    }

    /// `private void SkipBadMemberListTokens(ref SyntaxToken openBrace, SyntaxListBuilder members)` (LanguageParser.cs:2078).
    pub(crate) fn skip_bad_member_list_tokens(&mut self, open_brace: &mut Green, members: &mut SyntaxListBuilder) {
        // TODO(port): LanguageParser.cs:2078
    }

    /// `private void SkipBadMemberListTokens(ref GreenNode previousNode)` (LanguageParser.cs:2094).
    pub(crate) fn skip_bad_member_list_tokens_with_previous_node(&mut self, previous_node: &mut Green) {
        // TODO(port): LanguageParser.cs:2094
    }

    /// `private bool IsPossibleMemberStartOrStop()` (LanguageParser.cs:2154).
    pub(crate) fn is_possible_member_start_or_stop(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:2154
        Default::default()
    }

    /// `private bool IsPossibleAggregateClauseStartOrStop()` (LanguageParser.cs:2159).
    pub(crate) fn is_possible_aggregate_clause_start_or_stop(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:2159
        Default::default()
    }

    /// `private BaseListSyntax ParseBaseList()` (LanguageParser.cs:2165).
    pub(crate) fn parse_base_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:2165
        Default::default()
    }

    /// `private bool IsCurrentTokenWhereOfConstraintClause()` (LanguageParser.cs:2234).
    pub(crate) fn is_current_token_where_of_constraint_clause(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:2234
        Default::default()
    }

    /// `private void ParseTypeParameterConstraintClauses(SyntaxListBuilder list)` (LanguageParser.cs:2242).
    pub(crate) fn parse_type_parameter_constraint_clauses(&mut self, list: &mut SyntaxListBuilder) {
        // TODO(port): LanguageParser.cs:2242
    }

    /// `private TypeParameterConstraintClauseSyntax ParseTypeParameterConstraintClause()` (LanguageParser.cs:2250).
    pub(crate) fn parse_type_parameter_constraint_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:2250
        Default::default()
    }

    /// `private bool IsPossibleTypeParameterConstraint()` (LanguageParser.cs:2329).
    pub(crate) fn is_possible_type_parameter_constraint(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:2329
        Default::default()
    }

    /// `private TypeParameterConstraintSyntax ParseTypeParameterConstraint()` (LanguageParser.cs:2346).
    pub(crate) fn parse_type_parameter_constraint(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:2346
        Default::default()
    }

    /// `private bool IsPossibleMemberStart()` (LanguageParser.cs:2422).
    pub(crate) fn is_possible_member_start(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:2422
        Default::default()
    }

    /// `private static bool CanStartMember(SyntaxKind kind)` (LanguageParser.cs:2427).
    pub(crate) fn can_start_member(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:2427
        Default::default()
    }

    /// `private bool IsTypeDeclarationStart()` (LanguageParser.cs:2483).
    pub(crate) fn is_type_declaration_start(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:2483
        Default::default()
    }

    /// `private bool CanReuseMemberDeclaration(SyntaxKind kind, bool isGlobal)` (LanguageParser.cs:2529).
    pub(crate) fn can_reuse_member_declaration(&mut self, kind: SyntaxKind, is_global: bool) -> bool {
        // TODO(port): LanguageParser.cs:2529
        Default::default()
    }

    /// `public MemberDeclarationSyntax ParseMemberDeclaration()` (LanguageParser.cs:2570).
    pub(crate) fn parse_member_declaration(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:2570
        Default::default()
    }

    /// `internal MemberDeclarationSyntax ParseMemberDeclarationOrStatement(SyntaxKind parentKind)` (LanguageParser.cs:2590).
    pub(crate) fn parse_member_declaration_or_statement(&mut self, parent_kind: SyntaxKind) -> Green {
        // TODO(port): LanguageParser.cs:2590
        Default::default()
    }

    /// `private MemberDeclarationSyntax ParseMemberDeclarationOrStatementCore(SyntaxKind parentKind)` (LanguageParser.cs:2605).
    pub(crate) fn parse_member_declaration_or_statement_core(&mut self, parent_kind: SyntaxKind) -> Green {
        // TODO(port): LanguageParser.cs:2605
        Default::default()
    }

    /// `private bool IsMisplacedModifier(SyntaxListBuilder modifiers, SyntaxList<AttributeListSyntax> attributes, TypeSyntax type, out MemberDeclarationSyntax result)` (LanguageParser.cs:3039).
    pub(crate) fn is_misplaced_modifier(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        attributes: Green,
        type_: Green,
        result: &mut Green,
    ) -> bool {
        // TODO(port): LanguageParser.cs:3039
        Default::default()
    }

    /// `private bool IsNoneOrIncompleteMember( SyntaxKind parentKind, SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifierOrThisOpt, TypeParameterListSyntax typeParameterListOpt, out MemberDeclarationSyntax result)` (LanguageParser.cs:3064).
    pub(crate) fn is_none_or_incomplete_member(
        &mut self,
        parent_kind: SyntaxKind,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier_or_this_opt: Green,
        type_parameter_list_opt: Green,
        result: &mut Green,
    ) -> bool {
        // TODO(port): LanguageParser.cs:3064
        Default::default()
    }

    /// `private bool ReconsideredTypeAsAsyncModifier(ref SyntaxListBuilder modifiers, ref TypeSyntax type, ref ResetPoint afterTypeResetPoint, ref ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, ref SyntaxToken identifierOrThisOpt, ref TypeParameterListSyntax typeParameterListOpt)` (LanguageParser.cs:3168).
    pub(crate) fn reconsidered_type_as_async_modifier(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        type_: &mut Green,
        after_type_reset_point: &mut ResetPoint,
        explicit_interface_opt: &mut Green,
        identifier_or_this_opt: &mut Green,
        type_parameter_list_opt: &mut Green,
    ) -> bool {
        // TODO(port): LanguageParser.cs:3168
        Default::default()
    }

    /// `private bool TryParseIndexerOrPropertyDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifierOrThisOpt, TypeParameterListSyntax typeParameterListOpt, out MemberDeclarationSyntax result)` (LanguageParser.cs:3191).
    pub(crate) fn try_parse_indexer_or_property_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier_or_this_opt: Green,
        type_parameter_list_opt: Green,
        result: &mut Green,
    ) -> bool {
        // TODO(port): LanguageParser.cs:3191
        Default::default()
    }

    /// `private bool IsStartOfPropertyBody(int index)` (LanguageParser.cs:3219).
    pub(crate) fn is_start_of_property_body(&mut self, index: i32) -> bool {
        // TODO(port): LanguageParser.cs:3219
        Default::default()
    }

    /// `internal MemberDeclarationSyntax ParseMemberDeclaration(SyntaxKind parentKind)` (LanguageParser.cs:3239).
    pub(crate) fn parse_member_declaration_with_parent_kind(&mut self, parent_kind: SyntaxKind) -> Green {
        // TODO(port): LanguageParser.cs:3239
        Default::default()
    }

    /// `private MemberDeclarationSyntax ParseMemberDeclarationCore(SyntaxKind parentKind)` (LanguageParser.cs:3254).
    pub(crate) fn parse_member_declaration_core(&mut self, parent_kind: SyntaxKind) -> Green {
        // TODO(port): LanguageParser.cs:3254
        Default::default()
    }

    /// `private bool IsExtensionContainerStart()` (LanguageParser.cs:3412).
    pub(crate) fn is_extension_container_start(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3412
        Default::default()
    }

    /// `private static bool ReconsiderTypeAsAsyncModifier( ref SyntaxListBuilder modifiers, TypeSyntax type, SyntaxToken identifierOrThisOpt)` (LanguageParser.cs:3422).
    pub(crate) fn reconsider_type_as_async_modifier(
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        identifier_or_this_opt: Green,
    ) -> bool {
        // TODO(port): LanguageParser.cs:3422
        Default::default()
    }

    /// `private bool IsFieldDeclaration(bool isEvent, bool isGlobalScriptLevel)` (LanguageParser.cs:3445).
    pub(crate) fn is_field_declaration(&mut self, is_event: bool, is_global_script_level: bool) -> bool {
        // TODO(port): LanguageParser.cs:3445
        Default::default()
    }

    /// `private bool IsOperatorKeyword()` (LanguageParser.cs:3488).
    pub(crate) fn is_operator_keyword(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3488
        Default::default()
    }

    /// `public static bool IsComplete(CSharpSyntaxNode node)` (LanguageParser.cs:3493).
    pub(crate) fn is_complete(node: Green) -> bool {
        // TODO(port): LanguageParser.cs:3493
        Default::default()
    }

    /// `private ConstructorDeclarationSyntax ParseConstructorDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:3525).
    pub(crate) fn parse_constructor_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // TODO(port): LanguageParser.cs:3525
        Default::default()
    }

    /// `private ConstructorInitializerSyntax? TryParseConstructorInitializer()` (LanguageParser.cs:3546).
    pub(crate) fn try_parse_constructor_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:3546
        Default::default()
    }

    /// `private ConstructorInitializerSyntax ParseConstructorInitializer()` (LanguageParser.cs:3560).
    pub(crate) fn parse_constructor_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:3560
        Default::default()
    }

    /// `private DestructorDeclarationSyntax ParseDestructorDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:3588).
    pub(crate) fn parse_destructor_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // TODO(port): LanguageParser.cs:3588
        Default::default()
    }

    /// `private void ParseBlockAndExpressionBodiesWithSemicolon( out BlockSyntax blockBody, out ArrowExpressionClauseSyntax expressionBody, out SyntaxToken semicolon, bool parseSemicolonAfterBlock = true)` (LanguageParser.cs:3609).
    pub(crate) fn parse_block_and_expression_bodies_with_semicolon(
        &mut self,
        block_body: &mut Green,
        expression_body: &mut Green,
        semicolon: &mut Green,
        parse_semicolon_after_block: bool,
    ) {
        // TODO(port): LanguageParser.cs:3609
    }

    /// `private bool IsEndOfTypeParameterList()` (LanguageParser.cs:3649).
    pub(crate) fn is_end_of_type_parameter_list(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3649
        Default::default()
    }

    /// `private bool IsEndOfMethodSignature()` (LanguageParser.cs:3678).
    pub(crate) fn is_end_of_method_signature(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3678
        Default::default()
    }

    /// `private bool IsEndOfTypeSignature()` (LanguageParser.cs:3681).
    pub(crate) fn is_end_of_type_signature(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3681
        Default::default()
    }

    /// `private bool IsEndOfNameInExplicitInterface()` (LanguageParser.cs:3686).
    pub(crate) fn is_end_of_name_in_explicit_interface(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3686
        Default::default()
    }

    /// `private bool IsEndOfFunctionPointerParameterList(bool errored)` (LanguageParser.cs:3689).
    pub(crate) fn is_end_of_function_pointer_parameter_list(&mut self, errored: bool) -> bool {
        // TODO(port): LanguageParser.cs:3689
        Default::default()
    }

    /// `private bool IsEndOfFunctionPointerCallingConvention()` (LanguageParser.cs:3692).
    pub(crate) fn is_end_of_function_pointer_calling_convention(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3692
        Default::default()
    }

    /// `private MethodDeclarationSyntax ParseMethodDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifier, TypeParameterListSyntax typeParameterList)` (LanguageParser.cs:3695).
    pub(crate) fn parse_method_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier: Green,
        type_parameter_list: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:3695
        Default::default()
    }

    /// `private TypeSyntax ParseReturnType()` (LanguageParser.cs:3754).
    pub(crate) fn parse_return_type(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:3754
        Default::default()
    }

    /// `private bool IsEndOfReturnType()` (LanguageParser.cs:3763).
    pub(crate) fn is_end_of_return_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:3763
        Default::default()
    }

    /// `private ConversionOperatorDeclarationSyntax TryParseConversionOperatorDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:3776).
    pub(crate) fn try_parse_conversion_operator_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // TODO(port): LanguageParser.cs:3776
        Default::default()
    }

    /// `private SyntaxToken TryEatCheckedOrHandleUnchecked(ref SyntaxToken operatorKeyword)` (LanguageParser.cs:4004).
    pub(crate) fn try_eat_checked_or_handle_unchecked(&mut self, operator_keyword: &mut Green) -> Green {
        // TODO(port): LanguageParser.cs:4004
        Default::default()
    }

    /// `private MemberDeclarationSyntax ParseOperatorDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt)` (LanguageParser.cs:4017).
    pub(crate) fn parse_operator_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:4017
        Default::default()
    }

    /// `private IndexerDeclarationSyntax ParseIndexerDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken thisKeyword, TypeParameterListSyntax typeParameterList)` (LanguageParser.cs:4217).
    pub(crate) fn parse_indexer_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        this_keyword: Green,
        type_parameter_list: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:4217
        Default::default()
    }

    /// `private PropertyDeclarationSyntax ParsePropertyDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifier, TypeParameterListSyntax typeParameterList)` (LanguageParser.cs:4277).
    pub(crate) fn parse_property_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier: Green,
        type_parameter_list: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:4277
        Default::default()
    }

    // `ParserSyntaxContextResetter` (LanguageParser.cs:4344): TODO(port): LanguageParser.cs:4344
}

/// `AccessorDeclaringKind` (LanguageParser.cs:4393).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum AccessorDeclaringKind {
    #[default]
    Property,
    Indexer,
    Event,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private AccessorListSyntax ParseAccessorList(AccessorDeclaringKind declaringKind)` (LanguageParser.cs:4400).
    pub(crate) fn parse_accessor_list(&mut self, declaring_kind: AccessorDeclaringKind) -> Green {
        // TODO(port): LanguageParser.cs:4400
        Default::default()
    }

    /// `private ArrowExpressionClauseSyntax ParseArrowExpressionClause()` (LanguageParser.cs:4437).
    pub(crate) fn parse_arrow_expression_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:4437
        Default::default()
    }

    /// `private ExpressionSyntax ParsePossibleRefExpression()` (LanguageParser.cs:4444).
    pub(crate) fn parse_possible_ref_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:4444
        Default::default()
    }

    /// `private PostSkipAction SkipBadAccessorListTokens(ref SyntaxToken openBrace, SyntaxListBuilder<AccessorDeclarationSyntax> list, ErrorCode error)` (LanguageParser.cs:4455).
    pub(crate) fn skip_bad_accessor_list_tokens(
        &mut self,
        open_brace: &mut Green,
        list: &mut SyntaxListBuilder,
        error: ErrorCode,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4455
        Default::default()
    }

    /// `private bool IsPossibleAccessor()` (LanguageParser.cs:4463).
    pub(crate) fn is_possible_accessor(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:4463
        Default::default()
    }

    /// `private bool IsPossibleAccessorModifier()` (LanguageParser.cs:4473).
    pub(crate) fn is_possible_accessor_modifier(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:4473
        Default::default()
    }
}

/// `PostSkipAction` (LanguageParser.cs:4523).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum PostSkipAction {
    #[default]
    Continue,
    Abort,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private PostSkipAction SkipBadSeparatedListTokensWithExpectedKind<T, TNode>( ref T startToken, SeparatedSyntaxListBuilder<TNode> list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, SyntaxKind, bool> abortFunction, SyntaxKind expected, SyntaxKind closeKind = SyntaxKind.None) where T : CSharpSyntaxNode where TNode : CSharpSyntaxNode` (LanguageParser.cs:4529).
    pub(crate) fn skip_bad_separated_list_tokens_with_expected_kind(
        &mut self,
        start_token: &mut Green,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>, SyntaxKind) -> bool,
        expected: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4529
        Default::default()
    }

    /// `private PostSkipAction SkipBadListTokensWithErrorCode<T, TNode>( ref T startToken, SyntaxListBuilder<TNode> list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, bool> abortFunction, ErrorCode error) where T : CSharpSyntaxNode where TNode : CSharpSyntaxNode` (LanguageParser.cs:4550).
    pub(crate) fn skip_bad_list_tokens_with_error_code(
        &mut self,
        start_token: &mut Green,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        error: ErrorCode,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4550
        Default::default()
    }

    /// `private PostSkipAction SkipBadListTokensWithExpectedKindHelper( SyntaxListBuilder list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, SyntaxKind, bool> abortFunction, SyntaxKind expected, SyntaxKind closeKind, out GreenNode trailingTrivia)` (LanguageParser.cs:4572).
    pub(crate) fn skip_bad_list_tokens_with_expected_kind_helper(
        &mut self,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>, SyntaxKind) -> bool,
        expected: SyntaxKind,
        close_kind: SyntaxKind,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4572
        Default::default()
    }

    /// `private PostSkipAction SkipBadListTokensWithErrorCodeHelper<TNode>( SyntaxListBuilder<TNode> list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, bool> abortFunction, ErrorCode error, out GreenNode trailingTrivia) where TNode : CSharpSyntaxNode` (LanguageParser.cs:4597).
    pub(crate) fn skip_bad_list_tokens_with_error_code_helper(
        &mut self,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        error: ErrorCode,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4597
        Default::default()
    }

    /// `private PostSkipAction SkipBadTokensWithExpectedKind( Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, SyntaxKind, bool> abortFunction, SyntaxKind expected, SyntaxKind closeKind, out GreenNode trailingTrivia)` (LanguageParser.cs:4621).
    pub(crate) fn skip_bad_tokens_with_expected_kind(
        &mut self,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>, SyntaxKind) -> bool,
        expected: SyntaxKind,
        close_kind: SyntaxKind,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4621
        Default::default()
    }

    /// `private PostSkipAction SkipBadTokensWithErrorCode( Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, bool> abortFunction, ErrorCode errorCode, out GreenNode trailingTrivia)` (LanguageParser.cs:4648).
    pub(crate) fn skip_bad_tokens_with_error_code(
        &mut self,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        error_code: ErrorCode,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:4648
        Default::default()
    }

    /// `private AccessorDeclarationSyntax ParseAccessorDeclaration(AccessorDeclaringKind declaringKind)` (LanguageParser.cs:4674).
    pub(crate) fn parse_accessor_declaration(&mut self, declaring_kind: AccessorDeclaringKind) -> Green {
        // TODO(port): LanguageParser.cs:4674
        Default::default()
    }

    /// `private SyntaxToken EatAccessorSemicolon()` (LanguageParser.cs:4773).
    pub(crate) fn eat_accessor_semicolon(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:4773
        Default::default()
    }

    /// `private static SyntaxKind GetAccessorKind(SyntaxToken accessorName)` (LanguageParser.cs:4779).
    pub(crate) fn get_accessor_kind(accessor_name: Green) -> SyntaxKind {
        // TODO(port): LanguageParser.cs:4779
        Default::default()
    }

    /// `internal ParameterListSyntax ParseParenthesizedParameterList(bool forExtensionOrUnion)` (LanguageParser.cs:4792).
    pub(crate) fn parse_parenthesized_parameter_list(&mut self, for_extension_or_union: bool) -> Green {
        // TODO(port): LanguageParser.cs:4792
        Default::default()
    }

    /// `internal BracketedParameterListSyntax ParseBracketedParameterList()` (LanguageParser.cs:4803).
    pub(crate) fn parse_bracketed_parameter_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:4803
        Default::default()
    }

    /// `private static bool CanReuseParameterList(Syntax.ParameterListSyntax list, bool identifierIsOptional)` (LanguageParser.cs:4814).
    pub(crate) fn can_reuse_parameter_list(list: Green, identifier_is_optional: bool) -> bool {
        // TODO(port): LanguageParser.cs:4814
        Default::default()
    }

    /// `private static bool CanReuseBracketedParameterList(Syntax.BracketedParameterListSyntax list)` (LanguageParser.cs:4842).
    pub(crate) fn can_reuse_bracketed_parameter_list(list: Green) -> bool {
        // TODO(port): LanguageParser.cs:4842
        Default::default()
    }

    /// `private SeparatedSyntaxList<ParameterSyntax> ParseParameterList( out SyntaxToken open, out SyntaxToken close, SyntaxKind openKind, SyntaxKind closeKind, bool forExtensionOrUnion)` (LanguageParser.cs:4870).
    pub(crate) fn parse_parameter_list(
        &mut self,
        open: &mut Green,
        close: &mut Green,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
        for_extension_or_union: bool,
    ) -> Green {
        // TODO(port): LanguageParser.cs:4870
        Default::default()
    }

    /// `private bool IsEndOfParameterList()` (LanguageParser.cs:4911).
    pub(crate) fn is_end_of_parameter_list(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:4911
        Default::default()
    }

    /// `private bool IsPossibleParameter()` (LanguageParser.cs:4916).
    pub(crate) fn is_possible_parameter(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:4916
        Default::default()
    }

    /// `private static bool CanReuseParameter(CSharp.Syntax.ParameterSyntax parameter, bool identifierIsOptional)` (LanguageParser.cs:4936).
    pub(crate) fn can_reuse_parameter(parameter: Green, identifier_is_optional: bool) -> bool {
        // TODO(port): LanguageParser.cs:4936
        Default::default()
    }

    /// `private ParameterSyntax ParseParameter(bool identifierIsOptional)` (LanguageParser.cs:4980).
    pub(crate) fn parse_parameter(&mut self, identifier_is_optional: bool) -> Green {
        // TODO(port): LanguageParser.cs:4980
        Default::default()
    }

    /// `internal static bool NoTriviaBetween(SyntaxToken token1, SyntaxToken token2)` (LanguageParser.cs:5033).
    pub(crate) fn no_trivia_between(token1: Green, token2: Green) -> bool {
        // TODO(port): LanguageParser.cs:5033
        Default::default()
    }

    /// `private static bool IsParameterModifierIncludingScoped(SyntaxToken token)` (LanguageParser.cs:5038).
    pub(crate) fn is_parameter_modifier_including_scoped(token: Green) -> bool {
        // TODO(port): LanguageParser.cs:5038
        Default::default()
    }

    /// `private static bool IsParameterModifierExcludingScoped(SyntaxToken token)` (LanguageParser.cs:5041).
    pub(crate) fn is_parameter_modifier_excluding_scoped(token: Green) -> bool {
        // TODO(port): LanguageParser.cs:5041
        Default::default()
    }

    /// `private void ParseParameterModifiers(SyntaxListBuilder modifiers, bool isFunctionPointerParameter, bool isLambdaParameter)` (LanguageParser.cs:5057).
    pub(crate) fn parse_parameter_modifiers(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        is_function_pointer_parameter: bool,
        is_lambda_parameter: bool,
    ) {
        // TODO(port): LanguageParser.cs:5057
    }

    /// `private FieldDeclarationSyntax ParseFixedSizeBufferDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxKind parentKind)` (LanguageParser.cs:5106).
    pub(crate) fn parse_fixed_size_buffer_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        parent_kind: SyntaxKind,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5106
        Default::default()
    }

    /// `private MemberDeclarationSyntax ParseEventDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxKind parentKind)` (LanguageParser.cs:5124).
    pub(crate) fn parse_event_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        parent_kind: SyntaxKind,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5124
        Default::default()
    }

    /// `private EventDeclarationSyntax ParseEventDeclarationWithAccessors( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxToken eventToken, TypeSyntax type)` (LanguageParser.cs:5139).
    pub(crate) fn parse_event_declaration_with_accessors(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        event_token: Green,
        type_: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5139
        Default::default()
    }

    /// `private TNode EatUnexpectedTrailingSemicolon<TNode>(TNode decl) where TNode : CSharpSyntaxNode` (LanguageParser.cs:5229).
    pub(crate) fn eat_unexpected_trailing_semicolon(&mut self, decl: Green) -> Green {
        // TODO(port): LanguageParser.cs:5229
        Default::default()
    }

    /// `private FieldDeclarationSyntax ParseNormalFieldDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, SyntaxKind parentKind)` (LanguageParser.cs:5242).
    pub(crate) fn parse_normal_field_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        parent_kind: SyntaxKind,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5242
        Default::default()
    }

    /// `private EventFieldDeclarationSyntax ParseEventFieldDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxToken eventToken, TypeSyntax type, SyntaxKind parentKind)` (LanguageParser.cs:5264).
    pub(crate) fn parse_event_field_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        event_token: Green,
        type_: Green,
        parent_kind: SyntaxKind,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5264
        Default::default()
    }

    /// `private bool IsEndOfFieldDeclaration()` (LanguageParser.cs:5299).
    pub(crate) fn is_end_of_field_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:5299
        Default::default()
    }

    /// `private SeparatedSyntaxList<VariableDeclaratorSyntax> ParseFieldDeclarationVariableDeclarators( TypeSyntax type, VariableFlags flags, SyntaxKind parentKind)` (LanguageParser.cs:5304).
    pub(crate) fn parse_field_declaration_variable_declarators(
        &mut self,
        type_: Green,
        flags: VariableFlags,
        parent_kind: SyntaxKind,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5304
        Default::default()
    }

    /// `private void ParseVariableDeclarators( TypeSyntax type, VariableFlags flags, SeparatedSyntaxListBuilder<VariableDeclaratorSyntax> variables, bool variableDeclarationsExpected, bool allowLocalFunctions, bool stopOnCloseParen, SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> mods, out LocalFunctionStatementSyntax localFunction)` (LanguageParser.cs:5336).
    pub(crate) fn parse_variable_declarators(
        &mut self,
        type_: Green,
        flags: VariableFlags,
        variables: &mut SyntaxListBuilder,
        variable_declarations_expected: bool,
        allow_local_functions: bool,
        stop_on_close_paren: bool,
        attributes: Green,
        mods: Green,
        local_function: &mut Green,
    ) {
        // TODO(port): LanguageParser.cs:5336
    }

    /// `private PostSkipAction SkipBadVariableListTokens(SeparatedSyntaxListBuilder<VariableDeclaratorSyntax> list, SyntaxKind expected)` (LanguageParser.cs:5419).
    pub(crate) fn skip_bad_variable_list_tokens(
        &mut self,
        list: &mut SyntaxListBuilder,
        expected: SyntaxKind,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:5419
        Default::default()
    }
}

/// `VariableFlags` (LanguageParser.cs:5430).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct VariableFlags(pub(crate) u32);

impl VariableFlags {
    pub(crate) const NONE: VariableFlags = VariableFlags(0);
    pub(crate) const FIXED: VariableFlags = VariableFlags(0x01);
    pub(crate) const CONST_: VariableFlags = VariableFlags(0x02);
    pub(crate) const LOCAL_OR_FIELD: VariableFlags = VariableFlags(0x04);
    pub(crate) const FOR_STATEMENT: VariableFlags = VariableFlags(0x08);
}

impl VariableFlags {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: VariableFlags) -> bool {
        self.0 & flag.0 != 0
    }

    /// `value & ~flag`.
    #[inline]
    pub(crate) fn without(self, flag: VariableFlags) -> VariableFlags {
        VariableFlags(self.0 & !flag.0)
    }
}

impl std::ops::BitOr for VariableFlags {
    type Output = VariableFlags;

    #[inline]
    fn bitor(self, other: VariableFlags) -> VariableFlags {
        VariableFlags(self.0 | other.0)
    }
}

impl std::ops::BitAnd for VariableFlags {
    type Output = VariableFlags;

    #[inline]
    fn bitand(self, other: VariableFlags) -> VariableFlags {
        VariableFlags(self.0 & other.0)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private static SyntaxTokenList GetOriginalModifiers(CSharp.CSharpSyntaxNode decl)` (LanguageParser.cs:5439).
    pub(crate) fn get_original_modifiers(decl: Green) -> Green {
        // TODO(port): LanguageParser.cs:5439
        Default::default()
    }

    /// `private static bool WasFirstVariable(CSharp.Syntax.VariableDeclaratorSyntax variable)` (LanguageParser.cs:5478).
    pub(crate) fn was_first_variable(variable: Green) -> bool {
        // TODO(port): LanguageParser.cs:5478
        Default::default()
    }

    /// `private static VariableFlags GetOriginalVariableFlags(CSharp.Syntax.VariableDeclaratorSyntax old)` (LanguageParser.cs:5488).
    pub(crate) fn get_original_variable_flags(old: Green) -> VariableFlags {
        // TODO(port): LanguageParser.cs:5488
        Default::default()
    }

    /// `private static bool CanReuseVariableDeclarator(CSharp.Syntax.VariableDeclaratorSyntax old, VariableFlags flags, bool isFirst)` (LanguageParser.cs:5511).
    pub(crate) fn can_reuse_variable_declarator(old: Green, flags: VariableFlags, is_first: bool) -> bool {
        // TODO(port): LanguageParser.cs:5511
        Default::default()
    }

    /// `private VariableDeclaratorSyntax ParseVariableDeclarator( TypeSyntax parentType, VariableFlags flags, bool isFirst, bool allowLocalFunctions, SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> mods, out LocalFunctionStatementSyntax localFunction, bool isExpressionContext = false)` (LanguageParser.cs:5527).
    pub(crate) fn parse_variable_declarator(
        &mut self,
        parent_type: Green,
        flags: VariableFlags,
        is_first: bool,
        allow_local_functions: bool,
        attributes: Green,
        mods: Green,
        local_function: &mut Green,
        is_expression_context: bool,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5527
        Default::default()
    }

    /// `private bool IsLocalFunctionAfterIdentifier()` (LanguageParser.cs:5824).
    pub(crate) fn is_local_function_after_identifier(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:5824
        Default::default()
    }

    /// `private bool IsPossibleEndOfVariableDeclaration()` (LanguageParser.cs:5843).
    pub(crate) fn is_possible_end_of_variable_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:5843
        Default::default()
    }

    /// `private ExpressionSyntax ParseVariableInitializer()` (LanguageParser.cs:5855).
    pub(crate) fn parse_variable_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:5855
        Default::default()
    }

    /// `private bool IsPossibleVariableInitializer()` (LanguageParser.cs:5862).
    pub(crate) fn is_possible_variable_initializer(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:5862
        Default::default()
    }

    /// `private FieldDeclarationSyntax ParseConstantFieldDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxKind parentKind)` (LanguageParser.cs:5867).
    pub(crate) fn parse_constant_field_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        parent_kind: SyntaxKind,
    ) -> Green {
        // TODO(port): LanguageParser.cs:5867
        Default::default()
    }

    /// `private DelegateDeclarationSyntax ParseDelegateDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:5881).
    pub(crate) fn parse_delegate_declaration(&mut self, attributes: Green, modifiers: &mut SyntaxListBuilder) -> Green {
        // TODO(port): LanguageParser.cs:5881
        Default::default()
    }

    /// `private EnumDeclarationSyntax ParseEnumDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:5914).
    pub(crate) fn parse_enum_declaration(&mut self, attributes: Green, modifiers: &mut SyntaxListBuilder) -> Green {
        // TODO(port): LanguageParser.cs:5914
        Default::default()
    }

    /// `private EnumMemberDeclarationSyntax ParseEnumMemberDeclaration()` (LanguageParser.cs:5998).
    pub(crate) fn parse_enum_member_declaration(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:5998
        Default::default()
    }

    /// `private bool IsPossibleEnumMemberDeclaration()` (LanguageParser.cs:6021).
    pub(crate) fn is_possible_enum_member_declaration(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6021
        Default::default()
    }

    /// `private bool IsDotOrColonColon()` (LanguageParser.cs:6026).
    pub(crate) fn is_dot_or_colon_colon(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6026
        Default::default()
    }

    /// `public NameSyntax ParseName()` (LanguageParser.cs:6032).
    pub(crate) fn parse_name(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:6032
        Default::default()
    }

    /// `private IdentifierNameSyntax CreateMissingIdentifierName()` (LanguageParser.cs:6037).
    pub(crate) fn create_missing_identifier_name(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:6037
        Default::default()
    }

    /// `private static SyntaxToken CreateMissingIdentifierToken()` (LanguageParser.cs:6042).
    pub(crate) fn create_missing_identifier_token() -> Green {
        // TODO(port): LanguageParser.cs:6042
        Default::default()
    }
}

/// `NameOptions` (LanguageParser.cs:6048).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct NameOptions(pub(crate) u32);

impl NameOptions {
    pub(crate) const NONE: NameOptions = NameOptions(0);
    pub(crate) const IN_EXPRESSION: NameOptions = NameOptions(1 << 0);
    pub(crate) const IN_TYPE_LIST: NameOptions = NameOptions(1 << 1);
    pub(crate) const POSSIBLE_PATTERN: NameOptions = NameOptions(1 << 2);
    pub(crate) const AFTER_IS: NameOptions = NameOptions(1 << 3);
    pub(crate) const DEFINITE_PATTERN: NameOptions = NameOptions(1 << 4);
    pub(crate) const AFTER_OUT: NameOptions = NameOptions(1 << 5);
    pub(crate) const AFTER_TUPLE_COMMA: NameOptions = NameOptions(1 << 6);
    pub(crate) const FIRST_ELEMENT_OF_POSSIBLE_TUPLE_LITERAL: NameOptions = NameOptions(1 << 7);
}

impl NameOptions {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: NameOptions) -> bool {
        self.0 & flag.0 != 0
    }

    /// `value & ~flag`.
    #[inline]
    pub(crate) fn without(self, flag: NameOptions) -> NameOptions {
        NameOptions(self.0 & !flag.0)
    }
}

impl std::ops::BitOr for NameOptions {
    type Output = NameOptions;

    #[inline]
    fn bitor(self, other: NameOptions) -> NameOptions {
        NameOptions(self.0 | other.0)
    }
}

impl std::ops::BitAnd for NameOptions {
    type Output = NameOptions;

    #[inline]
    fn bitand(self, other: NameOptions) -> NameOptions {
        NameOptions(self.0 & other.0)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsTrueIdentifier()` (LanguageParser.cs:6065).
    pub(crate) fn is_true_identifier(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6065
        Default::default()
    }

    /// `private bool IsTrueIdentifier(SyntaxToken token)` (LanguageParser.cs:6084).
    pub(crate) fn is_true_identifier_with_token(&mut self, token: Green) -> bool {
        // TODO(port): LanguageParser.cs:6084
        Default::default()
    }

    /// `private IdentifierNameSyntax ParseIdentifierName(ErrorCode code = ErrorCode.ERR_IdentifierExpected)` (LanguageParser.cs:6091).
    pub(crate) fn parse_identifier_name(&mut self, code: ErrorCode) -> Green {
        // TODO(port): LanguageParser.cs:6091
        Default::default()
    }

    /// `private SyntaxToken ParseIdentifierToken(ErrorCode code = ErrorCode.ERR_IdentifierExpected)` (LanguageParser.cs:6105).
    pub(crate) fn parse_identifier_token(&mut self, code: ErrorCode) -> Green {
        // TODO(port): LanguageParser.cs:6105
        Default::default()
    }

    /// `private bool IsCurrentTokenQueryKeywordInQuery()` (LanguageParser.cs:6138).
    pub(crate) fn is_current_token_query_keyword_in_query(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6138
        Default::default()
    }

    /// `private bool IsCurrentTokenPartialKeywordOfPartialMemberOrType()` (LanguageParser.cs:6143).
    pub(crate) fn is_current_token_partial_keyword_of_partial_member_or_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6143
        Default::default()
    }

    /// `private bool IsCurrentTokenFieldInKeywordContext()` (LanguageParser.cs:6156).
    pub(crate) fn is_current_token_field_in_keyword_context(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:6156
        Default::default()
    }
}
