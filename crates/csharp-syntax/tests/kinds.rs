//! The generated tables against each other and against facts: every token and node kind maps to its `SyntaxKind` and
//! back, every node kind has its node type, keywords spell their kinds, and the diagnostics carry Roslyn's numbers.

use csharp_syntax::facts;
use csharp_syntax::syntax::{NODE_TYPES, SlotKind, node_type};
use csharp_syntax::{ErrorCode, LanguageVersion, MessageID, NodeKind, Severity, SyntaxKind, TokenKind};

#[test]
fn token_and_node_kinds_map_both_ways() {
    for i in 0..TokenKind::COUNT {
        let t = TokenKind::from_index(i).unwrap();
        assert_eq!(t.syntax_kind().token_kind(), Some(t));
        assert_eq!(t.syntax_kind().node_kind(), None);
        assert_eq!(SyntaxKind::from_raw(t.syntax_kind().raw()), Some(t.syntax_kind()));
        assert_eq!(t.syntax_kind().name(), t.debug_name());
    }
    for i in 0..NodeKind::COUNT {
        let n = NodeKind::from_index(i).unwrap();
        assert_eq!(n.syntax_kind().node_kind(), Some(n));
        assert_eq!(n.syntax_kind().token_kind(), None);
        assert!(node_type(n).kinds.contains(&n.syntax_kind()), "{n:?}");
    }
    assert_eq!(TokenKind::from_index(TokenKind::COUNT), None);
    assert_eq!(SyntaxKind::List.name(), "List");
    assert_eq!(SyntaxKind::None.token_kind(), None);
    assert_eq!(SyntaxKind::IdentifierToken.token_kind(), Some(TokenKind::IdentifierToken));
    assert_eq!(SyntaxKind::WhitespaceTrivia.token_kind(), Some(TokenKind::WhitespaceTrivia));
    assert_eq!(SyntaxKind::IfDirectiveTrivia.node_kind(), Some(NodeKind::IfDirectiveTrivia));
    assert_eq!(SyntaxKind::CompilationUnit.raw(), 8840);
    const { assert!(TokenKind::COUNT + NodeKind::COUNT < 4096) };
}

#[test]
fn every_syntax_kind_is_a_token_a_node_or_none() {
    let mut kinds = 0;
    for raw in 0..=u16::MAX {
        let Some(k) = SyntaxKind::from_raw(raw) else { continue };
        kinds += 1;
        let classes = k.token_kind().is_some() as u8 + k.node_kind().is_some() as u8;
        if matches!(k, SyntaxKind::None | SyntaxKind::List) {
            assert_eq!(classes, 0);
        } else {
            assert_eq!(classes, 1, "{k:?}");
            // a token kind is `IsAnyToken` or an unstructured trivia (`PreprocessingMessageTrivia` is no `IsTrivia`)
            if k.token_kind().is_some() && !facts::is_any_token(k) {
                assert!(facts::is_trivia(k) || k == SyntaxKind::PreprocessingMessageTrivia, "{k:?}");
            }
        }
    }
    assert_eq!(kinds, TokenKind::COUNT + NodeKind::COUNT + 2);
}

#[test]
fn node_types_describe_their_slots() {
    let class = node_type(NodeKind::ClassDeclaration);
    assert_eq!(class.name, "ClassDeclarationSyntax");
    let names: Vec<_> = class.slots.iter().map(|s| s.name).collect();
    assert_eq!(names[..4], ["AttributeLists", "Modifiers", "Keyword", "Identifier"]);
    assert_eq!(class.slots[0].kind, SlotKind::List);
    assert_eq!(class.slots[2].kind, SlotKind::Token);
    let if_directive = node_type(NodeKind::IfDirectiveTrivia);
    assert_eq!(if_directive.bools, ["IsActive", "BranchTaken", "ConditionValue"]);
    assert!(if_directive.is_directive && if_directive.is_structured_trivia);
    assert!(node_type(NodeKind::SkippedTokensTrivia).is_structured_trivia);
    assert!(!class.is_structured_trivia);
    let binary = node_type(NodeKind::AddExpression);
    assert!(binary.kinds.contains(&SyntaxKind::SubtractExpression));
    assert!(NODE_TYPES.iter().all(|t| t.slots.len() <= 15));
}

#[test]
fn keywords_spell_their_kinds() {
    let mut reserved = 0;
    for k in facts::get_reserved_keyword_kinds() {
        assert_eq!(facts::get_keyword_kind(facts::get_text(k).as_bytes()), k, "{k:?}");
        reserved += 1;
    }
    assert_eq!(reserved, 81);
    for k in facts::get_contextual_keyword_kinds() {
        assert!(facts::is_contextual_keyword(k), "{k:?}");
        assert_eq!(facts::get_contextual_keyword_kind(facts::get_text(k).as_bytes()), k, "{k:?}");
    }
    assert_eq!(facts::get_text(SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken), ">>>=");
    assert_eq!(facts::get_text(SyntaxKind::IdentifierToken), "");
    assert_eq!(facts::get_binary_expression(SyntaxKind::PlusToken), SyntaxKind::AddExpression);
    assert!(facts::is_reserved_keyword(SyntaxKind::ClassKeyword) && !facts::is_reserved_keyword(SyntaxKind::VarKeyword));
}

#[test]
fn diagnostics_carry_roslyn_numbers_and_messages() {
    assert_eq!(ErrorCode::ERR_SyntaxError.number(), 1003);
    assert_eq!(ErrorCode::ERR_SyntaxError.message_format(), "Syntax error, '{0}' expected");
    assert_eq!(ErrorCode::ERR_SemicolonExpected.number(), 1002);
    assert_eq!(ErrorCode::WRN_ErrorOverride.severity(), Severity::Warning);
    assert_eq!(ErrorCode::ERR_InsufficientStack.severity(), Severity::Error);
    assert_eq!(MessageID::IDS_FeatureRecords.message(), "records");
    assert_eq!(MessageID::IDS_FeatureRecords.required_version(), Some(LanguageVersion::CSharp9));
    assert!(LanguageVersion::LATEST.is_feature_enabled(MessageID::IDS_FeatureRecords));
    assert!(!LanguageVersion::CSharp8.is_feature_enabled(MessageID::IDS_FeatureRecords));
    assert_eq!(LanguageVersion::from_name("Latest"), Some(LanguageVersion::CSharp14));
    assert_eq!(LanguageVersion::from_name("7.3").map(|v| v.to_display_string()), Some("7.3"));
    assert_eq!(LanguageVersion::CSharp14.get_error_code(), Some(ErrorCode::ERR_FeatureNotAvailableInVersion14));
}
