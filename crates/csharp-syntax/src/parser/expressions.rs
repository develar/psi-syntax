//! `LanguageParser.cs` 11123-14419: expressions and their precedence, primary expressions, arguments, casts,
//! lambdas, object and collection creation, initializers, query expressions (lane C). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `public ExpressionSyntax ParseExpression()` (LanguageParser.cs:11123).
    pub(crate) fn parse_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:11123
        Default::default()
    }

    /// `private ExpressionSyntax ParseExpressionCore()` (LanguageParser.cs:11130).
    pub(crate) fn parse_expression_core(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:11130
        Default::default()
    }

    /// `private bool CanStartExpression()` (LanguageParser.cs:11138).
    pub(crate) fn can_start_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:11138
        Default::default()
    }

    /// `private bool IsPossibleExpression()` (LanguageParser.cs:11146).
    pub(crate) fn is_possible_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:11146
        Default::default()
    }

    /// `private bool IsPossibleExpression(bool allowBinaryExpressions, bool allowAssignmentExpressions)` (LanguageParser.cs:11151).
    pub(crate) fn is_possible_expression_with_allow_assignment_expressions(
        &mut self,
        allow_binary_expressions: bool,
        allow_assignment_expressions: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:11151
        Default::default()
    }

    /// `private static bool IsInvalidSubExpression(SyntaxKind kind)` (LanguageParser.cs:11209).
    pub(crate) fn is_invalid_sub_expression(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:11209
        Default::default()
    }

    /// `internal static bool IsRightAssociative(SyntaxKind op)` (LanguageParser.cs:11237).
    pub(crate) fn is_right_associative(op: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:11237
        Default::default()
    }
}

/// `Precedence` (LanguageParser.cs:11261).
/// (`enum Precedence : uint`: `Assignment` and `Lambda` alias `Expression`, so it is a number with named values.)
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) struct Precedence(pub(crate) u32);

