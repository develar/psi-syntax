//! Every token type of the Java lexer (`JavaSyntaxTokenType`) and of the JavaDoc lexer (`JavaDocSyntaxTokenType`), the
//! platform's `WHITE_SPACE` and `BAD_CHARACTER` (`SyntaxTokenTypes`), and the lazily parsed element types of
//! `JavaDocSyntaxElementType` that live in the token stream (the two doc comments the Java lexer emits, the two holders
//! JavaDocParser remaps or collapses tokens into), with the debug names the dumps print; the token sets of
//! `JavaDocSyntaxTokenTypes`.
//!
//! In the Java code every one of these is a `SyntaxElementType` like the composite types of `node_kind`; here a token
//! type is a `TokenKind`, a composite type a `NodeKind`, and `ElementType` is either.

use psi_builder::ElementTypeSet;

use crate::node_kind::NodeKind;

/// What PSI makes of a leaf of the token type (`JavaASTFactory`), and whether it is lazily parsed: the flags of
/// `TokenKind::flags`.
const JAVA: u8 = 0; // PsiJavaToken:TYPE
const SPACE: u8 = 1; // PsiWhiteSpace (WHITE_SPACE, and DOC_SPACE inside a doc comment)
const COMMENT: u8 = 2; // PsiComment(TYPE); `SyntaxElementTypes.JAVA_COMMENT_BIT_SET`
const KEYWORD: u8 = 4; // PsiKeyword:text; `SyntaxElementTypes.KEYWORD_BIT_SET`
const IDENT: u8 = 8; // PsiIdentifier:text
const DOC: u8 = 16; // PsiDocToken:TYPE
const FRAGMENT: u8 = 32; // PsiFragment:TYPE (string template fragments)
const LAZY: u8 = 64; // a lazily parsed element type: the tree expands a leaf of it into a composite
const DOC_OTHER: u8 = 128; // a JavaDoc token PSI prints as PsiElement(TYPE): DOC_TAG_FORMAT_STRING

