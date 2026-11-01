//! `SyntaxElementTypes`: the element type sets the parsers and the binders share (comments, keywords, literals,
//! operators, modifiers, expressions, statements, members).

use crate::node_kind::NodeKind;
use crate::tokens::{TokenKind, TokenSet};

/// `SyntaxElementTypes.JAVA_PLAIN_COMMENT_BIT_SET`.
pub const JAVA_PLAIN_COMMENT_BIT_SET: TokenSet =
    TokenSet::create(&[TokenKind::END_OF_LINE_COMMENT, TokenKind::C_STYLE_COMMENT]);

/// `SyntaxElementTypes.JAVA_COMMENT_BIT_SET`.
pub const JAVA_COMMENT_BIT_SET: TokenSet = TokenSet::or_set(&[
    JAVA_PLAIN_COMMENT_BIT_SET,
    TokenSet::create(&[TokenKind::DOC_COMMENT, TokenKind::DOC_MARKDOWN_COMMENT]),
]);

/// `SyntaxElementTypes.JAVA_COMMENT_OR_WHITESPACE_BIT_SET`.
pub const JAVA_COMMENT_OR_WHITESPACE_BIT_SET: TokenSet =
    TokenSet::or_set(&[JAVA_COMMENT_BIT_SET, TokenSet::create(&[TokenKind::WHITE_SPACE])]);

/// `SyntaxElementTypes.KEYWORD_BIT_SET`.
pub const KEYWORD_BIT_SET: TokenSet = TokenSet::create(&[
    TokenKind::ABSTRACT_KEYWORD,
    TokenKind::ASSERT_KEYWORD,
    TokenKind::BOOLEAN_KEYWORD,
    TokenKind::BREAK_KEYWORD,
    TokenKind::BYTE_KEYWORD,
    TokenKind::CASE_KEYWORD,
    TokenKind::CATCH_KEYWORD,
    TokenKind::CHAR_KEYWORD,
    TokenKind::CLASS_KEYWORD,
    TokenKind::CONST_KEYWORD,
    TokenKind::CONTINUE_KEYWORD,
    TokenKind::DEFAULT_KEYWORD,
    TokenKind::DO_KEYWORD,
    TokenKind::DOUBLE_KEYWORD,
    TokenKind::ELSE_KEYWORD,
    TokenKind::ENUM_KEYWORD,
    TokenKind::EXTENDS_KEYWORD,
    TokenKind::FINAL_KEYWORD,
    TokenKind::FINALLY_KEYWORD,
    TokenKind::FLOAT_KEYWORD,
    TokenKind::FOR_KEYWORD,
    TokenKind::GOTO_KEYWORD,
    TokenKind::IF_KEYWORD,
    TokenKind::IMPLEMENTS_KEYWORD,
    TokenKind::IMPORT_KEYWORD,
    TokenKind::INSTANCEOF_KEYWORD,
    TokenKind::INT_KEYWORD,
    TokenKind::INTERFACE_KEYWORD,
    TokenKind::LONG_KEYWORD,
    TokenKind::NATIVE_KEYWORD,
    TokenKind::NEW_KEYWORD,
    TokenKind::PACKAGE_KEYWORD,
    TokenKind::PRIVATE_KEYWORD,
    TokenKind::PROTECTED_KEYWORD,
    TokenKind::PUBLIC_KEYWORD,
    TokenKind::RETURN_KEYWORD,
    TokenKind::SHORT_KEYWORD,
    TokenKind::SUPER_KEYWORD,
    TokenKind::STATIC_KEYWORD,
    TokenKind::STRICTFP_KEYWORD,
    TokenKind::SWITCH_KEYWORD,
    TokenKind::SYNCHRONIZED_KEYWORD,
    TokenKind::THIS_KEYWORD,
    TokenKind::THROW_KEYWORD,
    TokenKind::THROWS_KEYWORD,
    TokenKind::TRANSIENT_KEYWORD,
    TokenKind::TRY_KEYWORD,
    TokenKind::VOID_KEYWORD,
    TokenKind::VOLATILE_KEYWORD,
    TokenKind::WHILE_KEYWORD,
    TokenKind::OPEN_KEYWORD,
    TokenKind::MODULE_KEYWORD,
    TokenKind::REQUIRES_KEYWORD,
    TokenKind::EXPORTS_KEYWORD,
    TokenKind::OPENS_KEYWORD,
    TokenKind::USES_KEYWORD,
    TokenKind::PROVIDES_KEYWORD,
    TokenKind::TRANSITIVE_KEYWORD,
    TokenKind::TO_KEYWORD,
    TokenKind::WITH_KEYWORD,
    TokenKind::VAR_KEYWORD,
    TokenKind::YIELD_KEYWORD,
    TokenKind::RECORD_KEYWORD,
    TokenKind::SEALED_KEYWORD,
    TokenKind::PERMITS_KEYWORD,
    TokenKind::NON_SEALED_KEYWORD,
    TokenKind::WHEN_KEYWORD,
    TokenKind::VALUE_KEYWORD,
]);

