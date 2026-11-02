//! `SyntaxFacts` of Roslyn's `Syntax/SyntaxKindFacts.cs` (at the pin of AGENTS.md), ported method by method in its
//! order: keyword and punctuation sets, the contextual keywords, the kind of an expression or declaration a token
//! starts, `GetKeywordKind` and `GetContextualKeywordKind` of a text and `GetText` of a kind. A text is bytes (the
//! keywords are ASCII). Not ported: `GetOperatorKind` and `IsCheckedOperator`, which map metadata names
//! (`WellKnownMemberNames`) for the binder; the parser never calls them.

use crate::syntax_kind::SyntaxKind;

/// The defined kinds from `first` to `last` by number, as the C# `for (int i = (int)first; i <= (int)last; i++)` loops
/// yield them (numbers no kind has, e.g. the deleted `DataKeyword` 8441, are skipped as `Enum.IsDefined` asserts).
fn kinds(first: SyntaxKind, last: SyntaxKind) -> impl Iterator<Item = SyntaxKind> {
    (first.raw()..=last.raw()).filter_map(SyntaxKind::from_raw)
}

/// `SyntaxFacts.IsKeywordKind` (SyntaxKindFacts.cs:13).
pub fn is_keyword_kind(kind: SyntaxKind) -> bool {
    is_reserved_keyword(kind) || is_contextual_keyword(kind)
}

/// `SyntaxFacts.GetReservedKeywordKinds` (SyntaxKindFacts.cs:18).
pub fn get_reserved_keyword_kinds() -> impl Iterator<Item = SyntaxKind> {
    kinds(SyntaxKind::BoolKeyword, SyntaxKind::ImplicitKeyword)
}

/// `SyntaxFacts.GetKeywordKinds` (SyntaxKindFacts.cs:27).
pub fn get_keyword_kinds() -> impl Iterator<Item = SyntaxKind> {
    get_reserved_keyword_kinds().chain(get_contextual_keyword_kinds())
}

/// `SyntaxFacts.IsReservedKeyword` (SyntaxKindFacts.cs:40).
pub fn is_reserved_keyword(kind: SyntaxKind) -> bool {
    kind >= SyntaxKind::BoolKeyword && kind <= SyntaxKind::ImplicitKeyword
}

/// `SyntaxFacts.IsAttributeTargetSpecifier` (SyntaxKindFacts.cs:45).
pub fn is_attribute_target_specifier(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::AssemblyKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::EventKeyword
            | SyntaxKind::FieldKeyword
            | SyntaxKind::MethodKeyword
            | SyntaxKind::ParamKeyword
            | SyntaxKind::PropertyKeyword
            | SyntaxKind::ReturnKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::TypeVarKeyword
    )
}

/// `SyntaxFacts.IsAccessibilityModifier` (SyntaxKindFacts.cs:65).
pub fn is_accessibility_modifier(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::InternalKeyword
            | SyntaxKind::PublicKeyword
    )
}

/// `SyntaxFacts.IsPreprocessorKeyword` (SyntaxKindFacts.cs:79).
pub fn is_preprocessor_keyword(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::DefaultKeyword
            | SyntaxKind::IfKeyword
            | SyntaxKind::ElseKeyword
            | SyntaxKind::ElifKeyword
            | SyntaxKind::EndIfKeyword
            | SyntaxKind::RegionKeyword
            | SyntaxKind::EndRegionKeyword
            | SyntaxKind::DefineKeyword
            | SyntaxKind::UndefKeyword
            | SyntaxKind::WarningKeyword
            | SyntaxKind::ErrorKeyword
            | SyntaxKind::LineKeyword
            | SyntaxKind::PragmaKeyword
            | SyntaxKind::HiddenKeyword
            | SyntaxKind::ChecksumKeyword
            | SyntaxKind::DisableKeyword
            | SyntaxKind::RestoreKeyword
            | SyntaxKind::ReferenceKeyword
            | SyntaxKind::LoadKeyword
            | SyntaxKind::NullableKeyword
            | SyntaxKind::EnableKeyword
            | SyntaxKind::WarningsKeyword
            | SyntaxKind::AnnotationsKeyword
    )
}

/// `SyntaxFacts.IsPreprocessorContextualKeyword` (SyntaxKindFacts.cs:123).
pub fn is_preprocessor_contextual_keyword(kind: SyntaxKind) -> bool {
    match kind {
        SyntaxKind::TrueKeyword
        | SyntaxKind::FalseKeyword
        | SyntaxKind::DefaultKeyword
        | SyntaxKind::HiddenKeyword
        | SyntaxKind::ChecksumKeyword
        | SyntaxKind::DisableKeyword
        | SyntaxKind::RestoreKeyword
        | SyntaxKind::EnableKeyword
        | SyntaxKind::WarningsKeyword
        | SyntaxKind::AnnotationsKeyword => false,
        _ => is_preprocessor_keyword(kind),
    }
}

/// `SyntaxFacts.GetPreprocessorKeywordKinds` (SyntaxKindFacts.cs:143).
pub fn get_preprocessor_keyword_kinds() -> impl Iterator<Item = SyntaxKind> {
    [SyntaxKind::TrueKeyword, SyntaxKind::FalseKeyword, SyntaxKind::DefaultKeyword]
        .into_iter()
        .chain(kinds(SyntaxKind::ElifKeyword, SyntaxKind::RestoreKeyword))
}

/// `SyntaxFacts.IsPunctuation` (SyntaxKindFacts.cs:156).
pub fn is_punctuation(kind: SyntaxKind) -> bool {
    kind >= SyntaxKind::TildeToken && kind <= SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
}

/// `SyntaxFacts.IsLanguagePunctuation` (SyntaxKindFacts.cs:161).
pub fn is_language_punctuation(kind: SyntaxKind) -> bool {
    is_punctuation(kind) && !is_preprocessor_keyword(kind) && !is_debugger_special_punctuation(kind)
}

/// `SyntaxFacts.IsPreprocessorPunctuation` (SyntaxKindFacts.cs:166).
pub fn is_preprocessor_punctuation(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::HashToken
}

/// `SyntaxFacts.IsDebuggerSpecialPunctuation` (SyntaxKindFacts.cs:171).
fn is_debugger_special_punctuation(kind: SyntaxKind) -> bool {
    // TODO: What about "<>f_AnonymousMethod"? Or "123#"? What's this used for?
    kind == SyntaxKind::DollarToken
}

/// `SyntaxFacts.GetPunctuationKinds` (SyntaxKindFacts.cs:177).
pub fn get_punctuation_kinds() -> impl Iterator<Item = SyntaxKind> {
    kinds(SyntaxKind::TildeToken, SyntaxKind::DotDotToken)
        .chain(kinds(SyntaxKind::SlashGreaterThanToken, SyntaxKind::XmlProcessingInstructionEndToken))
        .chain(kinds(SyntaxKind::BarBarToken, SyntaxKind::QuestionQuestionEqualsToken))
        .chain([
            SyntaxKind::GreaterThanGreaterThanGreaterThanToken,
            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken,
        ])
}

