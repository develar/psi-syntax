//! `LanguageParser.cs` 8248-11122: statements, blocks, local declarations and local functions (lane B). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `public StatementSyntax ParseStatement()` (LanguageParser.cs:8248).
    pub(crate) fn parse_statement(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:8248
        Default::default()
    }

    /// `private StatementSyntax ParsePossiblyAttributedStatement()` (LanguageParser.cs:8255).
    pub(crate) fn parse_possibly_attributed_statement(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:8255
        Default::default()
    }

    /// `private SyntaxList<AttributeListSyntax> ParseStatementAttributeDeclarations()` (LanguageParser.cs:8258).
    pub(crate) fn parse_statement_attribute_declarations(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:8258
        Default::default()
    }

    /// `private StatementSyntax ParseStatementCore(SyntaxList<AttributeListSyntax> attributes, bool isGlobal)` (LanguageParser.cs:8346).
    pub(crate) fn parse_statement_core(&mut self, attributes: Green, is_global: bool) -> Green {
        // TODO(port): LanguageParser.cs:8346
        Default::default()
    }

    /// `private StatementSyntax TryReuseStatement(SyntaxList<AttributeListSyntax> attributes, bool isGlobal)` (LanguageParser.cs:8428).
    pub(crate) fn try_reuse_statement(&mut self, attributes: Green, is_global: bool) -> Green {
        // TODO(port): LanguageParser.cs:8428
        Default::default()
    }

    /// `private StatementSyntax ParseStatementCoreRest(SyntaxList<AttributeListSyntax> attributes, bool isGlobal, ref ResetPoint resetPointBeforeStatement)` (LanguageParser.cs:8441).
    pub(crate) fn parse_statement_core_rest(
        &mut self,
        attributes: Green,
        is_global: bool,
        reset_point_before_statement: &mut ResetPoint,
    ) -> Green {
        // TODO(port): LanguageParser.cs:8441
        Default::default()
    }

    /// `private StatementSyntax TryParseStatementStartingWithIdentifier(SyntaxList<AttributeListSyntax> attributes, bool isGlobal)` (LanguageParser.cs:8491).
    pub(crate) fn try_parse_statement_starting_with_identifier(&mut self, attributes: Green, is_global: bool) -> Green {
        // TODO(port): LanguageParser.cs:8491
        Default::default()
    }

    /// `private StatementSyntax ParseStatementStartingWithUsing(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:8526).
    pub(crate) fn parse_statement_starting_with_using(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:8526
        Default::default()
    }

    /// `private StatementSyntax TryParseStatementStartingWithUnsafe(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:8530).
    pub(crate) fn try_parse_statement_starting_with_unsafe(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:8530
        Default::default()
    }

    /// `private StatementSyntax ParseExpressionStatementOrLocalFunctionStartingWithUnsafe(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:8540).
    pub(crate) fn parse_expression_statement_or_local_function_starting_with_unsafe(
        &mut self,
        attributes: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:8540
        Default::default()
    }

    /// `private bool IsPossibleAwaitUsing()` (LanguageParser.cs:8554).
    pub(crate) fn is_possible_await_using(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8554
        Default::default()
    }

    /// `private bool IsPossibleLabeledStatement()` (LanguageParser.cs:8557).
    pub(crate) fn is_possible_labeled_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8557
        Default::default()
    }

    /// `private bool IsPossibleYieldStatement()` (LanguageParser.cs:8562).
    pub(crate) fn is_possible_yield_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8562
        Default::default()
    }

    /// `private bool IsPossibleLocalDeclarationStatement(bool isGlobalScriptLevel)` (LanguageParser.cs:8568).
    pub(crate) fn is_possible_local_declaration_statement(&mut self, is_global_script_level: bool) -> bool {
        // TODO(port): LanguageParser.cs:8568
        Default::default()
    }

    /// `private bool IsPossibleFirstTypedIdentifierInLocalDeclarationStatement(bool isGlobalScriptLevel)` (LanguageParser.cs:8615).
    pub(crate) fn is_possible_first_typed_identifier_in_local_declaration_statement(
        &mut self,
        is_global_script_level: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:8615
        Default::default()
    }

    /// `private bool IsPossibleTopLevelUsingLocalDeclarationStatement()` (LanguageParser.cs:8719).
    pub(crate) fn is_possible_top_level_using_local_declaration_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8719
        Default::default()
    }

    /// `private bool IsPossibleDeclarationStatementFollowingNullableType(bool isGlobalScriptLevel)` (LanguageParser.cs:8765).
    pub(crate) fn is_possible_declaration_statement_following_nullable_type(
        &mut self,
        is_global_script_level: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:8765
        Default::default()
    }

    /// `private bool IsPossibleFieldDeclarationFollowingNullableType()` (LanguageParser.cs:8806).
    pub(crate) fn is_possible_field_declaration_following_nullable_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8806
        Default::default()
    }

    /// `private bool IsPossibleMethodDeclarationFollowingNullableType()` (LanguageParser.cs:8827).
    pub(crate) fn is_possible_method_declaration_following_nullable_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8827
        Default::default()
    }

    /// `private bool IsAnonymousDelegateExpression()` (LanguageParser.cs:8939).
    pub(crate) fn is_anonymous_delegate_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8939
        Default::default()
    }

    /// `private bool IsPossibleNewExpression()` (LanguageParser.cs:8987).
    pub(crate) fn is_possible_new_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:8987
        Default::default()
    }

    /// `private bool? IsPossibleTypedIdentifierStart(SyntaxToken current, SyntaxToken next, bool allowThisKeyword)` (LanguageParser.cs:9073).
    pub(crate) fn is_possible_typed_identifier_start(
        &mut self,
        current: Green,
        next: Green,
        allow_this_keyword: bool,
    ) -> Option<bool> {
        // TODO(port): LanguageParser.cs:9073
        Default::default()
    }

    /// `private BlockSyntax ParsePossiblyAttributedBlock()` (LanguageParser.cs:9114).
    pub(crate) fn parse_possibly_attributed_block(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:9114
        Default::default()
    }

    /// `private BlockSyntax ParseMethodOrAccessorBodyBlock(SyntaxList<AttributeListSyntax> attributes, bool isAccessorBody)` (LanguageParser.cs:9122).
    pub(crate) fn parse_method_or_accessor_body_block(&mut self, attributes: Green, is_accessor_body: bool) -> Green {
        // TODO(port): LanguageParser.cs:9122
        Default::default()
    }

    /// `private BlockSyntax ParseBlock(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9162).
    pub(crate) fn parse_block(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9162
        Default::default()
    }

    /// `private static bool IsLargeEnoughNonEmptyStatementList(SyntaxListBuilder<StatementSyntax> statements)` (LanguageParser.cs:9184).
    pub(crate) fn is_large_enough_non_empty_statement_list(statements: &mut SyntaxListBuilder) -> bool {
        // TODO(port): LanguageParser.cs:9184
        Default::default()
    }

    /// `private void ParseStatements(ref CSharpSyntaxNode previousNode, SyntaxListBuilder<StatementSyntax> statements, bool stopOnSwitchSections)` (LanguageParser.cs:9205).
    pub(crate) fn parse_statements(
        &mut self,
        previous_node: &mut Green,
        statements: &mut SyntaxListBuilder,
        stop_on_switch_sections: bool,
    ) {
        // TODO(port): LanguageParser.cs:9205
    }

    /// `private bool IsPossibleStatementStartOrStop()` (LanguageParser.cs:9245).
    pub(crate) fn is_possible_statement_start_or_stop(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9245
        Default::default()
    }

    /// `private PostSkipAction SkipBadStatementListTokens(SyntaxListBuilder<StatementSyntax> statements, SyntaxKind expected, out GreenNode trailingTrivia)` (LanguageParser.cs:9251).
    pub(crate) fn skip_bad_statement_list_tokens(
        &mut self,
        statements: &mut SyntaxListBuilder,
        expected: SyntaxKind,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:9251
        Default::default()
    }

    /// `private bool IsDefiniteStatement()` (LanguageParser.cs:9265).
    pub(crate) fn is_definite_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9265
        Default::default()
    }

    /// `private bool IsPossibleStatement()` (LanguageParser.cs:9297).
    pub(crate) fn is_possible_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9297
        Default::default()
    }

    /// `private FixedStatementSyntax ParseFixedStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9328).
    pub(crate) fn parse_fixed_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9328
        Default::default()
    }

    /// `private bool IsEndOfFixedStatement()` (LanguageParser.cs:9347).
    pub(crate) fn is_end_of_fixed_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9347
        Default::default()
    }

    /// `private StatementSyntax ParseEmbeddedStatement()` (LanguageParser.cs:9352).
    pub(crate) fn parse_embedded_statement(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:9352
        Default::default()
    }

    /// `private BreakStatementSyntax ParseBreakStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9394).
    pub(crate) fn parse_break_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9394
        Default::default()
    }

    /// `private ContinueStatementSyntax ParseContinueStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9403).
    pub(crate) fn parse_continue_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9403
        Default::default()
    }

    /// `private TryStatementSyntax ParseTryStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9412).
    pub(crate) fn parse_try_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9412
        Default::default()
    }

    /// `private bool IsEndOfTryBlock()` (LanguageParser.cs:9483).
    pub(crate) fn is_end_of_try_block(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9483
        Default::default()
    }

    /// `private CatchClauseSyntax ParseCatchClause()` (LanguageParser.cs:9488).
    pub(crate) fn parse_catch_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:9488
        Default::default()
    }

    /// `private bool IsEndOfCatchClause()` (LanguageParser.cs:9547).
    pub(crate) fn is_end_of_catch_clause(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9547
        Default::default()
    }

    /// `private bool IsEndOfFilterClause()` (LanguageParser.cs:9556).
    pub(crate) fn is_end_of_filter_clause(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9556
        Default::default()
    }

    /// `private bool IsEndOfCatchBlock()` (LanguageParser.cs:9564).
    pub(crate) fn is_end_of_catch_block(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9564
        Default::default()
    }

    /// `private StatementSyntax ParseCheckedStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9571).
    pub(crate) fn parse_checked_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9571
        Default::default()
    }

    /// `private DoStatementSyntax ParseDoStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9588).
    pub(crate) fn parse_do_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9588
        Default::default()
    }

    /// `private bool IsEndOfDoWhileExpression()` (LanguageParser.cs:9612).
    pub(crate) fn is_end_of_do_while_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9612
        Default::default()
    }

    /// `private StatementSyntax ParseForOrForEachStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9617).
    pub(crate) fn parse_for_or_for_each_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9617
        Default::default()
    }

    /// `private ForStatementSyntax ParseForStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9650).
    pub(crate) fn parse_for_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:9650
        Default::default()
    }

    /// `private bool IsEndOfForStatementArgument()` (LanguageParser.cs:9787).
    pub(crate) fn is_end_of_for_statement_argument(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9787
        Default::default()
    }

    /// `private CommonForEachStatementSyntax ParseForEachStatement( SyntaxList<AttributeListSyntax> attributes, SyntaxToken awaitTokenOpt)` (LanguageParser.cs:9792).
    pub(crate) fn parse_for_each_statement(&mut self, attributes: Green, await_token_opt: Green) -> Green {
        // TODO(port): LanguageParser.cs:9792
        Default::default()
    }

    /// `private ExpressionSyntax ParseExpressionOrDeclaration(ParseTypeMode mode, bool permitTupleDesignation)` (LanguageParser.cs:9889).
    pub(crate) fn parse_expression_or_declaration(
        &mut self,
        mode: ParseTypeMode,
        permit_tuple_designation: bool,
    ) -> Green {
        // TODO(port): LanguageParser.cs:9889
        Default::default()
    }

    /// `private bool IsPossibleDeclarationExpression(ParseTypeMode mode, bool permitTupleDesignation, out bool isScoped)` (LanguageParser.cs:9896).
    pub(crate) fn is_possible_declaration_expression(
        &mut self,
        mode: ParseTypeMode,
        permit_tuple_designation: bool,
        is_scoped: &mut bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:9896
        Default::default()
    }

    /// `private bool IsVarType()` (LanguageParser.cs:9972).
    pub(crate) fn is_var_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:9972
        Default::default()
    }

    /// `private static bool IsValidForeachVariable(ExpressionSyntax variable)` (LanguageParser.cs:9993).
    pub(crate) fn is_valid_foreach_variable(variable: Green) -> bool {
        // TODO(port): LanguageParser.cs:9993
        Default::default()
    }

    /// `private GotoStatementSyntax ParseGotoStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10011).
    pub(crate) fn parse_goto_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10011
        Default::default()
    }

    /// `private ExpressionSyntax ParseExpressionForParenthesizedConstruct()` (LanguageParser.cs:10044).
    pub(crate) fn parse_expression_for_parenthesized_construct(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:10044
        Default::default()
    }

    /// `private ExpressionSyntax ParseErrantExpressionWhenNoCloseParenToken(ExpressionSyntax expression)` (LanguageParser.cs:10047).
    pub(crate) fn parse_errant_expression_when_no_close_paren_token(&mut self, expression: Green) -> Green {
        // TODO(port): LanguageParser.cs:10047
        Default::default()
    }

    /// `private IfStatementSyntax ParseIfStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10077).
    pub(crate) fn parse_if_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10077
        Default::default()
    }

    /// `private IfStatementSyntax ParseMisplacedElse(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10142).
    pub(crate) fn parse_misplaced_else(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10142
        Default::default()
    }

    /// `private ElseClauseSyntax ParseElseClauseOpt()` (LanguageParser.cs:10156).
    pub(crate) fn parse_else_clause_opt(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:10156
        Default::default()
    }

    /// `private LockStatementSyntax ParseLockStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10165).
    pub(crate) fn parse_lock_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10165
        Default::default()
    }

    /// `private ReturnStatementSyntax ParseReturnStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10177).
    pub(crate) fn parse_return_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10177
        Default::default()
    }

    /// `private YieldStatementSyntax ParseYieldStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10187).
    pub(crate) fn parse_yield_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10187
        Default::default()
    }

    /// `private SwitchStatementSyntax ParseSwitchStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10224).
    pub(crate) fn parse_switch_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10224
        Default::default()
    }

    /// `private bool IsPossibleSwitchSection()` (LanguageParser.cs:10294).
    pub(crate) fn is_possible_switch_section(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:10294
        Default::default()
    }

    /// `private SwitchSectionSyntax ParseSwitchSection()` (LanguageParser.cs:10300).
    pub(crate) fn parse_switch_section(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:10300
        Default::default()
    }

    /// `private ThrowStatementSyntax ParseThrowStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10372).
    pub(crate) fn parse_throw_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10372
        Default::default()
    }

    /// `private UnsafeStatementSyntax ParseUnsafeStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10382).
    pub(crate) fn parse_unsafe_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10382
        Default::default()
    }

    /// `private UsingStatementSyntax ParseUsingStatement(SyntaxList<AttributeListSyntax> attributes, SyntaxToken awaitTokenOpt = null)` (LanguageParser.cs:10391).
    pub(crate) fn parse_using_statement(&mut self, attributes: Green, await_token_opt: Green) -> Green {
        // TODO(port): LanguageParser.cs:10391
        Default::default()
    }

    /// `private void ParseUsingExpression(ref VariableDeclarationSyntax declaration, ref ExpressionSyntax expression, ref ResetPoint resetPoint)` (LanguageParser.cs:10414).
    pub(crate) fn parse_using_expression(
        &mut self,
        declaration: &mut Green,
        expression: &mut Green,
        reset_point: &mut ResetPoint,
    ) {
        // TODO(port): LanguageParser.cs:10414
    }

    /// `private bool IsUsingStatementVariableDeclaration(ScanTypeFlags st)` (LanguageParser.cs:10507).
    pub(crate) fn is_using_statement_variable_declaration(&mut self, st: ScanTypeFlags) -> bool {
        // TODO(port): LanguageParser.cs:10507
        Default::default()
    }

    /// `private WhileStatementSyntax ParseWhileStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10518).
    pub(crate) fn parse_while_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10518
        Default::default()
    }

    /// `private LabeledStatementSyntax ParseLabeledStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10530).
    pub(crate) fn parse_labeled_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10530
        Default::default()
    }

    /// `private StatementSyntax ParseLocalDeclarationStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10546).
    pub(crate) fn parse_local_declaration_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:10546
        Default::default()
    }

    /// `private bool IsDefiniteScopedModifier( bool isFunctionPointerParameter, bool isLambdaParameter)` (LanguageParser.cs:10647).
    pub(crate) fn is_definite_scoped_modifier(
        &mut self,
        is_function_pointer_parameter: bool,
        is_lambda_parameter: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:10647
        Default::default()
    }

    /// `private SyntaxToken ParsePossibleScopedKeyword( bool isFunctionPointerParameter, bool isLambdaParameter)` (LanguageParser.cs:10696).
    pub(crate) fn parse_possible_scoped_keyword(
        &mut self,
        is_function_pointer_parameter: bool,
        is_lambda_parameter: bool,
    ) -> Green {
        // TODO(port): LanguageParser.cs:10696
        Default::default()
    }

    /// `private VariableDesignationSyntax ParseDesignation(bool forPattern)` (LanguageParser.cs:10705).
    pub(crate) fn parse_designation(&mut self, for_pattern: bool) -> Green {
        // TODO(port): LanguageParser.cs:10705
        Default::default()
    }

    /// `private VariableDesignationSyntax ParseSimpleDesignation()` (LanguageParser.cs:10761).
    pub(crate) fn parse_simple_designation(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:10761
        Default::default()
    }

    /// `private WhenClauseSyntax ParseWhenClause(Precedence precedence)` (LanguageParser.cs:10768).
    pub(crate) fn parse_when_clause(&mut self, precedence: Precedence) -> Green {
        // TODO(port): LanguageParser.cs:10768
        Default::default()
    }

    /// `private VariableDeclarationSyntax ParseParenthesizedVariableDeclaration( VariableFlags initialFlags, SyntaxToken? scopedKeyword)` (LanguageParser.cs:10786).
    pub(crate) fn parse_parenthesized_variable_declaration(
        &mut self,
        initial_flags: VariableFlags,
        scoped_keyword: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:10786
        Default::default()
    }

    /// `private void ParseLocalDeclaration( SeparatedSyntaxListBuilder<VariableDeclaratorSyntax> variables, bool allowLocalFunctions, bool stopOnCloseParen, SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> mods, SyntaxToken? scopedKeyword, VariableFlags initialFlags, out TypeSyntax type, out LocalFunctionStatementSyntax localFunction)` (LanguageParser.cs:10808).
    pub(crate) fn parse_local_declaration(
        &mut self,
        variables: &mut SyntaxListBuilder,
        allow_local_functions: bool,
        stop_on_close_paren: bool,
        attributes: Green,
        mods: Green,
        scoped_keyword: Green,
        initial_flags: VariableFlags,
        type_: &mut Green,
        local_function: &mut Green,
    ) {
        // TODO(port): LanguageParser.cs:10808
    }

    /// `private bool IsEndOfDeclarationClause()` (LanguageParser.cs:10852).
    pub(crate) fn is_end_of_declaration_clause(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:10852
        Default::default()
    }

    /// `private void ParseLocalDeclarationStatementModifiers(SyntaxListBuilder list, bool isUsingDeclaration)` (LanguageParser.cs:10864).
    pub(crate) fn parse_local_declaration_statement_modifiers(
        &mut self,
        list: &mut SyntaxListBuilder,
        is_using_declaration: bool,
    ) {
        // TODO(port): LanguageParser.cs:10864
    }

    /// `private static bool IsDeclarationModifier(SyntaxKind kind)` (LanguageParser.cs:10932).
    pub(crate) fn is_declaration_modifier(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:10932
        Default::default()
    }

    /// `private static bool IsAdditionalLocalFunctionModifier(SyntaxKind kind)` (LanguageParser.cs:10946).
    pub(crate) fn is_additional_local_function_modifier(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:10946
        Default::default()
    }

    /// `private static bool IsAccessibilityModifier(SyntaxKind kind)` (LanguageParser.cs:10968).
    pub(crate) fn is_accessibility_modifier(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:10968
        Default::default()
    }

    /// `private LocalFunctionStatementSyntax TryParseLocalFunctionStatementBody( SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> modifiers, TypeSyntax type, SyntaxToken identifier)` (LanguageParser.cs:10985).
    pub(crate) fn try_parse_local_function_statement_body(
        &mut self,
        attributes: Green,
        modifiers: Green,
        type_: Green,
        identifier: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:10985
        Default::default()
    }

    /// `private ExpressionStatementSyntax ParseExpressionStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:11101).
    pub(crate) fn parse_expression_statement(&mut self, attributes: Green) -> Green {
        // TODO(port): LanguageParser.cs:11101
        Default::default()
    }

    /// `private ExpressionStatementSyntax ParseExpressionStatement(SyntaxList<AttributeListSyntax> attributes, ExpressionSyntax expression)` (LanguageParser.cs:11106).
    pub(crate) fn parse_expression_statement_with_expression(&mut self, attributes: Green, expression: Green) -> Green {
        // TODO(port): LanguageParser.cs:11106
        Default::default()
    }
}