/// `SyntaxElementTypes.LITERAL_BIT_SET`.
pub const LITERAL_BIT_SET: TokenSet =
    TokenSet::create(&[TokenKind::TRUE_KEYWORD, TokenKind::FALSE_KEYWORD, TokenKind::NULL_KEYWORD]);

/// `SyntaxElementTypes.OPERATION_BIT_SET`.
pub const OPERATION_BIT_SET: TokenSet = TokenSet::create(&[
    TokenKind::EQ,
    TokenKind::GT,
    TokenKind::LT,
    TokenKind::EXCL,
    TokenKind::TILDE,
    TokenKind::QUEST,
    TokenKind::COLON,
    TokenKind::PLUS,
    TokenKind::MINUS,
    TokenKind::ASTERISK,
    TokenKind::DIV,
    TokenKind::AND,
    TokenKind::OR,
    TokenKind::XOR,
    TokenKind::PERC,
    TokenKind::EQEQ,
    TokenKind::LE,
    TokenKind::GE,
    TokenKind::NE,
    TokenKind::ANDAND,
    TokenKind::OROR,
    TokenKind::PLUSPLUS,
    TokenKind::MINUSMINUS,
    TokenKind::LTLT,
    TokenKind::GTGT,
    TokenKind::GTGTGT,
    TokenKind::PLUSEQ,
    TokenKind::MINUSEQ,
    TokenKind::ASTERISKEQ,
    TokenKind::DIVEQ,
    TokenKind::ANDEQ,
    TokenKind::OREQ,
    TokenKind::XOREQ,
    TokenKind::PERCEQ,
    TokenKind::LTLTEQ,
    TokenKind::GTGTEQ,
    TokenKind::GTGTGTEQ,
]);

/// `SyntaxElementTypes.MODIFIER_BIT_SET`.
pub const MODIFIER_BIT_SET: TokenSet = TokenSet::create(&[
    TokenKind::PUBLIC_KEYWORD,
    TokenKind::PROTECTED_KEYWORD,
    TokenKind::PRIVATE_KEYWORD,
    TokenKind::STATIC_KEYWORD,
    TokenKind::ABSTRACT_KEYWORD,
    TokenKind::FINAL_KEYWORD,
    TokenKind::NATIVE_KEYWORD,
    TokenKind::SYNCHRONIZED_KEYWORD,
    TokenKind::STRICTFP_KEYWORD,
    TokenKind::TRANSIENT_KEYWORD,
    TokenKind::VOLATILE_KEYWORD,
    TokenKind::DEFAULT_KEYWORD,
    TokenKind::SEALED_KEYWORD,
    TokenKind::NON_SEALED_KEYWORD,
    TokenKind::VALUE_KEYWORD,
]);

/// `SyntaxElementTypes.PRIMITIVE_TYPE_BIT_SET`.
pub const PRIMITIVE_TYPE_BIT_SET: TokenSet = TokenSet::create(&[
    TokenKind::BOOLEAN_KEYWORD,
    TokenKind::BYTE_KEYWORD,
    TokenKind::SHORT_KEYWORD,
    TokenKind::INT_KEYWORD,
    TokenKind::LONG_KEYWORD,
    TokenKind::CHAR_KEYWORD,
    TokenKind::FLOAT_KEYWORD,
    TokenKind::DOUBLE_KEYWORD,
    TokenKind::VOID_KEYWORD,
]);

/// `SyntaxElementTypes.EXPRESSION_BIT_SET`.
pub const EXPRESSION_BIT_SET: TokenSet = TokenSet::create_nodes(&[
    NodeKind::REFERENCE_EXPRESSION,
    NodeKind::LITERAL_EXPRESSION,
    NodeKind::THIS_EXPRESSION,
    NodeKind::SUPER_EXPRESSION,
    NodeKind::PARENTH_EXPRESSION,
    NodeKind::METHOD_CALL_EXPRESSION,
    NodeKind::TYPE_CAST_EXPRESSION,
    NodeKind::PREFIX_EXPRESSION,
    NodeKind::POSTFIX_EXPRESSION,
    NodeKind::BINARY_EXPRESSION,
    NodeKind::POLYADIC_EXPRESSION,
    NodeKind::CONDITIONAL_EXPRESSION,
    NodeKind::ASSIGNMENT_EXPRESSION,
    NodeKind::NEW_EXPRESSION,
    NodeKind::ARRAY_ACCESS_EXPRESSION,
    NodeKind::ARRAY_INITIALIZER_EXPRESSION,
    NodeKind::INSTANCE_OF_EXPRESSION,
    NodeKind::CLASS_OBJECT_ACCESS_EXPRESSION,
    NodeKind::METHOD_REF_EXPRESSION,
    NodeKind::LAMBDA_EXPRESSION,
    NodeKind::SWITCH_EXPRESSION,
    NodeKind::EMPTY_EXPRESSION,
    NodeKind::TEMPLATE_EXPRESSION,
]);