macro_rules! tokens {
    ($($(#[$doc:meta])* $name:ident = $id:literal, $debug:literal, $flags:expr;)*) => {
        /// A token type.
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[repr(u8)]
        pub enum TokenKind {
            $($(#[$doc])* $name = $id,)*
        }

        impl TokenKind {
            /// Every token type, in discriminant order.
            pub const ALL: &'static [TokenKind] = &[$(TokenKind::$name,)*];

            /// `SyntaxElementType.toString()`: the debug name the dumps print.
            pub const fn debug_name(self) -> &'static str {
                match self { $(TokenKind::$name => $debug,)* }
            }

            const fn flags(self) -> u8 {
                match self { $(TokenKind::$name => $flags,)* }
            }
        }
    };
}

tokens! {
    // SyntaxTokenTypes (the platform).
    WHITE_SPACE = 0, "WHITE_SPACE", SPACE;
    BAD_CHARACTER = 1, "BAD_CHARACTER", 0;
    // JavaSyntaxTokenType.
    IDENTIFIER = 2, "IDENTIFIER", IDENT;
    C_STYLE_COMMENT = 3, "C_STYLE_COMMENT", COMMENT;
    END_OF_LINE_COMMENT = 4, "END_OF_LINE_COMMENT", COMMENT;
    INTEGER_LITERAL = 5, "INTEGER_LITERAL", JAVA;
    LONG_LITERAL = 6, "LONG_LITERAL", JAVA;
    FLOAT_LITERAL = 7, "FLOAT_LITERAL", JAVA;
    DOUBLE_LITERAL = 8, "DOUBLE_LITERAL", JAVA;
    CHARACTER_LITERAL = 9, "CHARACTER_LITERAL", JAVA;
    STRING_LITERAL = 10, "STRING_LITERAL", JAVA;
    TEXT_BLOCK_LITERAL = 11, "TEXT_BLOCK_LITERAL", JAVA;
    STRING_TEMPLATE_BEGIN = 12, "STRING_TEMPLATE_BEGIN", FRAGMENT;
    STRING_TEMPLATE_MID = 13, "STRING_TEMPLATE_MID", FRAGMENT;
    STRING_TEMPLATE_END = 14, "STRING_TEMPLATE_END", FRAGMENT;
    TEXT_BLOCK_TEMPLATE_BEGIN = 15, "TEXT_BLOCK_TEMPLATE_BEGIN", FRAGMENT;
    TEXT_BLOCK_TEMPLATE_MID = 16, "TEXT_BLOCK_TEMPLATE_MID", FRAGMENT;
    TEXT_BLOCK_TEMPLATE_END = 17, "TEXT_BLOCK_TEMPLATE_END", FRAGMENT;
    TRUE_KEYWORD = 18, "TRUE_KEYWORD", JAVA;
    FALSE_KEYWORD = 19, "FALSE_KEYWORD", JAVA;
    NULL_KEYWORD = 20, "NULL_KEYWORD", JAVA;
    ABSTRACT_KEYWORD = 21, "ABSTRACT_KEYWORD", KEYWORD;
    ASSERT_KEYWORD = 22, "ASSERT_KEYWORD", KEYWORD;
    BOOLEAN_KEYWORD = 23, "BOOLEAN_KEYWORD", KEYWORD;
    BREAK_KEYWORD = 24, "BREAK_KEYWORD", KEYWORD;
    BYTE_KEYWORD = 25, "BYTE_KEYWORD", KEYWORD;
    CASE_KEYWORD = 26, "CASE_KEYWORD", KEYWORD;
    CATCH_KEYWORD = 27, "CATCH_KEYWORD", KEYWORD;
    CHAR_KEYWORD = 28, "CHAR_KEYWORD", KEYWORD;
    CLASS_KEYWORD = 29, "CLASS_KEYWORD", KEYWORD;
    CONST_KEYWORD = 30, "CONST_KEYWORD", KEYWORD;
    CONTINUE_KEYWORD = 31, "CONTINUE_KEYWORD", KEYWORD;
    DEFAULT_KEYWORD = 32, "DEFAULT_KEYWORD", KEYWORD;
    DO_KEYWORD = 33, "DO_KEYWORD", KEYWORD;
    DOUBLE_KEYWORD = 34, "DOUBLE_KEYWORD", KEYWORD;
    ELSE_KEYWORD = 35, "ELSE_KEYWORD", KEYWORD;
    ENUM_KEYWORD = 36, "ENUM_KEYWORD", KEYWORD;
    EXTENDS_KEYWORD = 37, "EXTENDS_KEYWORD", KEYWORD;
    FINAL_KEYWORD = 38, "FINAL_KEYWORD", KEYWORD;
    FINALLY_KEYWORD = 39, "FINALLY_KEYWORD", KEYWORD;
    FLOAT_KEYWORD = 40, "FLOAT_KEYWORD", KEYWORD;
    FOR_KEYWORD = 41, "FOR_KEYWORD", KEYWORD;
    GOTO_KEYWORD = 42, "GOTO_KEYWORD", KEYWORD;
    IF_KEYWORD = 43, "IF_KEYWORD", KEYWORD;
    IMPLEMENTS_KEYWORD = 44, "IMPLEMENTS_KEYWORD", KEYWORD;
    IMPORT_KEYWORD = 45, "IMPORT_KEYWORD", KEYWORD;
    INSTANCEOF_KEYWORD = 46, "INSTANCEOF_KEYWORD", KEYWORD;
    INT_KEYWORD = 47, "INT_KEYWORD", KEYWORD;
    INTERFACE_KEYWORD = 48, "INTERFACE_KEYWORD", KEYWORD;
    LONG_KEYWORD = 49, "LONG_KEYWORD", KEYWORD;
    NATIVE_KEYWORD = 50, "NATIVE_KEYWORD", KEYWORD;
    NEW_KEYWORD = 51, "NEW_KEYWORD", KEYWORD;
    PACKAGE_KEYWORD = 52, "PACKAGE_KEYWORD", KEYWORD;
    PRIVATE_KEYWORD = 53, "PRIVATE_KEYWORD", KEYWORD;
    PUBLIC_KEYWORD = 54, "PUBLIC_KEYWORD", KEYWORD;
    SHORT_KEYWORD = 55, "SHORT_KEYWORD", KEYWORD;
    SUPER_KEYWORD = 56, "SUPER_KEYWORD", KEYWORD;
    SWITCH_KEYWORD = 57, "SWITCH_KEYWORD", KEYWORD;
    SYNCHRONIZED_KEYWORD = 58, "SYNCHRONIZED_KEYWORD", KEYWORD;
    THIS_KEYWORD = 59, "THIS_KEYWORD", KEYWORD;
    THROW_KEYWORD = 60, "THROW_KEYWORD", KEYWORD;
    PROTECTED_KEYWORD = 61, "PROTECTED_KEYWORD", KEYWORD;
    TRANSIENT_KEYWORD = 62, "TRANSIENT_KEYWORD", KEYWORD;
    RETURN_KEYWORD = 63, "RETURN_KEYWORD", KEYWORD;
    VOID_KEYWORD = 64, "VOID_KEYWORD", KEYWORD;
    STATIC_KEYWORD = 65, "STATIC_KEYWORD", KEYWORD;
    STRICTFP_KEYWORD = 66, "STRICTFP_KEYWORD", KEYWORD;
    WHILE_KEYWORD = 67, "WHILE_KEYWORD", KEYWORD;
    TRY_KEYWORD = 68, "TRY_KEYWORD", KEYWORD;
    VOLATILE_KEYWORD = 69, "VOLATILE_KEYWORD", KEYWORD;
    THROWS_KEYWORD = 70, "THROWS_KEYWORD", KEYWORD;
    LPARENTH = 71, "LPARENTH", JAVA;
    RPARENTH = 72, "RPARENTH", JAVA;
    LBRACE = 73, "LBRACE", JAVA;
    RBRACE = 74, "RBRACE", JAVA;
    LBRACKET = 75, "LBRACKET", JAVA;
    RBRACKET = 76, "RBRACKET", JAVA;
    SEMICOLON = 77, "SEMICOLON", JAVA;
    COMMA = 78, "COMMA", JAVA;
    DOT = 79, "DOT", JAVA;
    ELLIPSIS = 80, "ELLIPSIS", JAVA;
    AT = 81, "AT", JAVA;
    EQ = 82, "EQ", JAVA;
    GT = 83, "GT", JAVA;
    LT = 84, "LT", JAVA;
    EXCL = 85, "EXCL", JAVA;
    TILDE = 86, "TILDE", JAVA;
    QUEST = 87, "QUEST", JAVA;
    COLON = 88, "COLON", JAVA;
    PLUS = 89, "PLUS", JAVA;
    MINUS = 90, "MINUS", JAVA;
    ASTERISK = 91, "ASTERISK", JAVA;
    DIV = 92, "DIV", JAVA;
    AND = 93, "AND", JAVA;
    OR = 94, "OR", JAVA;
    XOR = 95, "XOR", JAVA;
    PERC = 96, "PERC", JAVA;
    EQEQ = 97, "EQEQ", JAVA;
    LE = 98, "LE", JAVA;
    GE = 99, "GE", JAVA;
    NE = 100, "NE", JAVA;
    ANDAND = 101, "ANDAND", JAVA;
    OROR = 102, "OROR", JAVA;
    PLUSPLUS = 103, "PLUSPLUS", JAVA;
    MINUSMINUS = 104, "MINUSMINUS", JAVA;
    LTLT = 105, "LTLT", JAVA;
    GTGT = 106, "GTGT", JAVA;
    GTGTGT = 107, "GTGTGT", JAVA;
    PLUSEQ = 108, "PLUSEQ", JAVA;
    MINUSEQ = 109, "MINUSEQ", JAVA;
    ASTERISKEQ = 110, "ASTERISKEQ", JAVA;
    DIVEQ = 111, "DIVEQ", JAVA;
    ANDEQ = 112, "ANDEQ", JAVA;
    OREQ = 113, "OREQ", JAVA;
    XOREQ = 114, "XOREQ", JAVA;
    PERCEQ = 115, "PERCEQ", JAVA;
    LTLTEQ = 116, "LTLTEQ", JAVA;
    GTGTEQ = 117, "GTGTEQ", JAVA;
    GTGTGTEQ = 118, "GTGTGTEQ", JAVA;
    DOUBLE_COLON = 119, "DOUBLE_COLON", JAVA;
    ARROW = 120, "ARROW", JAVA;
    OPEN_KEYWORD = 121, "OPEN", KEYWORD;
    MODULE_KEYWORD = 122, "MODULE", KEYWORD;
    REQUIRES_KEYWORD = 123, "REQUIRES", KEYWORD;
    EXPORTS_KEYWORD = 124, "EXPORTS", KEYWORD;
    OPENS_KEYWORD = 125, "OPENS", KEYWORD;
    USES_KEYWORD = 126, "USES", KEYWORD;
    PROVIDES_KEYWORD = 127, "PROVIDES", KEYWORD;
    TRANSITIVE_KEYWORD = 128, "TRANSITIVE", KEYWORD;
    TO_KEYWORD = 129, "TO", KEYWORD;
    WITH_KEYWORD = 130, "WITH", KEYWORD;
    VAR_KEYWORD = 131, "VAR", KEYWORD;
    YIELD_KEYWORD = 132, "YIELD", KEYWORD;
    RECORD_KEYWORD = 133, "RECORD", KEYWORD;
    VALUE_KEYWORD = 134, "VALUE_KEYWORD", KEYWORD;
    SEALED_KEYWORD = 135, "SEALED", KEYWORD;
    NON_SEALED_KEYWORD = 136, "NON_SEALED", KEYWORD;
    PERMITS_KEYWORD = 137, "PERMITS", KEYWORD;
    WHEN_KEYWORD = 138, "WHEN_KEYWORD", KEYWORD;
    // JavaDocSyntaxTokenType.
    DOC_COMMENT_START = 139, "DOC_COMMENT_START", DOC;
    DOC_COMMENT_END = 140, "DOC_COMMENT_END", DOC;
    DOC_COMMENT_DATA = 141, "DOC_COMMENT_DATA", DOC;
    DOC_SPACE = 142, "DOC_SPACE", DOC | SPACE;
    DOC_COMMENT_LEADING_ASTERISKS = 143, "DOC_COMMENT_LEADING_ASTERISKS", DOC;
    DOC_TAG_NAME = 144, "DOC_TAG_NAME", DOC;
    DOC_INLINE_TAG_START = 145, "DOC_INLINE_TAG_START", DOC;
    DOC_INLINE_TAG_END = 146, "DOC_INLINE_TAG_END", DOC;
    DOC_TAG_VALUE_TOKEN = 147, "DOC_TAG_VALUE_TOKEN", DOC;
    DOC_TAG_VALUE_DOT = 148, "DOC_TAG_VALUE_DOT", DOC;
    DOC_TAG_VALUE_COMMA = 149, "DOC_TAG_VALUE_COMMA", DOC;
    DOC_TAG_VALUE_LPAREN = 150, "DOC_TAG_VALUE_LPAREN", DOC;
    DOC_TAG_VALUE_RPAREN = 151, "DOC_TAG_VALUE_RPAREN", DOC;
    DOC_TAG_VALUE_QUOTE = 152, "DOC_TAG_VALUE_QUOTE", DOC;
    DOC_TAG_VALUE_COLON = 153, "DOC_TAG_VALUE_COLON", DOC;
    DOC_TAG_ATTRIBUTE_NAME = 154, "DOC_TAG_ATTRIBUTE_NAME", DOC;
    DOC_TAG_ATTRIBUTE_VALUE = 155, "DOC_TAG_ATTRIBUTE_VALUE", DOC;
    DOC_TAG_VALUE_LT = 156, "DOC_TAG_VALUE_LT", DOC;
    DOC_TAG_VALUE_GT = 157, "DOC_TAG_VALUE_GT", DOC;
    DOC_TAG_VALUE_SLASH = 158, "DOC_TAG_VALUE_SLASH", DOC;
    DOC_TAG_VALUE_SHARP_TOKEN = 159, "DOC_TAG_VALUE_SHARP_TOKEN", DOC;
    DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN = 160, "DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN", DOC;
    DOC_TAG_FORMAT_STRING = 161, "DOC_TAG_FORMAT_STRING", DOC_OTHER;
    DOC_CODE_FENCE = 162, "DOC_CODE_FENCE", DOC;
    DOC_RBRACKET = 163, "DOC_RBRACKET", DOC;
    DOC_LBRACKET = 164, "DOC_LBRACKET", DOC;
    DOC_LPAREN = 165, "DOC_LPAREN", DOC;
    DOC_RPAREN = 166, "DOC_RPAREN", DOC;
    DOC_SHARP = 167, "DOC_SHARP", DOC;
    DOC_DOUBLE_SHARP = 168, "DOC_DOUBLE_SHARP", DOC;
    DOC_INLINE_CODE_FENCE = 169, "DOC_INLINE_CODE_FENCE", DOC;
    DOC_COMMA = 170, "DOC_COMMA", DOC;
    DOC_LT = 171, "DOC_LT", DOC;
    DOC_GT = 172, "DOC_GT", DOC;
    DOC_COMMENT_BAD_CHARACTER = 173, "DOC_COMMENT_BAD_CHARACTER", DOC;
    // JavaDocSyntaxElementType's lazily parsed element types that are tokens: the Java lexer emits the two comments,
    // JavaDocParser remaps or collapses tokens into the two holders.
    DOC_COMMENT = 174, "DOC_COMMENT", LAZY | COMMENT;
    DOC_MARKDOWN_COMMENT = 175, "DOC_MARKDOWN_COMMENT", LAZY | COMMENT;
    DOC_REFERENCE_HOLDER = 176, "DOC_REFERENCE_HOLDER", LAZY;
    DOC_TYPE_HOLDER = 177, "DOC_TYPE_HOLDER", LAZY;
}

impl TokenKind {
    /// `JavaSyntaxDefinition.whitespaces` or `.comments`: what the builder skips (`WHITE_SPACE`, `END_OF_LINE_COMMENT`,
    /// `C_STYLE_COMMENT`, `DOC_COMMENT`, `DOC_MARKDOWN_COMMENT`).
    #[inline(always)]
    pub const fn is_whitespace_or_comment(self) -> bool {
        matches!(
            self,
            TokenKind::WHITE_SPACE
                | TokenKind::END_OF_LINE_COMMENT
                | TokenKind::C_STYLE_COMMENT
                | TokenKind::DOC_COMMENT
                | TokenKind::DOC_MARKDOWN_COMMENT
        )
    }

    /// A lazily parsed element type in the token stream: the tree expands a leaf (or a collapsed marker) of it.
    pub const fn is_lazy(self) -> bool {
        self.flags() & LAZY != 0
    }

    /// PSI prints a leaf of it as `PsiWhiteSpace` (`WHITE_SPACE`, `DOC_SPACE`).
    pub const fn is_psi_white_space(self) -> bool {
        self.flags() & SPACE != 0
    }

    /// PSI prints a leaf of it as `PsiComment(TYPE)`.
    pub const fn is_psi_comment(self) -> bool {
        self.flags() & COMMENT != 0 && self.flags() & LAZY == 0
    }

    /// `SyntaxElementTypes.KEYWORD_BIT_SET`: PSI prints a leaf of it as `PsiKeyword:<text>`.
    pub const fn is_keyword(self) -> bool {
        self.flags() & KEYWORD != 0
    }

    /// PSI prints a leaf of it as `PsiIdentifier:<text>`.
    pub const fn is_identifier(self) -> bool {
        self.flags() & IDENT != 0
    }

    /// A JavaDoc token PSI prints as `PsiDocToken:TYPE`. (`DOC_TAG_VALUE_LT` and `DOC_TAG_VALUE_GT` are, by the
    /// goldens, though `JavaDocSyntaxTokenTypes.allJavaDocTypes` lacks them; `DOC_LT`, `DOC_GT` and
    /// `DOC_COMMENT_BAD_CHARACTER` are assumed to be, no golden has a leaf of them.)
    pub const fn is_psi_doc_token(self) -> bool {
        self.flags() & DOC != 0
    }

    /// A string template fragment, which PSI prints as `PsiFragment:TYPE`.
    pub const fn is_fragment(self) -> bool {
        self.flags() & FRAGMENT != 0
    }

    /// A token PSI prints as `PsiJavaToken:TYPE`.
    pub const fn is_psi_java_token(self) -> bool {
        self.flags() == JAVA && !matches!(self, TokenKind::BAD_CHARACTER)
    }

    /// A JavaDoc token PSI prints as `PsiElement(TYPE)` (`DOC_TAG_FORMAT_STRING`).
    pub const fn is_psi_doc_element(self) -> bool {
        self.flags() & DOC_OTHER != 0
    }
}

/// An element type: what `SyntaxElementType` is in the Kotlin code, a token type or a composite node type.
pub type ElementType = psi_builder::ElementType<TokenKind, NodeKind>;

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
    fn from(t: TokenKind) -> Self {
        ElementType::Token(t)
    }
}

impl From<NodeKind> for ElementType {
    fn from(n: NodeKind) -> Self {
        ElementType::Node(n)
    }
}

impl PartialEq<TokenKind> for ElementType {
    fn eq(&self, other: &TokenKind) -> bool {
        *self == ElementType::Token(*other)
    }
}

impl PartialEq<NodeKind> for ElementType {
    fn eq(&self, other: &NodeKind) -> bool {
        *self == ElementType::Node(*other)
    }
}

/// Lets the ported code compare a possibly-null token type with a token as Kotlin does: `builder.token_type() == LBRACE`.
impl PartialEq<TokenKind> for Option<TokenKind> {
    fn eq(&self, other: &TokenKind) -> bool {
        *self == Some(*other)
    }
}

impl PartialEq<TokenKind> for Option<ElementType> {
    fn eq(&self, other: &TokenKind) -> bool {
        *self == Some(ElementType::Token(*other))
    }
}

impl PartialEq<NodeKind> for Option<ElementType> {
    fn eq(&self, other: &NodeKind) -> bool {
        *self == Some(ElementType::Node(*other))
    }
}

/// `SyntaxElementTypeSet`: a set of element types (tokens and node types), built at compile time.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct TokenSet {
    bits: ElementTypeSet,
}

/// Anything `TokenSet::contains` accepts: a token, a node type, an element type, or a possibly-null one (null is in no
/// set, as in Kotlin).
pub trait TokenSetMember {
    fn element_type(self) -> Option<ElementType>;
}

impl TokenSetMember for TokenKind {
    fn element_type(self) -> Option<ElementType> {
        Some(ElementType::Token(self))
    }
}

impl TokenSetMember for NodeKind {
    fn element_type(self) -> Option<ElementType> {
        Some(ElementType::Node(self))
    }
}

impl TokenSetMember for ElementType {
    fn element_type(self) -> Option<ElementType> {
        Some(self)
    }
}

impl TokenSetMember for Option<TokenKind> {
    fn element_type(self) -> Option<ElementType> {
        self.map(ElementType::Token)
    }
}

impl TokenSetMember for Option<ElementType> {
    fn element_type(self) -> Option<ElementType> {
        self
    }
}

impl TokenSet {
    /// An empty set.
    pub const EMPTY: TokenSet = TokenSet { bits: ElementTypeSet::EMPTY };

    /// `syntaxElementTypeSetOf(tokens...)`.
    pub const fn create(tokens: &[TokenKind]) -> TokenSet {
        let mut bits = ElementTypeSet::EMPTY;
        let mut i = 0;
        while i < tokens.len() {
            bits = bits.with(tokens[i] as usize);
            i += 1;
        }
        TokenSet { bits }
    }

    /// `syntaxElementTypeSetOf(nodeTypes...)`.
    pub const fn create_nodes(nodes: &[NodeKind]) -> TokenSet {
        let mut bits = ElementTypeSet::EMPTY;
        let mut i = 0;
        while i < nodes.len() {
            bits = bits.with(<TokenKind as psi_builder::TokenType>::NODE_BASE + nodes[i] as usize);
            i += 1;
        }
        TokenSet { bits }
    }

    /// `a + b + ...` (`SyntaxElementTypeSet.plus`).
    pub const fn or_set(sets: &[TokenSet]) -> TokenSet {
        let mut bits = ElementTypeSet::EMPTY;
        let mut i = 0;
        while i < sets.len() {
            bits = ElementTypeSet::or_set(&[bits, sets[i].bits]);
            i += 1;
        }
        TokenSet { bits }
    }

    /// The intersection.
    pub const fn and_set(a: TokenSet, b: TokenSet) -> TokenSet {
        TokenSet { bits: ElementTypeSet::and_set(a.bits, b.bits) }
    }

    /// `a - b`.
    pub const fn and_not(a: TokenSet, b: TokenSet) -> TokenSet {
        TokenSet { bits: ElementTypeSet::and_not(a.bits, b.bits) }
    }

    /// `t in set`; a null (`None`) element type is in no set.
    #[inline]
    pub fn contains(&self, t: impl TokenSetMember) -> bool {
        match t.element_type() {
            Some(e) => self.bits.contains_index(e.index()),
            None => false,
        }
    }

    /// The tokens of this set in discriminant order.
    pub fn iter(&self) -> impl Iterator<Item = TokenKind> + '_ {
        TokenKind::ALL.iter().copied().filter(move |t| self.contains(*t))
    }

    /// The node types of this set.
    pub fn iter_nodes(&self) -> impl Iterator<Item = NodeKind> + '_ {
        NodeKind::ALL.iter().copied().filter(move |n| self.contains(*n))
    }
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set()
            .entries(self.iter().map(|t| t.debug_name()))
            .entries(self.iter_nodes().map(|n| n.debug_name()))
            .finish()
    }
}

