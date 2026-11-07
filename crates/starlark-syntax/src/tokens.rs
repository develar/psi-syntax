//! The token types of Bazel's `StarlarkTokenTypes`, including its indentation tokens.

use crate::node_kind::NodeKind;
use psi_builder::ElementTypeSet;

pub type ElementType = psi_builder::ElementType<TokenKind, NodeKind>;

macro_rules! tokens {
    ($($name:ident = $debug:literal;)*) => {
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[repr(u8)]
        pub enum TokenKind {
            $($name,)*
        }

        impl TokenKind {
            pub const ALL: &'static [TokenKind] = &[$(TokenKind::$name,)*];

            pub const fn debug_name(self) -> &'static str {
                match self { $(TokenKind::$name => $debug,)* }
            }
        }
    };
}

tokens! {
    SPACE = "SPACE";
    TAB = "TAB";
    LINE_CONTINUATION = "LINE_CONTINUATION";
    LINE_BREAK = "LINE_BREAK";
    INDENT = "INDENT";
    DEDENT = "DEDENT";
    INCONSISTENT_DEDENT = "INCONSISTENT_DEDENT";
    STATEMENT_BREAK = "STATEMENT_BREAK";
    COMMENT = "COMMENT";
    STRING = "STRING";
    BYTES = "BYTES";
    INT = "INT";
    FLOAT = "FLOAT";
    AND_KEYWORD = "and";
    BREAK_KEYWORD = "break";
    CONTINUE_KEYWORD = "continue";
    DEF_KEYWORD = "def";
    ELIF_KEYWORD = "elif";
    ELSE_KEYWORD = "else";
    FOR_KEYWORD = "for";
    IF_KEYWORD = "if";
    IN_KEYWORD = "in";
    LAMBDA_KEYWORD = "lambda";
    LOAD_KEYWORD = "load";
    NOT_KEYWORD = "not";
    OR_KEYWORD = "or";
    PASS_KEYWORD = "pass";
    RETURN_KEYWORD = "return";
    NONE_KEYWORD = "None";
    TRUE_KEYWORD = "True";
    FALSE_KEYWORD = "False";
    AS_KEYWORD = "as";
    ASSERT_KEYWORD = "assert";
    ASYNC_KEYWORD = "async";
    AWAIT_KEYWORD = "await";
    CLASS_KEYWORD = "class";
    DEL_KEYWORD = "del";
    EXCEPT_KEYWORD = "except";
    FINALLY_KEYWORD = "finally";
    FROM_KEYWORD = "from";
    GLOBAL_KEYWORD = "global";
    IMPORT_KEYWORD = "import";
    IS_KEYWORD = "is";
    NONLOCAL_KEYWORD = "nonlocal";
    RAISE_KEYWORD = "raise";
    TRY_KEYWORD = "try";
    WHILE_KEYWORD = "while";
    WITH_KEYWORD = "with";
    YIELD_KEYWORD = "yield";
    IDENTIFIER = "IDENTIFIER";
    PLUS = "+";
    MINUS = "-";
    MULT = "*";
    DIV = "/";
    FLOORDIV = "//";
    PERC = "%";
    EXP = "**";
    TILDE = "~";
    AND = "&";
    OR = "|";
    XOR = "^";
    LTLT = "<<";
    GTGT = ">>";
    DOT = ".";
    COMMA = ",";
    EQ = "=";
    SEMICOLON = ";";
    COLON = ":";
    LPAR = "(";
    RPAR = ")";
    LBRACKET = "[";
    RBRACKET = "]";
    LBRACE = "{";
    RBRACE = "}";
    LT = "<";
    GT = ">";
    LE = "<=";
    GE = ">=";
    EQEQ = "==";
    NE = "!=";
    PLUSEQ = "+=";
    MINUSEQ = "-=";
    MULTEQ = "*=";
    DIVEQ = "/=";
    FLOORDIVEQ = "//=";
    PERCEQ = "%=";
    ANDEQ = "&=";
    OREQ = "|=";
    XOREQ = "^=";
    LTLTEQ = "<<=";
    GTGTEQ = ">>=";
    BAD_CHARACTER = "BAD_CHARACTER";
}