/// `SyntaxElementTypes.ANNOTATION_MEMBER_VALUE_BIT_SET`.
pub const ANNOTATION_MEMBER_VALUE_BIT_SET: TokenSet = TokenSet::or_set(&[
    EXPRESSION_BIT_SET,
    TokenSet::create_nodes(&[NodeKind::ANNOTATION, NodeKind::ANNOTATION_ARRAY_INITIALIZER]),
]);

/// `SyntaxElementTypes.ARRAY_DIMENSION_BIT_SET`.
pub const ARRAY_DIMENSION_BIT_SET: TokenSet = TokenSet::create_nodes(&[
    NodeKind::REFERENCE_EXPRESSION,
    NodeKind::LITERAL_EXPRESSION,
    NodeKind::THIS_EXPRESSION,
    NodeKind::SUPER_EXPRESSION,
    NodeKind::PARENTH_EXPRESSION,
    NodeKind::METHOD_CALL_EXPRESSION,
    NodeKind::TYPE_CAST_EXPRESSION,
    NodeKind::PREFIX_EXPRESSION,
    NodeKind::POSTFIX_EXPRESSION,
    NodeKind::BINARY_EXPRESSION,
    NodeKind::POLYADIC_EXPRESSION,
    NodeKind::CONDITIONAL_EXPRESSION,
    NodeKind::ASSIGNMENT_EXPRESSION,
    NodeKind::NEW_EXPRESSION,
    NodeKind::ARRAY_ACCESS_EXPRESSION,
    NodeKind::INSTANCE_OF_EXPRESSION,
    NodeKind::CLASS_OBJECT_ACCESS_EXPRESSION,
    NodeKind::EMPTY_EXPRESSION,
]);

/// `SyntaxElementTypes.JAVA_STATEMENT_BIT_SET`.
pub const JAVA_STATEMENT_BIT_SET: TokenSet = TokenSet::create_nodes(&[
    NodeKind::EMPTY_STATEMENT,
    NodeKind::BLOCK_STATEMENT,
    NodeKind::EXPRESSION_STATEMENT,
    NodeKind::EXPRESSION_LIST_STATEMENT,
    NodeKind::DECLARATION_STATEMENT,
    NodeKind::IF_STATEMENT,
    NodeKind::WHILE_STATEMENT,
    NodeKind::FOR_STATEMENT,
    NodeKind::FOREACH_STATEMENT,
    NodeKind::DO_WHILE_STATEMENT,
    NodeKind::SWITCH_STATEMENT,
    NodeKind::SWITCH_LABEL_STATEMENT,
    NodeKind::BREAK_STATEMENT,
    NodeKind::CONTINUE_STATEMENT,
    NodeKind::RETURN_STATEMENT,
    NodeKind::THROW_STATEMENT,
    NodeKind::SYNCHRONIZED_STATEMENT,
    NodeKind::TRY_STATEMENT,
    NodeKind::LABELED_STATEMENT,
    NodeKind::ASSERT_STATEMENT,
    NodeKind::YIELD_STATEMENT,
]);

/// `SyntaxElementTypes.JAVA_PATTERN_BIT_SET`.
pub const JAVA_PATTERN_BIT_SET: TokenSet =
    TokenSet::create_nodes(&[NodeKind::TYPE_TEST_PATTERN, NodeKind::DECONSTRUCTION_PATTERN, NodeKind::UNNAMED_PATTERN]);

/// `SyntaxElementTypes.JAVA_CASE_LABEL_ELEMENT_BIT_SET`.
pub const JAVA_CASE_LABEL_ELEMENT_BIT_SET: TokenSet = TokenSet::or_set(&[
    JAVA_PATTERN_BIT_SET,
    EXPRESSION_BIT_SET,
    TokenSet::create_nodes(&[NodeKind::DEFAULT_CASE_LABEL_ELEMENT]),
]);

/// `SyntaxElementTypes.JAVA_MODULE_STATEMENT_BIT_SET`.
pub const JAVA_MODULE_STATEMENT_BIT_SET: TokenSet = TokenSet::create_nodes(&[
    NodeKind::REQUIRES_STATEMENT,
    NodeKind::EXPORTS_STATEMENT,
    NodeKind::OPENS_STATEMENT,
    NodeKind::USES_STATEMENT,
    NodeKind::PROVIDES_STATEMENT,
]);

