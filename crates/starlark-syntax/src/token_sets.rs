//! The token sets of IntelliJ's `StarlarkTokenSets`.

use crate::tokens::{TokenKind as T, TokenSet};

pub const WHITESPACE: TokenSet =
    TokenSet::EMPTY.with(T::SPACE).with(T::TAB).with(T::LINE_BREAK).with(T::LINE_CONTINUATION);
pub const IDENTIFIER: TokenSet = TokenSet::EMPTY.with(T::IDENTIFIER);
pub const COMMENT: TokenSet = TokenSet::EMPTY.with(T::COMMENT);
pub const STRINGS: TokenSet = TokenSet::EMPTY.with(T::STRING).with(T::BYTES);
pub const OPEN_BRACKETS: TokenSet = TokenSet::EMPTY.with(T::LBRACKET).with(T::LBRACE).with(T::LPAR);
pub const CLOSE_BRACKETS: TokenSet = TokenSet::EMPTY.with(T::RBRACKET).with(T::RBRACE).with(T::RPAR);
pub const RESERVED_KEYWORDS: TokenSet = TokenSet::EMPTY
    .with(T::AS_KEYWORD)
    .with(T::ASSERT_KEYWORD)
    .with(T::ASYNC_KEYWORD)
    .with(T::AWAIT_KEYWORD)
    .with(T::CLASS_KEYWORD)
    .with(T::DEL_KEYWORD)
    .with(T::EXCEPT_KEYWORD)
    .with(T::FINALLY_KEYWORD)
    .with(T::FROM_KEYWORD)
    .with(T::GLOBAL_KEYWORD)
    .with(T::IMPORT_KEYWORD)
    .with(T::IS_KEYWORD)
    .with(T::NONLOCAL_KEYWORD)
    .with(T::RAISE_KEYWORD)
    .with(T::TRY_KEYWORD)
    .with(T::WHILE_KEYWORD)
    .with(T::WITH_KEYWORD)
    .with(T::YIELD_KEYWORD);
pub const PRIMARY_EXPRESSION_STARTERS: TokenSet = TokenSet::EMPTY
    .with(T::IDENTIFIER)
    .with(T::INT)
    .with(T::FLOAT)
    .with(T::LPAR)
    .with(T::LBRACKET)
    .with(T::LBRACE)
    .with(T::STRING)
    .with(T::BYTES)
    .with(T::NONE_KEYWORD)
    .with(T::TRUE_KEYWORD)
    .with(T::FALSE_KEYWORD);
pub const ENDS_OF_STATEMENT: TokenSet = TokenSet::EMPTY.with(T::STATEMENT_BREAK).with(T::SEMICOLON);
pub const COMPOUND_ASSIGN_OPERATIONS: TokenSet = TokenSet::EMPTY
    .with(T::PLUSEQ)
    .with(T::MINUSEQ)
    .with(T::MULTEQ)
    .with(T::DIVEQ)
    .with(T::FLOORDIVEQ)
    .with(T::PERCEQ)
    .with(T::ANDEQ)
    .with(T::OREQ)
    .with(T::XOREQ)
    .with(T::LTLTEQ)
    .with(T::GTGTEQ);
pub const COMPARISON_OPERATIONS: TokenSet = TokenSet::EMPTY
    .with(T::LT)
    .with(T::GT)
    .with(T::EQEQ)
    .with(T::LE)
    .with(T::GE)
    .with(T::NE)
    .with(T::IN_KEYWORD)
    .with(T::NOT_KEYWORD);
pub const SHIFT_OPERATIONS: TokenSet = TokenSet::EMPTY.with(T::LTLT).with(T::GTGT);
pub const ADDITIVE_OPERATIONS: TokenSet = TokenSet::EMPTY.with(T::PLUS).with(T::MINUS);
pub const MULTIPLICATIVE_OPERATIONS: TokenSet =
    TokenSet::EMPTY.with(T::MULT).with(T::FLOORDIV).with(T::DIV).with(T::PERC);
pub const UNARY_OPERATIONS: TokenSet = TokenSet::EMPTY.with(T::PLUS).with(T::MINUS).with(T::TILDE).with(T::NOT_KEYWORD);
pub const BINARY_OPERATIONS: TokenSet = TokenSet::EMPTY
    .with(T::AND_KEYWORD)
    .with(T::AND)
    .with(T::OR_KEYWORD)
    .with(T::OR)
    .with(T::XOR)
    .with(T::EXP)
    .with(T::NOT_KEYWORD)
    .with(T::IN_KEYWORD)
    .with(T::IS_KEYWORD)
    .with(T::PLUS)
    .with(T::MINUS)
    .with(T::LT)
    .with(T::GT)
    .with(T::EQEQ)
    .with(T::LE)
    .with(T::GE)
    .with(T::NE)
    .with(T::MULT)
    .with(T::FLOORDIV)
    .with(T::DIV)
    .with(T::PERC)
    .with(T::LTLT)
    .with(T::GTGT);

pub const BRACKET_OR_COMMA: TokenSet = TokenSet::EMPTY.with(T::RBRACKET).with(T::COMMA);
pub const BRACKET_COLON_COMMA: TokenSet = BRACKET_OR_COMMA.with(T::COLON);