use TokenKind::*;

/// `JavaSyntaxTokenType.JAVA_TOKEN_TYPE_SET`: every token of `JavaSyntaxTokenType`.
pub const JAVA_TOKEN_TYPE_SET: TokenSet = {
    let mut bits = ElementTypeSet::EMPTY;
    let mut i = IDENTIFIER as usize;
    while i <= WHEN_KEYWORD as usize {
        bits = bits.with(i);
        i += 1;
    }
    TokenSet { bits }
};

/// `JavaDocSyntaxTokenTypes.spaceCommentsTokenSet`: what `JavaDocLexer` merges into one token when adjacent.
pub const SPACE_COMMENTS_TOKEN_SET: TokenSet = TokenSet::create(&[DOC_SPACE, DOC_COMMENT_DATA]);

/// `JavaDocSyntaxTokenTypes.allJavaDocTypes`.
pub const ALL_JAVA_DOC_TYPES: TokenSet = TokenSet::create(&[
    DOC_COMMENT_START,
    DOC_COMMENT_END,
    DOC_COMMENT_DATA,
    DOC_SPACE,
    DOC_COMMENT_LEADING_ASTERISKS,
    DOC_TAG_NAME,
    DOC_INLINE_TAG_START,
    DOC_INLINE_TAG_END,
    DOC_TAG_VALUE_TOKEN,
    DOC_TAG_VALUE_DOT,
    DOC_TAG_VALUE_COMMA,
    DOC_TAG_VALUE_LPAREN,
    DOC_TAG_VALUE_RPAREN,
    DOC_TAG_VALUE_SHARP_TOKEN,
    DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN,
    DOC_TAG_VALUE_SLASH,
    DOC_TAG_VALUE_QUOTE,
    DOC_TAG_VALUE_COLON,
    DOC_TAG_ATTRIBUTE_NAME,
    DOC_TAG_ATTRIBUTE_VALUE,
    DOC_CODE_FENCE,
    DOC_RBRACKET,
    DOC_LBRACKET,
    DOC_LPAREN,
    DOC_RPAREN,
    DOC_SHARP,
    DOC_DOUBLE_SHARP,
    DOC_INLINE_CODE_FENCE,
    DOC_COMMA,
]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_are_dense_and_names_unique() {
        for (i, t) in TokenKind::ALL.iter().enumerate() {
            assert_eq!(*t as usize, i);
        }
        let mut names: Vec<&str> = TokenKind::ALL.iter().map(|t| t.debug_name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
        assert!(TokenKind::ALL.len() < 256);
        assert_eq!(JAVA_TOKEN_TYPE_SET.iter().count(), WHEN_KEYWORD as usize - IDENTIFIER as usize + 1);
    }
}