/// `SyntaxFacts.IsPunctuationOrKeyword` (SyntaxKindFacts.cs:201).
pub fn is_punctuation_or_keyword(kind: SyntaxKind) -> bool {
    kind >= SyntaxKind::TildeToken && kind <= SyntaxKind::EndOfFileToken
}

/// `SyntaxFacts.IsLiteral` (SyntaxKindFacts.cs:206).
pub fn is_literal(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IdentifierToken
            | SyntaxKind::StringLiteralToken
            | SyntaxKind::Utf8StringLiteralToken
            | SyntaxKind::SingleLineRawStringLiteralToken
            | SyntaxKind::Utf8SingleLineRawStringLiteralToken
            | SyntaxKind::MultiLineRawStringLiteralToken
            | SyntaxKind::Utf8MultiLineRawStringLiteralToken
            | SyntaxKind::CharacterLiteralToken
            | SyntaxKind::NumericLiteralToken
            | SyntaxKind::XmlTextLiteralToken
            | SyntaxKind::XmlTextLiteralNewLineToken
            | SyntaxKind::XmlEntityLiteralToken
    )
}

/// `SyntaxFacts.IsAnyToken` (SyntaxKindFacts.cs:228).
pub fn is_any_token(kind: SyntaxKind) -> bool {
    if kind >= SyntaxKind::TildeToken && kind < SyntaxKind::EndOfLineTrivia {
        return true;
    }
    matches!(
        kind,
        SyntaxKind::InterpolatedStringToken
            | SyntaxKind::InterpolatedStringStartToken
            | SyntaxKind::InterpolatedVerbatimStringStartToken
            | SyntaxKind::InterpolatedMultiLineRawStringStartToken
            | SyntaxKind::InterpolatedSingleLineRawStringStartToken
            | SyntaxKind::InterpolatedStringTextToken
            | SyntaxKind::InterpolatedStringEndToken
            | SyntaxKind::InterpolatedRawStringEndToken
            | SyntaxKind::LoadKeyword
            | SyntaxKind::NullableKeyword
            | SyntaxKind::EnableKeyword
            | SyntaxKind::UnderscoreToken
            | SyntaxKind::MultiLineRawStringLiteralToken
            | SyntaxKind::SingleLineRawStringLiteralToken
    )
}

/// `SyntaxFacts.IsTrivia` (SyntaxKindFacts.cs:253).
pub fn is_trivia(kind: SyntaxKind) -> bool {
    match kind {
        SyntaxKind::EndOfLineTrivia
        | SyntaxKind::WhitespaceTrivia
        | SyntaxKind::SingleLineCommentTrivia
        | SyntaxKind::MultiLineCommentTrivia
        | SyntaxKind::SingleLineDocumentationCommentTrivia
        | SyntaxKind::MultiLineDocumentationCommentTrivia
        | SyntaxKind::DisabledTextTrivia
        | SyntaxKind::DocumentationCommentExteriorTrivia
        | SyntaxKind::ConflictMarkerTrivia => true,
        _ => is_preprocessor_directive(kind),
    }
}

/// `SyntaxFacts.IsPreprocessorDirective` (SyntaxKindFacts.cs:272).
pub fn is_preprocessor_directive(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IfDirectiveTrivia
            | SyntaxKind::ElifDirectiveTrivia
            | SyntaxKind::ElseDirectiveTrivia
            | SyntaxKind::EndIfDirectiveTrivia
            | SyntaxKind::RegionDirectiveTrivia
            | SyntaxKind::EndRegionDirectiveTrivia
            | SyntaxKind::DefineDirectiveTrivia
            | SyntaxKind::UndefDirectiveTrivia
            | SyntaxKind::ErrorDirectiveTrivia
            | SyntaxKind::WarningDirectiveTrivia
            | SyntaxKind::LineDirectiveTrivia
            | SyntaxKind::LineSpanDirectiveTrivia
            | SyntaxKind::PragmaWarningDirectiveTrivia
            | SyntaxKind::PragmaChecksumDirectiveTrivia
            | SyntaxKind::ReferenceDirectiveTrivia
            | SyntaxKind::LoadDirectiveTrivia
            | SyntaxKind::BadDirectiveTrivia
            | SyntaxKind::ShebangDirectiveTrivia
            | SyntaxKind::IgnoredDirectiveTrivia
            | SyntaxKind::NullableDirectiveTrivia
    )
}

/// `SyntaxFacts.IsName` (SyntaxKindFacts.cs:302).
pub fn is_name(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IdentifierName
            | SyntaxKind::GenericName
            | SyntaxKind::QualifiedName
            | SyntaxKind::AliasQualifiedName
    )
}

/// `SyntaxFacts.IsPredefinedType` (SyntaxKindFacts.cs:316).
pub fn is_predefined_type(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BoolKeyword
            | SyntaxKind::ByteKeyword
            | SyntaxKind::SByteKeyword
            | SyntaxKind::IntKeyword
            | SyntaxKind::UIntKeyword
            | SyntaxKind::ShortKeyword
            | SyntaxKind::UShortKeyword
            | SyntaxKind::LongKeyword
            | SyntaxKind::ULongKeyword
            | SyntaxKind::FloatKeyword
            | SyntaxKind::DoubleKeyword
            | SyntaxKind::DecimalKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::CharKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::VoidKeyword
    )
}

/// `SyntaxFacts.IsTypeSyntax` (SyntaxKindFacts.cs:342).
pub fn is_type_syntax(kind: SyntaxKind) -> bool {
    match kind {
        SyntaxKind::ArrayType
        | SyntaxKind::PointerType
        | SyntaxKind::NullableType
        | SyntaxKind::PredefinedType
        | SyntaxKind::TupleType
        | SyntaxKind::FunctionPointerType => true,
        _ => is_name(kind),
    }
}

/// `SyntaxFacts.IsGlobalMemberDeclaration` (SyntaxKindFacts.cs:361).
pub fn is_global_member_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::GlobalStatement
            | SyntaxKind::FieldDeclaration
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::EventDeclaration
            | SyntaxKind::EventFieldDeclaration
    )
}

/// `SyntaxFacts.IsTypeDeclaration` (SyntaxKindFacts.cs:376).
pub fn is_type_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ClassDeclaration
            | SyntaxKind::StructDeclaration
            | SyntaxKind::UnionDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::DelegateDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::RecordDeclaration
            | SyntaxKind::RecordStructDeclaration
            | SyntaxKind::ExtensionBlockDeclaration
    )
}

/// `SyntaxFacts.IsNamespaceMemberDeclaration` (SyntaxKindFacts.cs:396).
pub fn is_namespace_member_declaration(kind: SyntaxKind) -> bool {
    is_type_declaration(kind)
        || kind == SyntaxKind::NamespaceDeclaration
        || kind == SyntaxKind::FileScopedNamespaceDeclaration
}