impl TokenKind {
    pub const fn is_whitespace(self) -> bool {
        matches!(self, TokenKind::SPACE | TokenKind::TAB | TokenKind::LINE_BREAK | TokenKind::LINE_CONTINUATION)
    }

    pub const fn is_comment(self) -> bool {
        matches!(self, TokenKind::COMMENT)
    }

    pub const fn is_keyword(self) -> bool {
        matches!(
            self,
            TokenKind::AND_KEYWORD
                | TokenKind::BREAK_KEYWORD
                | TokenKind::CONTINUE_KEYWORD
                | TokenKind::DEF_KEYWORD
                | TokenKind::ELIF_KEYWORD
                | TokenKind::ELSE_KEYWORD
                | TokenKind::FOR_KEYWORD
                | TokenKind::IF_KEYWORD
                | TokenKind::IN_KEYWORD
                | TokenKind::LAMBDA_KEYWORD
                | TokenKind::LOAD_KEYWORD
                | TokenKind::NOT_KEYWORD
                | TokenKind::OR_KEYWORD
                | TokenKind::PASS_KEYWORD
                | TokenKind::RETURN_KEYWORD
                | TokenKind::NONE_KEYWORD
                | TokenKind::TRUE_KEYWORD
                | TokenKind::FALSE_KEYWORD
                | TokenKind::AS_KEYWORD
                | TokenKind::ASSERT_KEYWORD
                | TokenKind::ASYNC_KEYWORD
                | TokenKind::AWAIT_KEYWORD
                | TokenKind::CLASS_KEYWORD
                | TokenKind::DEL_KEYWORD
                | TokenKind::EXCEPT_KEYWORD
                | TokenKind::FINALLY_KEYWORD
                | TokenKind::FROM_KEYWORD
                | TokenKind::GLOBAL_KEYWORD
                | TokenKind::IMPORT_KEYWORD
                | TokenKind::IS_KEYWORD
                | TokenKind::NONLOCAL_KEYWORD
                | TokenKind::RAISE_KEYWORD
                | TokenKind::TRY_KEYWORD
                | TokenKind::WHILE_KEYWORD
                | TokenKind::WITH_KEYWORD
                | TokenKind::YIELD_KEYWORD
        )
    }
}

impl psi_builder::TokenType for TokenKind {
    #[inline(always)]
    fn index(self) -> u16 {
        self as u16
    }

    #[inline(always)]
    fn from_index(index: usize) -> Option<TokenKind> {
        TokenKind::ALL.get(index).copied()
    }

    #[inline]
    fn debug_name(self) -> &'static str {
        TokenKind::debug_name(self)
    }
}

impl From<TokenKind> for ElementType {
    fn from(kind: TokenKind) -> ElementType {
        ElementType::Token(kind)
    }
}

impl From<NodeKind> for ElementType {
    fn from(kind: NodeKind) -> ElementType {
        ElementType::Node(kind)
    }
}

/// A set of Starlark token types, corresponding to IntelliJ's `TokenSet`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TokenSet {
    bits: ElementTypeSet,
}

impl TokenSet {
    pub const EMPTY: TokenSet = TokenSet { bits: ElementTypeSet::EMPTY };

    pub const fn with(self, kind: TokenKind) -> TokenSet {
        TokenSet { bits: self.bits.with(kind as usize) }
    }

    pub const fn contains(self, kind: TokenKind) -> bool {
        self.bits.contains_index(kind as usize)
    }

    pub const fn contains_opt(self, kind: Option<TokenKind>) -> bool {
        match kind {
            Some(kind) => self.contains(kind),
            None => false,
        }
    }
}