impl Precedence {
    pub(crate) const EXPRESSION: Precedence = Precedence(0);
    pub(crate) const ASSIGNMENT: Precedence = Precedence(0);
    pub(crate) const LAMBDA: Precedence = Precedence(0);
    pub(crate) const CONDITIONAL: Precedence = Precedence(1);
    pub(crate) const COALESCING: Precedence = Precedence(2);
    pub(crate) const CONDITIONAL_OR: Precedence = Precedence(3);
    pub(crate) const CONDITIONAL_AND: Precedence = Precedence(4);
    pub(crate) const LOGICAL_OR: Precedence = Precedence(5);
    pub(crate) const LOGICAL_XOR: Precedence = Precedence(6);
    pub(crate) const LOGICAL_AND: Precedence = Precedence(7);
    pub(crate) const EQUALITY: Precedence = Precedence(8);
    pub(crate) const RELATIONAL: Precedence = Precedence(9);
    pub(crate) const SHIFT: Precedence = Precedence(10);
    pub(crate) const ADDITIVE: Precedence = Precedence(11);
    pub(crate) const MULTIPLICATIVE: Precedence = Precedence(12);
    pub(crate) const SWITCH: Precedence = Precedence(13);
    pub(crate) const RANGE: Precedence = Precedence(14);
    pub(crate) const UNARY: Precedence = Precedence(15);
    pub(crate) const CAST: Precedence = Precedence(16);
    pub(crate) const POINTER_INDIRECTION: Precedence = Precedence(17);
    pub(crate) const ADDRESS_OF: Precedence = Precedence(18);
    pub(crate) const PRIMARY: Precedence = Precedence(19);
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private static Precedence GetPrecedence(SyntaxKind op)` (LanguageParser.cs:11287).
    pub(crate) fn get_precedence(op: SyntaxKind) -> Precedence {
        // TODO(port): LanguageParser.cs:11287
        Default::default()
    }

    /// `private static bool IsExpectedPrefixUnaryOperator(SyntaxKind kind)` (LanguageParser.cs:11420).
    pub(crate) fn is_expected_prefix_unary_operator(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:11420
        Default::default()
    }

    /// `private static bool IsExpectedBinaryOperator(SyntaxKind kind)` (LanguageParser.cs:11425).
    pub(crate) fn is_expected_binary_operator(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:11425
        Default::default()
    }

    /// `private static bool IsExpectedAssignmentOperator(SyntaxKind kind)` (LanguageParser.cs:11430).
    pub(crate) fn is_expected_assignment_operator(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:11430
        Default::default()
    }

    /// `private bool IsPossibleAwaitExpressionStatement()` (LanguageParser.cs:11435).
    pub(crate) fn is_possible_await_expression_statement(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:11435
        Default::default()
    }

    /// `private bool IsAwaitExpression()` (LanguageParser.cs:11440).
    pub(crate) fn is_await_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:11440
        Default::default()
    }

    /// `private ExpressionSyntax ParseSubExpression(Precedence precedence)` (LanguageParser.cs:11496).
    pub(crate) fn parse_sub_expression(&mut self, precedence: Precedence) -> Green {
        // TODO(port): LanguageParser.cs:11496
        Default::default()
    }

    /// `private ExpressionSyntax ParseSubExpressionCore(Precedence precedence)` (LanguageParser.cs:11511).
    pub(crate) fn parse_sub_expression_core(&mut self, precedence: Precedence) -> Green {
        // TODO(port): LanguageParser.cs:11511
        Default::default()
    }

    /// `private ExpressionSyntax ParseExpressionContinued(ExpressionSyntax unaryOrPrimaryExpression, Precedence precedence)` (LanguageParser.cs:11596).
    pub(crate) fn parse_expression_continued(
        &mut self,
        unary_or_primary_expression: Green,
        precedence: Precedence,
    ) -> Green {
        // TODO(port): LanguageParser.cs:11596
        Default::default()
    }

    /// `private TUPLE GetExpressionOperatorTokenKindAndExpressionKind()` (LanguageParser.cs:11797).
    pub(crate) fn get_expression_operator_token_kind_and_expression_kind(&mut self) -> (SyntaxKind, SyntaxKind) {
        // TODO(port): LanguageParser.cs:11797
        Default::default()
    }

    /// `private SyntaxToken EatExpressionOperatorToken(SyntaxKind operatorTokenKind)` (LanguageParser.cs:11857).
    pub(crate) fn eat_expression_operator_token(&mut self, operator_token_kind: SyntaxKind) -> Green {
        // TODO(port): LanguageParser.cs:11857
        Default::default()
    }

    /// `private AssignmentExpressionSyntax ParseAssignmentExpression(SyntaxKind operatorExpressionKind, ExpressionSyntax leftOperand, SyntaxToken operatorToken)` (LanguageParser.cs:11898).
    pub(crate) fn parse_assignment_expression(
        &mut self,
        operator_expression_kind: SyntaxKind,
        left_operand: Green,
        operator_token: Green,
    ) -> Green {
        // TODO(port): LanguageParser.cs:11898
        Default::default()
    }

    /// `public bool IsAtDotDotToken()` (LanguageParser.cs:11923).
    pub(crate) fn is_at_dot_dot_token(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:11923
        Default::default()
    }

    /// `public static bool IsAtDotDotToken(SyntaxToken token1, SyntaxToken token2)` (LanguageParser.cs:11932).
    pub(crate) fn is_at_dot_dot_token_with_token2(token1: Green, token2: Green) -> bool {
        // TODO(port): LanguageParser.cs:11932
        Default::default()
    }

    /// `public SyntaxToken EatDotDotToken()` (LanguageParser.cs:11940).
    pub(crate) fn eat_dot_dot_token(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:11940
        Default::default()
    }

    /// `private DeclarationExpressionSyntax ParseDeclarationExpression(ParseTypeMode mode, bool isScoped)` (LanguageParser.cs:11976).
    pub(crate) fn parse_declaration_expression(&mut self, mode: ParseTypeMode, is_scoped: bool) -> Green {
        // TODO(port): LanguageParser.cs:11976
        Default::default()
    }

    /// `private ExpressionSyntax ParseThrowExpression()` (LanguageParser.cs:11988).
    pub(crate) fn parse_throw_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:11988
        Default::default()
    }

    /// `private ExpressionSyntax ParseIsExpression(ExpressionSyntax leftOperand, SyntaxToken opToken)` (LanguageParser.cs:11995).
    pub(crate) fn parse_is_expression(&mut self, left_operand: Green, op_token: Green) -> Green {
        // TODO(port): LanguageParser.cs:11995
        Default::default()
    }

    /// `private ExpressionSyntax ParsePrimaryExpression(Precedence precedence)` (LanguageParser.cs:12006).
    pub(crate) fn parse_primary_expression(&mut self, precedence: Precedence) -> Green {
        // TODO(port): LanguageParser.cs:12006
        Default::default()
    }

    /// `private ExpressionSyntax ParseBaseExpression()` (LanguageParser.cs:12281).
    pub(crate) fn parse_base_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12281
        Default::default()
    }

    /// `private bool IsPossibleDeconstructionLeft(Precedence precedence)` (LanguageParser.cs:12295).
    pub(crate) fn is_possible_deconstruction_left(&mut self, precedence: Precedence) -> bool {
        // TODO(port): LanguageParser.cs:12295
        Default::default()
    }

    /// `private bool ScanDesignator()` (LanguageParser.cs:12310).
    pub(crate) fn scan_designator(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:12310
        Default::default()
    }

    /// `private bool IsPossibleAnonymousMethodExpression()` (LanguageParser.cs:12347).
    pub(crate) fn is_possible_anonymous_method_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:12347
        Default::default()
    }

    /// `private bool TryParseConditionalAccessExpression( ExpressionSyntax primaryExpression, [NotNullWhen(true)] out ConditionalAccessExpressionSyntax? conditionalAccessExpression)` (LanguageParser.cs:12368).
    pub(crate) fn try_parse_conditional_access_expression(
        &mut self,
        primary_expression: Green,
        conditional_access_expression: &mut Green,
    ) -> bool {
        // TODO(port): LanguageParser.cs:12368
        Default::default()
    }

    /// `internal ArgumentListSyntax ParseParenthesizedArgumentList()` (LanguageParser.cs:12507).
    pub(crate) fn parse_parenthesized_argument_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12507
        Default::default()
    }

    /// `internal BracketedArgumentListSyntax ParseBracketedArgumentList()` (LanguageParser.cs:12523).
    pub(crate) fn parse_bracketed_argument_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12523
        Default::default()
    }

    /// `private void ParseArgumentList( out SyntaxToken openToken, out SeparatedSyntaxList<ArgumentSyntax> arguments, out SyntaxToken closeToken, SyntaxKind openKind, SyntaxKind closeKind)` (LanguageParser.cs:12539).
    pub(crate) fn parse_argument_list(
        &mut self,
        open_token: &mut Green,
        arguments: &mut Green,
        close_token: &mut Green,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) {
        // TODO(port): LanguageParser.cs:12539
    }

    /// `private bool IsEndOfArgumentList()` (LanguageParser.cs:12622).
    pub(crate) fn is_end_of_argument_list(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:12622
        Default::default()
    }

    /// `private bool IsPossibleArgumentExpression()` (LanguageParser.cs:12627).
    pub(crate) fn is_possible_argument_expression(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:12627
        Default::default()
    }

    /// `private static bool IsValidArgumentRefKindKeyword(SyntaxKind kind)` (LanguageParser.cs:12632).
    pub(crate) fn is_valid_argument_ref_kind_keyword(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:12632
        Default::default()
    }

    /// `private ArgumentSyntax ParseArgumentExpression(bool isIndexer)` (LanguageParser.cs:12645).
    pub(crate) fn parse_argument_expression(&mut self, is_indexer: bool) -> Green {
        // TODO(port): LanguageParser.cs:12645
        Default::default()
    }

    /// `private TypeOfExpressionSyntax ParseTypeOfExpression()` (LanguageParser.cs:12688).
    pub(crate) fn parse_type_of_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12688
        Default::default()
    }

    /// `private ExpressionSyntax ParseDefaultExpression()` (LanguageParser.cs:12697).
    pub(crate) fn parse_default_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12697
        Default::default()
    }

    /// `private SizeOfExpressionSyntax ParseSizeOfExpression()` (LanguageParser.cs:12714).
    pub(crate) fn parse_size_of_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12714
        Default::default()
    }

    /// `private MakeRefExpressionSyntax ParseMakeRefExpression()` (LanguageParser.cs:12723).
    pub(crate) fn parse_make_ref_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12723
        Default::default()
    }

    /// `private RefTypeExpressionSyntax ParseRefTypeExpression()` (LanguageParser.cs:12732).
    pub(crate) fn parse_ref_type_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12732
        Default::default()
    }

    /// `private CheckedExpressionSyntax ParseCheckedOrUncheckedExpression()` (LanguageParser.cs:12741).
    pub(crate) fn parse_checked_or_unchecked_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12741
        Default::default()
    }

    /// `private UnsafeExpressionSyntax ParseUnsafeExpression()` (LanguageParser.cs:12755).
    pub(crate) fn parse_unsafe_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12755
        Default::default()
    }

    /// `private RefValueExpressionSyntax ParseRefValueExpression()` (LanguageParser.cs:12764).
    pub(crate) fn parse_ref_value_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12764
        Default::default()
    }

    /// `private bool ScanParenthesizedLambda(Precedence precedence)` (LanguageParser.cs:12775).
    pub(crate) fn scan_parenthesized_lambda(&mut self, precedence: Precedence) -> bool {
        // TODO(port): LanguageParser.cs:12775
        Default::default()
    }

    /// `private bool ScanImplicitlyTypedLambdaOrSimpleExplicitlyTypedParenthesizedLambda(Precedence precedence)` (LanguageParser.cs:12785).
    pub(crate) fn scan_implicitly_typed_lambda_or_simple_explicitly_typed_parenthesized_lambda(
        &mut self,
        precedence: Precedence,
    ) -> bool {
        // TODO(port): LanguageParser.cs:12785
        Default::default()
    }

    /// `private bool ScanExplicitlyTypedLambda(Precedence precedence)` (LanguageParser.cs:12816).
    pub(crate) fn scan_explicitly_typed_lambda(&mut self, precedence: Precedence) -> bool {
        // TODO(port): LanguageParser.cs:12816
        Default::default()
    }

    /// `private ExpressionSyntax ParseCastOrParenExpressionOrTuple()` (LanguageParser.cs:12892).
    pub(crate) fn parse_cast_or_paren_expression_or_tuple(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:12892
        Default::default()
    }

    /// `private TupleExpressionSyntax ParseTupleExpressionTail(SyntaxToken openParen, ArgumentSyntax firstArg)` (LanguageParser.cs:12946).
    pub(crate) fn parse_tuple_expression_tail(&mut self, open_paren: Green, first_arg: Green) -> Green {
        // TODO(port): LanguageParser.cs:12946
        Default::default()
    }

    /// `private bool ScanCast(bool forPattern, bool inSwitchArmPattern)` (LanguageParser.cs:12980).
    pub(crate) fn scan_cast(&mut self, for_pattern: bool, in_switch_arm_pattern: bool) -> bool {
        // TODO(port): LanguageParser.cs:12980
        Default::default()
    }

    /// `private bool IsPossibleLambdaExpression(Precedence precedence)` (LanguageParser.cs:13118).
    pub(crate) fn is_possible_lambda_expression(&mut self, precedence: Precedence) -> bool {
        // TODO(port): LanguageParser.cs:13118
        Default::default()
    }

    /// `private static bool CanFollowCast(SyntaxKind kind)` (LanguageParser.cs:13245).
    pub(crate) fn can_follow_cast(kind: SyntaxKind) -> bool {
        // TODO(port): LanguageParser.cs:13245
        Default::default()
    }

    /// `private ExpressionSyntax ParseNewExpression()` (LanguageParser.cs:13306).
    pub(crate) fn parse_new_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13306
        Default::default()
    }

    /// `private CollectionExpressionSyntax ParseCollectionExpression()` (LanguageParser.cs:13325).
    pub(crate) fn parse_collection_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13325
        Default::default()
    }

    /// `private bool IsPossibleCollectionElement()` (LanguageParser.cs:13354).
    pub(crate) fn is_possible_collection_element(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13354
        Default::default()
    }

    /// `private CollectionElementSyntax ParseCollectionElement()` (LanguageParser.cs:13359).
    pub(crate) fn parse_collection_element(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13359
        Default::default()
    }

    /// `private bool IsAnonymousType()` (LanguageParser.cs:13379).
    pub(crate) fn is_anonymous_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13379
        Default::default()
    }

    /// `private AnonymousObjectCreationExpressionSyntax ParseAnonymousTypeExpression()` (LanguageParser.cs:13384).
    pub(crate) fn parse_anonymous_type_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13384
        Default::default()
    }

    /// `private AnonymousObjectMemberDeclaratorSyntax ParseAnonymousTypeMemberInitializer()` (LanguageParser.cs:13409).
    pub(crate) fn parse_anonymous_type_member_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13409
        Default::default()
    }

    /// `private bool IsInitializerMember()` (LanguageParser.cs:13416).
    pub(crate) fn is_initializer_member(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13416
        Default::default()
    }

    /// `private bool IsComplexElementInitializer()` (LanguageParser.cs:13424).
    pub(crate) fn is_complex_element_initializer(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13424
        Default::default()
    }

    /// `private bool IsNamedAssignment()` (LanguageParser.cs:13429).
    pub(crate) fn is_named_assignment(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13429
        Default::default()
    }

    /// `private bool IsNamedMemberInitializer()` (LanguageParser.cs:13434).
    pub(crate) fn is_named_member_initializer(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13434
        Default::default()
    }

    /// `private bool IsDictionaryInitializer()` (LanguageParser.cs:13439).
    pub(crate) fn is_dictionary_initializer(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13439
        Default::default()
    }

    /// `private ExpressionSyntax ParseArrayOrObjectCreationExpression()` (LanguageParser.cs:13444).
    pub(crate) fn parse_array_or_object_creation_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13444
        Default::default()
    }

    /// `private bool IsImplicitObjectCreation()` (LanguageParser.cs:13491).
    pub(crate) fn is_implicit_object_creation(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13491
        Default::default()
    }

    /// `private WithExpressionSyntax ParseWithExpression(ExpressionSyntax receiverExpression, SyntaxToken withKeyword)` (LanguageParser.cs:13519).
    pub(crate) fn parse_with_expression(&mut self, receiver_expression: Green, with_keyword: Green) -> Green {
        // TODO(port): LanguageParser.cs:13519
        Default::default()
    }

    /// `private InitializerExpressionSyntax ParseObjectOrCollectionInitializer()` (LanguageParser.cs:13544).
    pub(crate) fn parse_object_or_collection_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13544
        Default::default()
    }

    /// `private ExpressionSyntax ParseObjectOrCollectionInitializerMember()` (LanguageParser.cs:13590).
    pub(crate) fn parse_object_or_collection_initializer_member(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13590
        Default::default()
    }

    /// `private static PostSkipAction SkipBadInitializerListTokens<T>( LanguageParser @this, ref SyntaxToken startToken, SeparatedSyntaxListBuilder<T> list, SyntaxKind expectedKind, SyntaxKind closeKind) where T : CSharpSyntaxNode` (LanguageParser.cs:13619).
    pub(crate) fn skip_bad_initializer_list_tokens(
        &mut self,
        start_token: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        // TODO(port): LanguageParser.cs:13619
        Default::default()
    }

    /// `private AssignmentExpressionSyntax ParseObjectInitializerNamedAssignment()` (LanguageParser.cs:13629).
    pub(crate) fn parse_object_initializer_named_assignment(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13629
        Default::default()
    }

    /// `private AssignmentExpressionSyntax ParseDictionaryInitializer()` (LanguageParser.cs:13642).
    pub(crate) fn parse_dictionary_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13642
        Default::default()
    }

    /// `private InitializerExpressionSyntax ParseComplexElementInitializer()` (LanguageParser.cs:13653).
    pub(crate) fn parse_complex_element_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13653
        Default::default()
    }

    /// `private bool IsImplicitlyTypedArray()` (LanguageParser.cs:13674).
    pub(crate) fn is_implicitly_typed_array(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13674
        Default::default()
    }

    /// `private ImplicitArrayCreationExpressionSyntax ParseImplicitlyTypedArrayCreation()` (LanguageParser.cs:13680).
    pub(crate) fn parse_implicitly_typed_array_creation(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13680
        Default::default()
    }

    /// `private InitializerExpressionSyntax ParseArrayInitializer()` (LanguageParser.cs:13720).
    pub(crate) fn parse_array_initializer(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13720
        Default::default()
    }

    /// `private ExpressionSyntax ParseStackAllocExpression()` (LanguageParser.cs:13749).
    pub(crate) fn parse_stack_alloc_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13749
        Default::default()
    }

    /// `private ExpressionSyntax ParseImplicitlyTypedStackAllocExpression()` (LanguageParser.cs:13756).
    pub(crate) fn parse_implicitly_typed_stack_alloc_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13756
        Default::default()
    }

    /// `private ExpressionSyntax ParseRegularStackAllocExpression()` (LanguageParser.cs:13787).
    pub(crate) fn parse_regular_stack_alloc_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13787
        Default::default()
    }

    /// `private AnonymousMethodExpressionSyntax ParseAnonymousMethodExpression()` (LanguageParser.cs:13795).
    pub(crate) fn parse_anonymous_method_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13795
        Default::default()
    }

    /// `private SyntaxList<SyntaxToken> ParseAnonymousFunctionModifiers()` (LanguageParser.cs:13847).
    pub(crate) fn parse_anonymous_function_modifiers(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13847
        Default::default()
    }

    /// `private bool IsAnonymousFunctionAsyncModifier()` (LanguageParser.cs:13872).
    pub(crate) fn is_anonymous_function_async_modifier(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:13872
        Default::default()
    }

    /// `private LambdaExpressionSyntax TryParseLambdaExpression()` (LanguageParser.cs:13894).
    pub(crate) fn try_parse_lambda_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13894
        Default::default()
    }

    /// `private LambdaExpressionSyntax ParseLambdaExpression()` (LanguageParser.cs:13909).
    pub(crate) fn parse_lambda_expression(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13909
        Default::default()
    }

    /// `private TUPLE ParseLambdaBody()` (LanguageParser.cs:13963).
    pub(crate) fn parse_lambda_body(&mut self) -> (Green, Green) {
        // TODO(port): LanguageParser.cs:13963
        Default::default()
    }

    /// `private ParameterListSyntax ParseLambdaParameterList()` (LanguageParser.cs:13968).
    pub(crate) fn parse_lambda_parameter_list(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:13968
        Default::default()
    }

    /// `private bool IsPossibleLambdaParameter()` (LanguageParser.cs:14001).
    pub(crate) fn is_possible_lambda_parameter(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:14001
        Default::default()
    }

    /// `private ParameterSyntax ParseLambdaParameter()` (LanguageParser.cs:14025).
    pub(crate) fn parse_lambda_parameter(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14025
        Default::default()
    }

    /// `private bool ShouldParseLambdaParameterType()` (LanguageParser.cs:14056).
    pub(crate) fn should_parse_lambda_parameter_type(&mut self) -> bool {
        // TODO(port): LanguageParser.cs:14056
        Default::default()
    }

    // LanguageParser.cs:14101: `private bool IsCurrentTokenQueryContextualKeyword`
    /// `private static bool IsTokenQueryContextualKeyword(SyntaxToken token)` (LanguageParser.cs:14104).
    pub(crate) fn is_token_query_contextual_keyword(token: Green) -> bool {
        // TODO(port): LanguageParser.cs:14104
        Default::default()
    }

    /// `private static bool IsTokenStartOfNewQueryClause(SyntaxToken token)` (LanguageParser.cs:14124).
    pub(crate) fn is_token_start_of_new_query_clause(token: Green) -> bool {
        // TODO(port): LanguageParser.cs:14124
        Default::default()
    }

    /// `private bool IsQueryExpression(bool mayBeVariableDeclaration, bool mayBeMemberDeclaration)` (LanguageParser.cs:14142).
    pub(crate) fn is_query_expression(
        &mut self,
        may_be_variable_declaration: bool,
        may_be_member_declaration: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:14142
        Default::default()
    }

    /// `private bool IsQueryExpressionAfterFrom(bool mayBeVariableDeclaration, bool mayBeMemberDeclaration)` (LanguageParser.cs:14149).
    pub(crate) fn is_query_expression_after_from(
        &mut self,
        may_be_variable_declaration: bool,
        may_be_member_declaration: bool,
    ) -> bool {
        // TODO(port): LanguageParser.cs:14149
        Default::default()
    }

    /// `private QueryExpressionSyntax ParseQueryExpression(Precedence precedence)` (LanguageParser.cs:14200).
    pub(crate) fn parse_query_expression(&mut self, precedence: Precedence) -> Green {
        // TODO(port): LanguageParser.cs:14200
        Default::default()
    }

    /// `private QueryBodySyntax ParseQueryBody()` (LanguageParser.cs:14212).
    pub(crate) fn parse_query_body(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14212
        Default::default()
    }

    /// `private FromClauseSyntax ParseFromClause()` (LanguageParser.cs:14259).
    pub(crate) fn parse_from_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14259
        Default::default()
    }

    /// `private JoinClauseSyntax ParseJoinClause()` (LanguageParser.cs:14292).
    pub(crate) fn parse_join_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14292
        Default::default()
    }

    /// `private LetClauseSyntax ParseLetClause()` (LanguageParser.cs:14312).
    pub(crate) fn parse_let_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14312
        Default::default()
    }

    /// `private WhereClauseSyntax ParseWhereClause()` (LanguageParser.cs:14325).
    pub(crate) fn parse_where_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14325
        Default::default()
    }

    /// `private OrderByClauseSyntax ParseOrderByClause()` (LanguageParser.cs:14333).
    pub(crate) fn parse_order_by_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14333
        Default::default()
    }

    /// `private OrderingSyntax ParseOrdering()` (LanguageParser.cs:14378).
    pub(crate) fn parse_ordering(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14378
        Default::default()
    }

    /// `private SelectClauseSyntax ParseSelectClause()` (LanguageParser.cs:14396).
    pub(crate) fn parse_select_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14396
        Default::default()
    }

    /// `private GroupClauseSyntax ParseGroupClause()` (LanguageParser.cs:14404).
    pub(crate) fn parse_group_clause(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14404
        Default::default()
    }

    /// `private QueryContinuationSyntax ParseQueryContinuation()` (LanguageParser.cs:14414).
    pub(crate) fn parse_query_continuation(&mut self) -> Green {
        // TODO(port): LanguageParser.cs:14414
        Default::default()
    }
}