/// `SyntaxFacts.IsAnyUnaryExpression` (SyntaxKindFacts.cs:401).
pub fn is_any_unary_expression(token: SyntaxKind) -> bool {
    is_prefix_unary_expression(token) || is_postfix_unary_expression(token)
}

/// `SyntaxFacts.IsPrefixUnaryExpression` (SyntaxKindFacts.cs:406).
pub fn is_prefix_unary_expression(token: SyntaxKind) -> bool {
    get_prefix_unary_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.IsPrefixUnaryExpressionOperatorToken` (SyntaxKindFacts.cs:411).
pub fn is_prefix_unary_expression_operator_token(token: SyntaxKind) -> bool {
    get_prefix_unary_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.GetPrefixUnaryExpression` (SyntaxKindFacts.cs:416).
pub fn get_prefix_unary_expression(token: SyntaxKind) -> SyntaxKind {
    match token {
        SyntaxKind::PlusToken => SyntaxKind::UnaryPlusExpression,
        SyntaxKind::MinusToken => SyntaxKind::UnaryMinusExpression,
        SyntaxKind::TildeToken => SyntaxKind::BitwiseNotExpression,
        SyntaxKind::ExclamationToken => SyntaxKind::LogicalNotExpression,
        SyntaxKind::PlusPlusToken => SyntaxKind::PreIncrementExpression,
        SyntaxKind::MinusMinusToken => SyntaxKind::PreDecrementExpression,
        SyntaxKind::AmpersandToken => SyntaxKind::AddressOfExpression,
        SyntaxKind::AsteriskToken => SyntaxKind::PointerIndirectionExpression,
        SyntaxKind::CaretToken => SyntaxKind::IndexExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsPostfixUnaryExpression` (SyntaxKindFacts.cs:443).
pub fn is_postfix_unary_expression(token: SyntaxKind) -> bool {
    get_postfix_unary_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.IsPostfixUnaryExpressionToken` (SyntaxKindFacts.cs:448).
pub fn is_postfix_unary_expression_token(token: SyntaxKind) -> bool {
    get_postfix_unary_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.GetPostfixUnaryExpression` (SyntaxKindFacts.cs:453).
pub fn get_postfix_unary_expression(token: SyntaxKind) -> SyntaxKind {
    match token {
        SyntaxKind::PlusPlusToken => SyntaxKind::PostIncrementExpression,
        SyntaxKind::MinusMinusToken => SyntaxKind::PostDecrementExpression,
        SyntaxKind::ExclamationToken => SyntaxKind::SuppressNullableWarningExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsIncrementOrDecrementOperator` (SyntaxKindFacts.cs:468).
pub fn is_increment_or_decrement_operator(token: SyntaxKind) -> bool {
    matches!(token, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
}

/// `SyntaxFacts.IsUnaryOperatorDeclarationToken` (SyntaxKindFacts.cs:480).
pub fn is_unary_operator_declaration_token(token: SyntaxKind) -> bool {
    is_prefix_unary_expression_operator_token(token)
        || token == SyntaxKind::TrueKeyword
        || token == SyntaxKind::FalseKeyword
}

/// `SyntaxFacts.IsAnyOverloadableOperator` (SyntaxKindFacts.cs:487).
pub fn is_any_overloadable_operator(kind: SyntaxKind) -> bool {
    is_overloadable_binary_operator(kind)
        || is_overloadable_unary_operator(kind)
        || is_overloadable_compound_assignment_operator(kind)
}

/// `SyntaxFacts.IsOverloadableBinaryOperator` (SyntaxKindFacts.cs:494).
pub fn is_overloadable_binary_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::AsteriskToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::CaretToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::BarToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
            | SyntaxKind::ExclamationEqualsToken
    )
}

/// `SyntaxFacts.IsOverloadableUnaryOperator` (SyntaxKindFacts.cs:521).
pub fn is_overloadable_unary_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
    )
}

/// `SyntaxFacts.IsOverloadableCompoundAssignmentOperator` (SyntaxKindFacts.cs:539).
pub fn is_overloadable_compound_assignment_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PlusEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::CaretEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
    )
}

/// `SyntaxFacts.IsPrimaryFunction` (SyntaxKindFacts.cs:560).
pub fn is_primary_function(keyword: SyntaxKind) -> bool {
    get_primary_function(keyword) != SyntaxKind::None
}