/// `SyntaxElementTypes.IMPORT_STATEMENT_BASE_BIT_SET`.
pub const IMPORT_STATEMENT_BASE_BIT_SET: TokenSet = TokenSet::create_nodes(&[
    NodeKind::IMPORT_STATEMENT,
    NodeKind::IMPORT_STATIC_STATEMENT,
    NodeKind::IMPORT_MODULE_STATEMENT,
]);

/// `SyntaxElementTypes.CLASS_KEYWORD_BIT_SET`.
pub const CLASS_KEYWORD_BIT_SET: TokenSet = TokenSet::create(&[
    TokenKind::CLASS_KEYWORD,
    TokenKind::INTERFACE_KEYWORD,
    TokenKind::ENUM_KEYWORD,
    TokenKind::RECORD_KEYWORD,
]);

/// `SyntaxElementTypes.MEMBER_BIT_SET`.
pub const MEMBER_BIT_SET: TokenSet = TokenSet::create_nodes(&[
    NodeKind::CLASS,
    NodeKind::FIELD,
    NodeKind::ENUM_CONSTANT,
    NodeKind::METHOD,
    NodeKind::ANNOTATION_METHOD,
]);

/// `SyntaxElementTypes.FULL_MEMBER_BIT_SET`.
pub const FULL_MEMBER_BIT_SET: TokenSet =
    TokenSet::or_set(&[MEMBER_BIT_SET, TokenSet::create_nodes(&[NodeKind::CLASS_INITIALIZER])]);

/// `SyntaxElementTypes.INTEGER_LITERALS`.
pub const INTEGER_LITERALS: TokenSet = TokenSet::create(&[TokenKind::INTEGER_LITERAL, TokenKind::LONG_LITERAL]);

/// `SyntaxElementTypes.REAL_LITERALS`.
pub const REAL_LITERALS: TokenSet = TokenSet::create(&[TokenKind::FLOAT_LITERAL, TokenKind::DOUBLE_LITERAL]);

/// `SyntaxElementTypes.STRING_LITERALS`.
pub const STRING_LITERALS: TokenSet = TokenSet::create(&[TokenKind::STRING_LITERAL, TokenKind::TEXT_BLOCK_LITERAL]);

/// `SyntaxElementTypes.TEXT_LITERALS`.
pub const TEXT_LITERALS: TokenSet =
    TokenSet::create(&[TokenKind::STRING_LITERAL, TokenKind::TEXT_BLOCK_LITERAL, TokenKind::CHARACTER_LITERAL]);

/// `SyntaxElementTypes.STRING_TEMPLATE_FRAGMENTS`.
pub const STRING_TEMPLATE_FRAGMENTS: TokenSet = TokenSet::create(&[
    TokenKind::STRING_TEMPLATE_BEGIN,
    TokenKind::STRING_TEMPLATE_MID,
    TokenKind::STRING_TEMPLATE_END,
    TokenKind::TEXT_BLOCK_TEMPLATE_BEGIN,
    TokenKind::TEXT_BLOCK_TEMPLATE_MID,
    TokenKind::TEXT_BLOCK_TEMPLATE_END,
]);

/// `SyntaxElementTypes.ALL_LITERALS`.
pub const ALL_LITERALS: TokenSet = TokenSet::or_set(&[INTEGER_LITERALS, REAL_LITERALS, TEXT_LITERALS, LITERAL_BIT_SET]);

/// `SyntaxElementTypes.SHIFT_OPS`.
pub const SHIFT_OPS: TokenSet = TokenSet::create(&[TokenKind::LTLT, TokenKind::GTGT, TokenKind::GTGTGT]);

/// `SyntaxElementTypes.ADDITIVE_OPS`.
pub const ADDITIVE_OPS: TokenSet = TokenSet::create(&[TokenKind::PLUS, TokenKind::MINUS]);

/// `SyntaxElementTypes.MULTIPLICATIVE_OPS`.
pub const MULTIPLICATIVE_OPS: TokenSet = TokenSet::create(&[TokenKind::ASTERISK, TokenKind::DIV, TokenKind::PERC]);

/// `SyntaxElementTypes.ASSIGNMENT_OPS`.
pub const ASSIGNMENT_OPS: TokenSet = TokenSet::create(&[
    TokenKind::EQ,
    TokenKind::ASTERISKEQ,
    TokenKind::DIVEQ,
    TokenKind::PERCEQ,
    TokenKind::PLUSEQ,
    TokenKind::MINUSEQ,
    TokenKind::LTLTEQ,
    TokenKind::GTGTEQ,
    TokenKind::GTGTGTEQ,
    TokenKind::ANDEQ,
    TokenKind::OREQ,
    TokenKind::XOREQ,
]);