/// `SyntaxFacts.GetPrimaryFunction` (SyntaxKindFacts.cs:565).
pub fn get_primary_function(keyword: SyntaxKind) -> SyntaxKind {
    match keyword {
        SyntaxKind::MakeRefKeyword => SyntaxKind::MakeRefExpression,
        SyntaxKind::RefTypeKeyword => SyntaxKind::RefTypeExpression,
        SyntaxKind::RefValueKeyword => SyntaxKind::RefValueExpression,
        SyntaxKind::CheckedKeyword => SyntaxKind::CheckedExpression,
        SyntaxKind::UncheckedKeyword => SyntaxKind::UncheckedExpression,
        SyntaxKind::UnsafeKeyword => SyntaxKind::UnsafeExpression,
        SyntaxKind::DefaultKeyword => SyntaxKind::DefaultExpression,
        SyntaxKind::TypeOfKeyword => SyntaxKind::TypeOfExpression,
        SyntaxKind::SizeOfKeyword => SyntaxKind::SizeOfExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsLiteralExpression` (SyntaxKindFacts.cs:592).
pub fn is_literal_expression(token: SyntaxKind) -> bool {
    get_literal_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.GetLiteralExpression` (SyntaxKindFacts.cs:597).
pub fn get_literal_expression(token: SyntaxKind) -> SyntaxKind {
    match token {
        SyntaxKind::StringLiteralToken => SyntaxKind::StringLiteralExpression,
        SyntaxKind::Utf8StringLiteralToken => SyntaxKind::Utf8StringLiteralExpression,
        SyntaxKind::SingleLineRawStringLiteralToken => SyntaxKind::StringLiteralExpression,
        SyntaxKind::Utf8SingleLineRawStringLiteralToken => SyntaxKind::Utf8StringLiteralExpression,
        SyntaxKind::MultiLineRawStringLiteralToken => SyntaxKind::StringLiteralExpression,
        SyntaxKind::Utf8MultiLineRawStringLiteralToken => SyntaxKind::Utf8StringLiteralExpression,
        SyntaxKind::CharacterLiteralToken => SyntaxKind::CharacterLiteralExpression,
        SyntaxKind::NumericLiteralToken => SyntaxKind::NumericLiteralExpression,
        SyntaxKind::NullKeyword => SyntaxKind::NullLiteralExpression,
        SyntaxKind::TrueKeyword => SyntaxKind::TrueLiteralExpression,
        SyntaxKind::FalseKeyword => SyntaxKind::FalseLiteralExpression,
        SyntaxKind::ArgListKeyword => SyntaxKind::ArgListExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsInstanceExpression` (SyntaxKindFacts.cs:617).
pub fn is_instance_expression(token: SyntaxKind) -> bool {
    get_instance_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.GetInstanceExpression` (SyntaxKindFacts.cs:622).
pub fn get_instance_expression(token: SyntaxKind) -> SyntaxKind {
    match token {
        SyntaxKind::ThisKeyword => SyntaxKind::ThisExpression,
        SyntaxKind::BaseKeyword => SyntaxKind::BaseExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsBinaryExpression` (SyntaxKindFacts.cs:635).
pub fn is_binary_expression(token: SyntaxKind) -> bool {
    get_binary_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.IsBinaryExpressionOperatorToken` (SyntaxKindFacts.cs:640).
pub fn is_binary_expression_operator_token(token: SyntaxKind) -> bool {
    get_binary_expression(token) != SyntaxKind::None
}

/// `SyntaxFacts.GetBinaryExpression` (SyntaxKindFacts.cs:645).
pub fn get_binary_expression(token: SyntaxKind) -> SyntaxKind {
    match token {
        SyntaxKind::QuestionQuestionToken => SyntaxKind::CoalesceExpression,
        SyntaxKind::IsKeyword => SyntaxKind::IsExpression,
        SyntaxKind::AsKeyword => SyntaxKind::AsExpression,
        SyntaxKind::BarToken => SyntaxKind::BitwiseOrExpression,
        SyntaxKind::CaretToken => SyntaxKind::ExclusiveOrExpression,
        SyntaxKind::AmpersandToken => SyntaxKind::BitwiseAndExpression,
        SyntaxKind::EqualsEqualsToken => SyntaxKind::EqualsExpression,
        SyntaxKind::ExclamationEqualsToken => SyntaxKind::NotEqualsExpression,
        SyntaxKind::LessThanToken => SyntaxKind::LessThanExpression,
        SyntaxKind::LessThanEqualsToken => SyntaxKind::LessThanOrEqualExpression,
        SyntaxKind::GreaterThanToken => SyntaxKind::GreaterThanExpression,
        SyntaxKind::GreaterThanEqualsToken => SyntaxKind::GreaterThanOrEqualExpression,
        SyntaxKind::LessThanLessThanToken => SyntaxKind::LeftShiftExpression,
        SyntaxKind::GreaterThanGreaterThanToken => SyntaxKind::RightShiftExpression,
        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => SyntaxKind::UnsignedRightShiftExpression,
        SyntaxKind::PlusToken => SyntaxKind::AddExpression,
        SyntaxKind::MinusToken => SyntaxKind::SubtractExpression,
        SyntaxKind::AsteriskToken => SyntaxKind::MultiplyExpression,
        SyntaxKind::SlashToken => SyntaxKind::DivideExpression,
        SyntaxKind::PercentToken => SyntaxKind::ModuloExpression,
        SyntaxKind::AmpersandAmpersandToken => SyntaxKind::LogicalAndExpression,
        SyntaxKind::BarBarToken => SyntaxKind::LogicalOrExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsAssignmentExpression` (SyntaxKindFacts.cs:698).
pub fn is_assignment_expression(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CoalesceAssignmentExpression
            | SyntaxKind::OrAssignmentExpression
            | SyntaxKind::AndAssignmentExpression
            | SyntaxKind::ExclusiveOrAssignmentExpression
            | SyntaxKind::LeftShiftAssignmentExpression
            | SyntaxKind::RightShiftAssignmentExpression
            | SyntaxKind::UnsignedRightShiftAssignmentExpression
            | SyntaxKind::AddAssignmentExpression
            | SyntaxKind::SubtractAssignmentExpression
            | SyntaxKind::MultiplyAssignmentExpression
            | SyntaxKind::DivideAssignmentExpression
            | SyntaxKind::ModuloAssignmentExpression
            | SyntaxKind::SimpleAssignmentExpression
    )
}

/// `SyntaxFacts.IsAssignmentExpressionOperatorToken` (SyntaxKindFacts.cs:721).
pub fn is_assignment_expression_operator_token(token: SyntaxKind) -> bool {
    matches!(
        token,
        SyntaxKind::QuestionQuestionEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::CaretEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::PlusEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::EqualsToken
    )
}

/// `SyntaxFacts.GetAssignmentExpression` (SyntaxKindFacts.cs:744).
pub fn get_assignment_expression(token: SyntaxKind) -> SyntaxKind {
    match token {
        SyntaxKind::BarEqualsToken => SyntaxKind::OrAssignmentExpression,
        SyntaxKind::AmpersandEqualsToken => SyntaxKind::AndAssignmentExpression,
        SyntaxKind::CaretEqualsToken => SyntaxKind::ExclusiveOrAssignmentExpression,
        SyntaxKind::LessThanLessThanEqualsToken => SyntaxKind::LeftShiftAssignmentExpression,
        SyntaxKind::GreaterThanGreaterThanEqualsToken => SyntaxKind::RightShiftAssignmentExpression,
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => SyntaxKind::UnsignedRightShiftAssignmentExpression,
        SyntaxKind::PlusEqualsToken => SyntaxKind::AddAssignmentExpression,
        SyntaxKind::MinusEqualsToken => SyntaxKind::SubtractAssignmentExpression,
        SyntaxKind::AsteriskEqualsToken => SyntaxKind::MultiplyAssignmentExpression,
        SyntaxKind::SlashEqualsToken => SyntaxKind::DivideAssignmentExpression,
        SyntaxKind::PercentEqualsToken => SyntaxKind::ModuloAssignmentExpression,
        SyntaxKind::EqualsToken => SyntaxKind::SimpleAssignmentExpression,
        SyntaxKind::QuestionQuestionEqualsToken => SyntaxKind::CoalesceAssignmentExpression,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetCheckStatement` (SyntaxKindFacts.cs:779).
pub fn get_check_statement(keyword: SyntaxKind) -> SyntaxKind {
    match keyword {
        SyntaxKind::CheckedKeyword => SyntaxKind::CheckedStatement,
        SyntaxKind::UncheckedKeyword => SyntaxKind::UncheckedStatement,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetAccessorDeclarationKind` (SyntaxKindFacts.cs:792).
pub fn get_accessor_declaration_kind(keyword: SyntaxKind) -> SyntaxKind {
    match keyword {
        SyntaxKind::GetKeyword => SyntaxKind::GetAccessorDeclaration,
        SyntaxKind::SetKeyword => SyntaxKind::SetAccessorDeclaration,
        SyntaxKind::InitKeyword => SyntaxKind::InitAccessorDeclaration,
        SyntaxKind::AddKeyword => SyntaxKind::AddAccessorDeclaration,
        SyntaxKind::RemoveKeyword => SyntaxKind::RemoveAccessorDeclaration,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.IsAccessorDeclaration` (SyntaxKindFacts.cs:811).
pub fn is_accessor_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::GetAccessorDeclaration
            | SyntaxKind::SetAccessorDeclaration
            | SyntaxKind::InitAccessorDeclaration
            | SyntaxKind::AddAccessorDeclaration
            | SyntaxKind::RemoveAccessorDeclaration
    )
}

/// `SyntaxFacts.IsAccessorDeclarationKeyword` (SyntaxKindFacts.cs:826).
pub fn is_accessor_declaration_keyword(keyword: SyntaxKind) -> bool {
    matches!(
        keyword,
        SyntaxKind::GetKeyword
            | SyntaxKind::SetKeyword
            | SyntaxKind::InitKeyword
            | SyntaxKind::AddKeyword
            | SyntaxKind::RemoveKeyword
    )
}

/// `SyntaxFacts.GetSwitchLabelKind` (SyntaxKindFacts.cs:841).
pub fn get_switch_label_kind(keyword: SyntaxKind) -> SyntaxKind {
    match keyword {
        SyntaxKind::CaseKeyword => SyntaxKind::CaseSwitchLabel,
        SyntaxKind::DefaultKeyword => SyntaxKind::DefaultSwitchLabel,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetBaseTypeDeclarationKind` (SyntaxKindFacts.cs:854).
pub fn get_base_type_declaration_kind(kind: SyntaxKind) -> SyntaxKind {
    if kind == SyntaxKind::EnumKeyword { SyntaxKind::EnumDeclaration } else { get_type_declaration_kind(kind) }
}

/// `SyntaxFacts.GetTypeDeclarationKind` (SyntaxKindFacts.cs:859).
pub fn get_type_declaration_kind(kind: SyntaxKind) -> SyntaxKind {
    match kind {
        SyntaxKind::ClassKeyword => SyntaxKind::ClassDeclaration,
        SyntaxKind::StructKeyword => SyntaxKind::StructDeclaration,
        SyntaxKind::UnionKeyword => SyntaxKind::UnionDeclaration,
        SyntaxKind::InterfaceKeyword => SyntaxKind::InterfaceDeclaration,
        SyntaxKind::RecordKeyword => SyntaxKind::RecordDeclaration,
        SyntaxKind::ExtensionKeyword => SyntaxKind::ExtensionBlockDeclaration,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetKeywordKind` (SyntaxKindFacts.cs:880).
pub fn get_keyword_kind(text: &[u8]) -> SyntaxKind {
    match text {
        b"bool" => SyntaxKind::BoolKeyword,
        b"byte" => SyntaxKind::ByteKeyword,
        b"sbyte" => SyntaxKind::SByteKeyword,
        b"short" => SyntaxKind::ShortKeyword,
        b"ushort" => SyntaxKind::UShortKeyword,
        b"int" => SyntaxKind::IntKeyword,
        b"uint" => SyntaxKind::UIntKeyword,
        b"long" => SyntaxKind::LongKeyword,
        b"ulong" => SyntaxKind::ULongKeyword,
        b"double" => SyntaxKind::DoubleKeyword,
        b"float" => SyntaxKind::FloatKeyword,
        b"decimal" => SyntaxKind::DecimalKeyword,
        b"string" => SyntaxKind::StringKeyword,
        b"char" => SyntaxKind::CharKeyword,
        b"void" => SyntaxKind::VoidKeyword,
        b"object" => SyntaxKind::ObjectKeyword,
        b"typeof" => SyntaxKind::TypeOfKeyword,
        b"sizeof" => SyntaxKind::SizeOfKeyword,
        b"null" => SyntaxKind::NullKeyword,
        b"true" => SyntaxKind::TrueKeyword,
        b"false" => SyntaxKind::FalseKeyword,
        b"if" => SyntaxKind::IfKeyword,
        b"else" => SyntaxKind::ElseKeyword,
        b"while" => SyntaxKind::WhileKeyword,
        b"for" => SyntaxKind::ForKeyword,
        b"foreach" => SyntaxKind::ForEachKeyword,
        b"do" => SyntaxKind::DoKeyword,
        b"switch" => SyntaxKind::SwitchKeyword,
        b"case" => SyntaxKind::CaseKeyword,
        b"default" => SyntaxKind::DefaultKeyword,
        b"lock" => SyntaxKind::LockKeyword,
        b"try" => SyntaxKind::TryKeyword,
        b"throw" => SyntaxKind::ThrowKeyword,
        b"catch" => SyntaxKind::CatchKeyword,
        b"finally" => SyntaxKind::FinallyKeyword,
        b"goto" => SyntaxKind::GotoKeyword,
        b"break" => SyntaxKind::BreakKeyword,
        b"continue" => SyntaxKind::ContinueKeyword,
        b"return" => SyntaxKind::ReturnKeyword,
        b"public" => SyntaxKind::PublicKeyword,
        b"private" => SyntaxKind::PrivateKeyword,
        b"internal" => SyntaxKind::InternalKeyword,
        b"protected" => SyntaxKind::ProtectedKeyword,
        b"static" => SyntaxKind::StaticKeyword,
        b"readonly" => SyntaxKind::ReadOnlyKeyword,
        b"sealed" => SyntaxKind::SealedKeyword,
        b"const" => SyntaxKind::ConstKeyword,
        b"fixed" => SyntaxKind::FixedKeyword,
        b"stackalloc" => SyntaxKind::StackAllocKeyword,
        b"volatile" => SyntaxKind::VolatileKeyword,
        b"new" => SyntaxKind::NewKeyword,
        b"override" => SyntaxKind::OverrideKeyword,
        b"abstract" => SyntaxKind::AbstractKeyword,
        b"virtual" => SyntaxKind::VirtualKeyword,
        b"event" => SyntaxKind::EventKeyword,
        b"extern" => SyntaxKind::ExternKeyword,
        b"ref" => SyntaxKind::RefKeyword,
        b"out" => SyntaxKind::OutKeyword,
        b"in" => SyntaxKind::InKeyword,
        b"is" => SyntaxKind::IsKeyword,
        b"as" => SyntaxKind::AsKeyword,
        b"params" => SyntaxKind::ParamsKeyword,
        b"__arglist" => SyntaxKind::ArgListKeyword,
        b"__makeref" => SyntaxKind::MakeRefKeyword,
        b"__reftype" => SyntaxKind::RefTypeKeyword,
        b"__refvalue" => SyntaxKind::RefValueKeyword,
        b"this" => SyntaxKind::ThisKeyword,
        b"base" => SyntaxKind::BaseKeyword,
        b"namespace" => SyntaxKind::NamespaceKeyword,
        b"using" => SyntaxKind::UsingKeyword,
        b"class" => SyntaxKind::ClassKeyword,
        b"struct" => SyntaxKind::StructKeyword,
        b"interface" => SyntaxKind::InterfaceKeyword,
        b"enum" => SyntaxKind::EnumKeyword,
        b"delegate" => SyntaxKind::DelegateKeyword,
        b"checked" => SyntaxKind::CheckedKeyword,
        b"unchecked" => SyntaxKind::UncheckedKeyword,
        b"unsafe" => SyntaxKind::UnsafeKeyword,
        b"operator" => SyntaxKind::OperatorKeyword,
        b"implicit" => SyntaxKind::ImplicitKeyword,
        b"explicit" => SyntaxKind::ExplicitKeyword,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetPreprocessorKeywordKind` (SyntaxKindFacts.cs:1181).
pub fn get_preprocessor_keyword_kind(text: &[u8]) -> SyntaxKind {
    match text {
        b"true" => SyntaxKind::TrueKeyword,
        b"false" => SyntaxKind::FalseKeyword,
        b"default" => SyntaxKind::DefaultKeyword,
        b"if" => SyntaxKind::IfKeyword,
        b"else" => SyntaxKind::ElseKeyword,
        b"elif" => SyntaxKind::ElifKeyword,
        b"endif" => SyntaxKind::EndIfKeyword,
        b"region" => SyntaxKind::RegionKeyword,
        b"endregion" => SyntaxKind::EndRegionKeyword,
        b"define" => SyntaxKind::DefineKeyword,
        b"undef" => SyntaxKind::UndefKeyword,
        b"warning" => SyntaxKind::WarningKeyword,
        b"error" => SyntaxKind::ErrorKeyword,
        b"line" => SyntaxKind::LineKeyword,
        b"pragma" => SyntaxKind::PragmaKeyword,
        b"hidden" => SyntaxKind::HiddenKeyword,
        b"checksum" => SyntaxKind::ChecksumKeyword,
        b"disable" => SyntaxKind::DisableKeyword,
        b"restore" => SyntaxKind::RestoreKeyword,
        b"r" => SyntaxKind::ReferenceKeyword,
        b"load" => SyntaxKind::LoadKeyword,
        b"nullable" => SyntaxKind::NullableKeyword,
        b"enable" => SyntaxKind::EnableKeyword,
        b"warnings" => SyntaxKind::WarningsKeyword,
        b"annotations" => SyntaxKind::AnnotationsKeyword,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetContextualKeywordKinds` (SyntaxKindFacts.cs:1240).
pub fn get_contextual_keyword_kinds() -> impl Iterator<Item = SyntaxKind> {
    // 8441 corresponds to a deleted kind (DataKeyword) that was previously shipped.
    kinds(SyntaxKind::YieldKeyword, SyntaxKind::SafeKeyword)
}

/// `SyntaxFacts.IsContextualKeyword` (SyntaxKindFacts.cs:1253).
pub fn is_contextual_keyword(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::YieldKeyword
            | SyntaxKind::PartialKeyword
            | SyntaxKind::FromKeyword
            | SyntaxKind::GroupKeyword
            | SyntaxKind::JoinKeyword
            | SyntaxKind::IntoKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::ByKeyword
            | SyntaxKind::WhereKeyword
            | SyntaxKind::SelectKeyword
            | SyntaxKind::GetKeyword
            | SyntaxKind::SetKeyword
            | SyntaxKind::AddKeyword
            | SyntaxKind::RemoveKeyword
            | SyntaxKind::OrderByKeyword
            | SyntaxKind::AliasKeyword
            | SyntaxKind::OnKeyword
            | SyntaxKind::EqualsKeyword
            | SyntaxKind::AscendingKeyword
            | SyntaxKind::DescendingKeyword
            | SyntaxKind::AssemblyKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::GlobalKeyword
            | SyntaxKind::FieldKeyword
            | SyntaxKind::MethodKeyword
            | SyntaxKind::ParamKeyword
            | SyntaxKind::PropertyKeyword
            | SyntaxKind::TypeVarKeyword
            | SyntaxKind::NameOfKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::WhenKeyword
            | SyntaxKind::UnderscoreToken
            | SyntaxKind::VarKeyword
            | SyntaxKind::OrKeyword
            | SyntaxKind::AndKeyword
            | SyntaxKind::NotKeyword
            | SyntaxKind::WithKeyword
            | SyntaxKind::InitKeyword
            | SyntaxKind::RecordKeyword
            | SyntaxKind::ManagedKeyword
            | SyntaxKind::UnmanagedKeyword
            | SyntaxKind::RequiredKeyword
            | SyntaxKind::ScopedKeyword
            | SyntaxKind::FileKeyword
            | SyntaxKind::AllowsKeyword
            | SyntaxKind::ExtensionKeyword
            | SyntaxKind::UnionKeyword
            | SyntaxKind::ClosedKeyword
            | SyntaxKind::SafeKeyword
    )
}

/// `SyntaxFacts.IsQueryContextualKeyword` (SyntaxKindFacts.cs:1314).
pub fn is_query_contextual_keyword(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FromKeyword
            | SyntaxKind::WhereKeyword
            | SyntaxKind::SelectKeyword
            | SyntaxKind::GroupKeyword
            | SyntaxKind::IntoKeyword
            | SyntaxKind::OrderByKeyword
            | SyntaxKind::JoinKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::OnKeyword
            | SyntaxKind::EqualsKeyword
            | SyntaxKind::ByKeyword
            | SyntaxKind::AscendingKeyword
            | SyntaxKind::DescendingKeyword
    )
}

/// `SyntaxFacts.GetContextualKeywordKind` (SyntaxKindFacts.cs:1337).
pub fn get_contextual_keyword_kind(text: &[u8]) -> SyntaxKind {
    match text {
        b"yield" => SyntaxKind::YieldKeyword,
        b"partial" => SyntaxKind::PartialKeyword,
        b"from" => SyntaxKind::FromKeyword,
        b"group" => SyntaxKind::GroupKeyword,
        b"join" => SyntaxKind::JoinKeyword,
        b"into" => SyntaxKind::IntoKeyword,
        b"let" => SyntaxKind::LetKeyword,
        b"by" => SyntaxKind::ByKeyword,
        b"where" => SyntaxKind::WhereKeyword,
        b"select" => SyntaxKind::SelectKeyword,
        b"get" => SyntaxKind::GetKeyword,
        b"set" => SyntaxKind::SetKeyword,
        b"add" => SyntaxKind::AddKeyword,
        b"remove" => SyntaxKind::RemoveKeyword,
        b"orderby" => SyntaxKind::OrderByKeyword,
        b"alias" => SyntaxKind::AliasKeyword,
        b"on" => SyntaxKind::OnKeyword,
        b"equals" => SyntaxKind::EqualsKeyword,
        b"ascending" => SyntaxKind::AscendingKeyword,
        b"descending" => SyntaxKind::DescendingKeyword,
        b"assembly" => SyntaxKind::AssemblyKeyword,
        b"module" => SyntaxKind::ModuleKeyword,
        b"type" => SyntaxKind::TypeKeyword,
        b"field" => SyntaxKind::FieldKeyword,
        b"method" => SyntaxKind::MethodKeyword,
        b"param" => SyntaxKind::ParamKeyword,
        b"property" => SyntaxKind::PropertyKeyword,
        b"typevar" => SyntaxKind::TypeVarKeyword,
        b"global" => SyntaxKind::GlobalKeyword,
        b"async" => SyntaxKind::AsyncKeyword,
        b"await" => SyntaxKind::AwaitKeyword,
        b"when" => SyntaxKind::WhenKeyword,
        b"nameof" => SyntaxKind::NameOfKeyword,
        b"_" => SyntaxKind::UnderscoreToken,
        b"var" => SyntaxKind::VarKeyword,
        b"and" => SyntaxKind::AndKeyword,
        b"or" => SyntaxKind::OrKeyword,
        b"not" => SyntaxKind::NotKeyword,
        b"with" => SyntaxKind::WithKeyword,
        b"init" => SyntaxKind::InitKeyword,
        b"record" => SyntaxKind::RecordKeyword,
        b"managed" => SyntaxKind::ManagedKeyword,
        b"unmanaged" => SyntaxKind::UnmanagedKeyword,
        b"required" => SyntaxKind::RequiredKeyword,
        b"scoped" => SyntaxKind::ScopedKeyword,
        b"file" => SyntaxKind::FileKeyword,
        b"allows" => SyntaxKind::AllowsKeyword,
        b"extension" => SyntaxKind::ExtensionKeyword,
        b"union" => SyntaxKind::UnionKeyword,
        b"closed" => SyntaxKind::ClosedKeyword,
        b"safe" => SyntaxKind::SafeKeyword,
        _ => SyntaxKind::None,
    }
}

/// `SyntaxFacts.GetText` (SyntaxKindFacts.cs:1448).
pub fn get_text(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::TildeToken => "~",
        SyntaxKind::ExclamationToken => "!",
        SyntaxKind::DollarToken => "$",
        SyntaxKind::PercentToken => "%",
        SyntaxKind::CaretToken => "^",
        SyntaxKind::AmpersandToken => "&",
        SyntaxKind::AsteriskToken => "*",
        SyntaxKind::OpenParenToken => "(",
        SyntaxKind::CloseParenToken => ")",
        SyntaxKind::MinusToken => "-",
        SyntaxKind::PlusToken => "+",
        SyntaxKind::EqualsToken => "=",
        SyntaxKind::OpenBraceToken => "{",
        SyntaxKind::CloseBraceToken => "}",
        SyntaxKind::OpenBracketToken => "[",
        SyntaxKind::CloseBracketToken => "]",
        SyntaxKind::BarToken => "|",
        SyntaxKind::BackslashToken => "\\",
        SyntaxKind::ColonToken => ":",
        SyntaxKind::SemicolonToken => ";",
        SyntaxKind::DoubleQuoteToken => "\"",
        SyntaxKind::SingleQuoteToken => "'",
        SyntaxKind::LessThanToken => "<",
        SyntaxKind::CommaToken => ",",
        SyntaxKind::GreaterThanToken => ">",
        SyntaxKind::DotToken => ".",
        SyntaxKind::QuestionToken => "?",
        SyntaxKind::HashToken => "#",
        SyntaxKind::SlashToken => "/",
        SyntaxKind::SlashGreaterThanToken => "/>",
        SyntaxKind::LessThanSlashToken => "</",
        SyntaxKind::XmlCommentStartToken => "<!--",
        SyntaxKind::XmlCommentEndToken => "-->",
        SyntaxKind::XmlCDataStartToken => "<![CDATA[",
        SyntaxKind::XmlCDataEndToken => "]]>",
        SyntaxKind::XmlProcessingInstructionStartToken => "<?",
        SyntaxKind::XmlProcessingInstructionEndToken => "?>",
        SyntaxKind::BarBarToken => "||",
        SyntaxKind::AmpersandAmpersandToken => "&&",
        SyntaxKind::MinusMinusToken => "--",
        SyntaxKind::PlusPlusToken => "++",
        SyntaxKind::ColonColonToken => "::",
        SyntaxKind::QuestionQuestionToken => "??",
        SyntaxKind::MinusGreaterThanToken => "->",
        SyntaxKind::ExclamationEqualsToken => "!=",
        SyntaxKind::EqualsEqualsToken => "==",
        SyntaxKind::EqualsGreaterThanToken => "=>",
        SyntaxKind::LessThanEqualsToken => "<=",
        SyntaxKind::LessThanLessThanToken => "<<",
        SyntaxKind::LessThanLessThanEqualsToken => "<<=",
        SyntaxKind::GreaterThanEqualsToken => ">=",
        SyntaxKind::GreaterThanGreaterThanToken => ">>",
        SyntaxKind::GreaterThanGreaterThanEqualsToken => ">>=",
        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => ">>>",
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => ">>>=",
        SyntaxKind::SlashEqualsToken => "/=",
        SyntaxKind::AsteriskEqualsToken => "*=",
        SyntaxKind::BarEqualsToken => "|=",
        SyntaxKind::AmpersandEqualsToken => "&=",
        SyntaxKind::PlusEqualsToken => "+=",
        SyntaxKind::MinusEqualsToken => "-=",
        SyntaxKind::CaretEqualsToken => "^=",
        SyntaxKind::PercentEqualsToken => "%=",
        SyntaxKind::QuestionQuestionEqualsToken => "??=",
        SyntaxKind::DotDotToken => "..",
        SyntaxKind::BoolKeyword => "bool",
        SyntaxKind::ByteKeyword => "byte",
        SyntaxKind::SByteKeyword => "sbyte",
        SyntaxKind::ShortKeyword => "short",
        SyntaxKind::UShortKeyword => "ushort",
        SyntaxKind::IntKeyword => "int",
        SyntaxKind::UIntKeyword => "uint",
        SyntaxKind::LongKeyword => "long",
        SyntaxKind::ULongKeyword => "ulong",
        SyntaxKind::DoubleKeyword => "double",
        SyntaxKind::FloatKeyword => "float",
        SyntaxKind::DecimalKeyword => "decimal",
        SyntaxKind::StringKeyword => "string",
        SyntaxKind::CharKeyword => "char",
        SyntaxKind::VoidKeyword => "void",
        SyntaxKind::ObjectKeyword => "object",
        SyntaxKind::TypeOfKeyword => "typeof",
        SyntaxKind::SizeOfKeyword => "sizeof",
        SyntaxKind::NullKeyword => "null",
        SyntaxKind::TrueKeyword => "true",
        SyntaxKind::FalseKeyword => "false",
        SyntaxKind::IfKeyword => "if",
        SyntaxKind::ElseKeyword => "else",
        SyntaxKind::WhileKeyword => "while",
        SyntaxKind::ForKeyword => "for",
        SyntaxKind::ForEachKeyword => "foreach",
        SyntaxKind::DoKeyword => "do",
        SyntaxKind::SwitchKeyword => "switch",
        SyntaxKind::CaseKeyword => "case",
        SyntaxKind::DefaultKeyword => "default",
        SyntaxKind::TryKeyword => "try",
        SyntaxKind::CatchKeyword => "catch",
        SyntaxKind::FinallyKeyword => "finally",
        SyntaxKind::LockKeyword => "lock",
        SyntaxKind::GotoKeyword => "goto",
        SyntaxKind::BreakKeyword => "break",
        SyntaxKind::ContinueKeyword => "continue",
        SyntaxKind::ReturnKeyword => "return",
        SyntaxKind::ThrowKeyword => "throw",
        SyntaxKind::PublicKeyword => "public",
        SyntaxKind::PrivateKeyword => "private",
        SyntaxKind::InternalKeyword => "internal",
        SyntaxKind::ProtectedKeyword => "protected",
        SyntaxKind::StaticKeyword => "static",
        SyntaxKind::ReadOnlyKeyword => "readonly",
        SyntaxKind::SealedKeyword => "sealed",
        SyntaxKind::ConstKeyword => "const",
        SyntaxKind::FixedKeyword => "fixed",
        SyntaxKind::StackAllocKeyword => "stackalloc",
        SyntaxKind::VolatileKeyword => "volatile",
        SyntaxKind::NewKeyword => "new",
        SyntaxKind::OverrideKeyword => "override",
        SyntaxKind::AbstractKeyword => "abstract",
        SyntaxKind::VirtualKeyword => "virtual",
        SyntaxKind::EventKeyword => "event",
        SyntaxKind::ExternKeyword => "extern",
        SyntaxKind::RefKeyword => "ref",
        SyntaxKind::OutKeyword => "out",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::IsKeyword => "is",
        SyntaxKind::AsKeyword => "as",
        SyntaxKind::ParamsKeyword => "params",
        SyntaxKind::ArgListKeyword => "__arglist",
        SyntaxKind::MakeRefKeyword => "__makeref",
        SyntaxKind::RefTypeKeyword => "__reftype",
        SyntaxKind::RefValueKeyword => "__refvalue",
        SyntaxKind::ThisKeyword => "this",
        SyntaxKind::BaseKeyword => "base",
        SyntaxKind::NamespaceKeyword => "namespace",
        SyntaxKind::UsingKeyword => "using",
        SyntaxKind::ClassKeyword => "class",
        SyntaxKind::StructKeyword => "struct",
        SyntaxKind::InterfaceKeyword => "interface",
        SyntaxKind::EnumKeyword => "enum",
        SyntaxKind::DelegateKeyword => "delegate",
        SyntaxKind::CheckedKeyword => "checked",
        SyntaxKind::UncheckedKeyword => "unchecked",
        SyntaxKind::UnsafeKeyword => "unsafe",
        SyntaxKind::OperatorKeyword => "operator",
        SyntaxKind::ImplicitKeyword => "implicit",
        SyntaxKind::ExplicitKeyword => "explicit",
        SyntaxKind::ElifKeyword => "elif",
        SyntaxKind::EndIfKeyword => "endif",
        SyntaxKind::RegionKeyword => "region",
        SyntaxKind::EndRegionKeyword => "endregion",
        SyntaxKind::DefineKeyword => "define",
        SyntaxKind::UndefKeyword => "undef",
        SyntaxKind::WarningKeyword => "warning",
        SyntaxKind::ErrorKeyword => "error",
        SyntaxKind::LineKeyword => "line",
        SyntaxKind::PragmaKeyword => "pragma",
        SyntaxKind::HiddenKeyword => "hidden",
        SyntaxKind::ChecksumKeyword => "checksum",
        SyntaxKind::DisableKeyword => "disable",
        SyntaxKind::RestoreKeyword => "restore",
        SyntaxKind::ReferenceKeyword => "r",
        SyntaxKind::LoadKeyword => "load",
        SyntaxKind::NullableKeyword => "nullable",
        SyntaxKind::EnableKeyword => "enable",
        SyntaxKind::WarningsKeyword => "warnings",
        SyntaxKind::AnnotationsKeyword => "annotations",
        SyntaxKind::YieldKeyword => "yield",
        SyntaxKind::PartialKeyword => "partial",
        SyntaxKind::FromKeyword => "from",
        SyntaxKind::GroupKeyword => "group",
        SyntaxKind::JoinKeyword => "join",
        SyntaxKind::IntoKeyword => "into",
        SyntaxKind::LetKeyword => "let",
        SyntaxKind::ByKeyword => "by",
        SyntaxKind::WhereKeyword => "where",
        SyntaxKind::SelectKeyword => "select",
        SyntaxKind::GetKeyword => "get",
        SyntaxKind::SetKeyword => "set",
        SyntaxKind::AddKeyword => "add",
        SyntaxKind::RemoveKeyword => "remove",
        SyntaxKind::OrderByKeyword => "orderby",
        SyntaxKind::AliasKeyword => "alias",
        SyntaxKind::OnKeyword => "on",
        SyntaxKind::EqualsKeyword => "equals",
        SyntaxKind::AscendingKeyword => "ascending",
        SyntaxKind::DescendingKeyword => "descending",
        SyntaxKind::AssemblyKeyword => "assembly",
        SyntaxKind::ModuleKeyword => "module",
        SyntaxKind::TypeKeyword => "type",
        SyntaxKind::FieldKeyword => "field",
        SyntaxKind::MethodKeyword => "method",
        SyntaxKind::ParamKeyword => "param",
        SyntaxKind::PropertyKeyword => "property",
        SyntaxKind::TypeVarKeyword => "typevar",
        SyntaxKind::GlobalKeyword => "global",
        SyntaxKind::NameOfKeyword => "nameof",
        SyntaxKind::AsyncKeyword => "async",
        SyntaxKind::AwaitKeyword => "await",
        SyntaxKind::WhenKeyword => "when",
        SyntaxKind::InterpolatedStringStartToken => "$\"",
        SyntaxKind::InterpolatedStringEndToken => "\"",
        SyntaxKind::InterpolatedVerbatimStringStartToken => "$@\"",
        SyntaxKind::UnderscoreToken => "_",
        SyntaxKind::VarKeyword => "var",
        SyntaxKind::AndKeyword => "and",
        SyntaxKind::OrKeyword => "or",
        SyntaxKind::NotKeyword => "not",
        SyntaxKind::WithKeyword => "with",
        SyntaxKind::InitKeyword => "init",
        SyntaxKind::RecordKeyword => "record",
        SyntaxKind::ManagedKeyword => "managed",
        SyntaxKind::UnmanagedKeyword => "unmanaged",
        SyntaxKind::RequiredKeyword => "required",
        SyntaxKind::ScopedKeyword => "scoped",
        SyntaxKind::FileKeyword => "file",
        SyntaxKind::AllowsKeyword => "allows",
        SyntaxKind::ExtensionKeyword => "extension",
        SyntaxKind::UnionKeyword => "union",
        SyntaxKind::ClosedKeyword => "closed",
        SyntaxKind::SafeKeyword => "safe",
        _ => "",
    }
}

/// `SyntaxFacts.IsTypeParameterVarianceKeyword` (SyntaxKindFacts.cs:1903).
pub fn is_type_parameter_variance_keyword(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::OutKeyword || kind == SyntaxKind::InKeyword
}

/// `SyntaxFacts.IsDocumentationCommentTrivia` (SyntaxKindFacts.cs:1908).
pub fn is_documentation_comment_trivia(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::SingleLineDocumentationCommentTrivia || kind == SyntaxKind::MultiLineDocumentationCommentTrivia
}
