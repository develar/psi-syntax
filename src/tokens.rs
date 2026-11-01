//! Every token type of the Kotlin lexer (`KtTokens`, `KtKeywordToken`, `KtModifierKeywordToken`,
//! `KtSingleValueToken`) and of the KDoc lexer (`KDocTokens`), plus the platform's `WHITE_SPACE` and `BAD_CHARACTER`,
//! with the debug names `DebugUtil.psiToString` prints (`PsiElement(fun)`, `PsiElement(IDENTIFIER)`), and the token
//! sets of `KtTokens` and `KtTokenSets`.
//!
//! The discriminant of a Kotlin token is its `KtTokens.*_Id`, so `TokenKind::LPAR as i32 == KtTokens.LPAR_Id`.

use crate::node_kind::NodeKind;

/// What a token type is in the Java class hierarchy: the flags of `TokenKind::flags`.
const KT: u8 = 1; // instanceof KtToken
const SINGLE: u8 = 2; // instanceof KtSingleValueToken
const KEYWORD: u8 = 4; // instanceof KtKeywordToken
const SOFT: u8 = 8; // KtKeywordToken.isSoft()
const MODIFIER: u8 = 16; // instanceof KtModifierKeywordToken
const LAZY: u8 = 32; // ILazyParseableElementType: the tree expands it into a composite (KDoc, KDOC_MARKDOWN_LINK)
const KDOC: u8 = 64; // a KDocToken (a KtToken subclass of the KDoc lexer)

const K_KW: u8 = KT | SINGLE | KEYWORD;
const K_SOFT: u8 = KT | SINGLE | KEYWORD | SOFT;
const K_MOD: u8 = KT | SINGLE | KEYWORD | MODIFIER;
const K_SOFT_MOD: u8 = KT | SINGLE | KEYWORD | SOFT | MODIFIER;
const K_SINGLE: u8 = KT | SINGLE;

macro_rules! tokens {
    ($($name:ident = $id:literal, $debug:literal, $flags:expr, $value:expr;)*) => {
        /// A token type. Kotlin tokens carry their `KtTokens` id as discriminant.
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[repr(u8)]
        pub enum TokenKind {
            $($name = $id,)*
        }

        impl TokenKind {
            /// Every token type, in discriminant order.
            pub const ALL: &'static [TokenKind] = &[$(TokenKind::$name,)*];

            /// `IElementType.toString()`: the name the PSI dump prints inside `PsiElement(...)`.
            pub const fn debug_name(self) -> &'static str {
                match self { $(TokenKind::$name => $debug,)* }
            }

            const fn flags(self) -> u8 {
                match self { $(TokenKind::$name => $flags,)* }
            }

            /// `KtSingleValueToken.getValue()`: the fixed text of a punctuation or keyword token.
            pub const fn value(self) -> Option<&'static str> {
                match self { $(TokenKind::$name => $value,)* }
            }
        }
    };
}

tokens! {
    EOF = 1, "EOF", KT, None;
    RESERVED = 2, "RESERVED", KT, None;
    BLOCK_COMMENT = 3, "BLOCK_COMMENT", KT, None;
    EOL_COMMENT = 4, "EOL_COMMENT", KT, None;
    SHEBANG_COMMENT = 5, "SHEBANG_COMMENT", KT, None;
    INTEGER_LITERAL = 6, "INTEGER_LITERAL", KT, None;
    FLOAT_LITERAL = 7, "FLOAT_CONSTANT", KT, None;
    CHARACTER_LITERAL = 8, "CHARACTER_LITERAL", KT, None;
    CLOSING_QUOTE = 9, "CLOSING_QUOTE", KT, None;
    OPEN_QUOTE = 10, "OPEN_QUOTE", KT, None;
    REGULAR_STRING_PART = 11, "REGULAR_STRING_PART", KT, None;
    ESCAPE_SEQUENCE = 12, "ESCAPE_SEQUENCE", KT, None;
    SHORT_TEMPLATE_ENTRY_START = 13, "SHORT_TEMPLATE_ENTRY_START", KT, None;
    LONG_TEMPLATE_ENTRY_START = 14, "LONG_TEMPLATE_ENTRY_START", KT, None;
    LONG_TEMPLATE_ENTRY_END = 15, "LONG_TEMPLATE_ENTRY_END", KT, None;
    DANGLING_NEWLINE = 16, "DANGLING_NEWLINE", KT, None;
    PACKAGE_KEYWORD = 17, "package", K_KW, Some("package");
    AS_KEYWORD = 18, "as", K_KW, Some("as");
    TYPE_ALIAS_KEYWORD = 19, "typealias", K_KW, Some("typealias");
    CLASS_KEYWORD = 20, "class", K_KW, Some("class");
    THIS_KEYWORD = 21, "this", K_KW, Some("this");
    SUPER_KEYWORD = 22, "super", K_KW, Some("super");
    VAL_KEYWORD = 23, "val", K_KW, Some("val");
    VAR_KEYWORD = 24, "var", K_KW, Some("var");
    FUN_KEYWORD = 25, "fun", K_MOD, Some("fun");
    FOR_KEYWORD = 26, "for", K_KW, Some("for");
    NULL_KEYWORD = 27, "null", K_KW, Some("null");
    TRUE_KEYWORD = 28, "true", K_KW, Some("true");
    FALSE_KEYWORD = 29, "false", K_KW, Some("false");
    IS_KEYWORD = 30, "is", K_KW, Some("is");
    IN_KEYWORD = 31, "in", K_MOD, Some("in");
    THROW_KEYWORD = 32, "throw", K_KW, Some("throw");
    RETURN_KEYWORD = 33, "return", K_KW, Some("return");
    BREAK_KEYWORD = 34, "break", K_KW, Some("break");
    CONTINUE_KEYWORD = 35, "continue", K_KW, Some("continue");
    OBJECT_KEYWORD = 36, "object", K_KW, Some("object");
    IF_KEYWORD = 37, "if", K_KW, Some("if");
    TRY_KEYWORD = 38, "try", K_KW, Some("try");
    ELSE_KEYWORD = 39, "else", K_KW, Some("else");
    WHILE_KEYWORD = 40, "while", K_KW, Some("while");
    DO_KEYWORD = 41, "do", K_KW, Some("do");
    WHEN_KEYWORD = 42, "when", K_KW, Some("when");
    INTERFACE_KEYWORD = 43, "interface", K_KW, Some("interface");
    TYPEOF_KEYWORD = 44, "typeof", K_KW, Some("typeof");
    AS_SAFE = 45, "AS_SAFE", K_KW, Some("as?");
    IDENTIFIER = 46, "IDENTIFIER", KT, None;
    FIELD_IDENTIFIER = 47, "FIELD_IDENTIFIER", KT, None;
    LBRACKET = 48, "LBRACKET", K_SINGLE, Some("[");
    RBRACKET = 49, "RBRACKET", K_SINGLE, Some("]");
    LBRACE = 50, "LBRACE", K_SINGLE, Some("{");
    RBRACE = 51, "RBRACE", K_SINGLE, Some("}");
    LPAR = 52, "LPAR", K_SINGLE, Some("(");
    RPAR = 53, "RPAR", K_SINGLE, Some(")");
    DOT = 54, "DOT", K_SINGLE, Some(".");
    PLUSPLUS = 55, "PLUSPLUS", K_SINGLE, Some("++");
    MINUSMINUS = 56, "MINUSMINUS", K_SINGLE, Some("--");
    MUL = 57, "MUL", K_SINGLE, Some("*");
    PLUS = 58, "PLUS", K_SINGLE, Some("+");
    MINUS = 59, "MINUS", K_SINGLE, Some("-");
    EXCL = 60, "EXCL", K_SINGLE, Some("!");
    DIV = 61, "DIV", K_SINGLE, Some("/");
    PERC = 62, "PERC", K_SINGLE, Some("%");
    LT = 63, "LT", K_SINGLE, Some("<");
    GT = 64, "GT", K_SINGLE, Some(">");
    LTEQ = 65, "LTEQ", K_SINGLE, Some("<=");
    GTEQ = 66, "GTEQ", K_SINGLE, Some(">=");
    EQEQEQ = 67, "EQEQEQ", K_SINGLE, Some("===");
    ARROW = 68, "ARROW", K_SINGLE, Some("->");
    DOUBLE_ARROW = 69, "DOUBLE_ARROW", K_SINGLE, Some("=>");
    EXCLEQEQEQ = 70, "EXCLEQEQEQ", K_SINGLE, Some("!==");
    EQEQ = 71, "EQEQ", K_SINGLE, Some("==");
    EXCLEQ = 72, "EXCLEQ", K_SINGLE, Some("!=");
    EXCLEXCL = 73, "EXCLEXCL", K_SINGLE, Some("!!");
    ANDAND = 74, "ANDAND", K_SINGLE, Some("&&");
    AND = 75, "AND", K_SINGLE, Some("&");
    OROR = 76, "OROR", K_SINGLE, Some("||");
    SAFE_ACCESS = 77, "SAFE_ACCESS", K_SINGLE, Some("?.");
    ELVIS = 78, "ELVIS", K_SINGLE, Some("?:");
    QUEST = 79, "QUEST", K_SINGLE, Some("?");
    COLONCOLON = 80, "COLONCOLON", K_SINGLE, Some("::");
    COLON = 81, "COLON", K_SINGLE, Some(":");
    SEMICOLON = 82, "SEMICOLON", K_SINGLE, Some(";");
    DOUBLE_SEMICOLON = 83, "DOUBLE_SEMICOLON", K_SINGLE, Some(";;");
    RANGE = 84, "RANGE", K_SINGLE, Some("..");
    RANGE_UNTIL = 85, "RANGE_UNTIL", K_SINGLE, Some("..<");
    EQ = 86, "EQ", K_SINGLE, Some("=");
    MULTEQ = 87, "MULTEQ", K_SINGLE, Some("*=");
    DIVEQ = 88, "DIVEQ", K_SINGLE, Some("/=");
    PERCEQ = 89, "PERCEQ", K_SINGLE, Some("%=");
    PLUSEQ = 90, "PLUSEQ", K_SINGLE, Some("+=");
    MINUSEQ = 91, "MINUSEQ", K_SINGLE, Some("-=");
    NOT_IN = 92, "NOT_IN", K_KW, Some("!in");
    NOT_IS = 93, "NOT_IS", K_KW, Some("!is");
    HASH = 94, "HASH", K_SINGLE, Some("#");
    AT = 95, "AT", K_SINGLE, Some("@");
    COMMA = 96, "COMMA", K_SINGLE, Some(",");
    EOL_OR_SEMICOLON = 97, "EOL_OR_SEMICOLON", KT, None;
    FILE_KEYWORD = 98, "file", K_SOFT, Some("file");
    FIELD_KEYWORD = 99, "field", K_SOFT, Some("field");
    PROPERTY_KEYWORD = 100, "property", K_SOFT, Some("property");
    RECEIVER_KEYWORD = 101, "receiver", K_SOFT, Some("receiver");
    PARAM_KEYWORD = 102, "param", K_SOFT, Some("param");
    SETPARAM_KEYWORD = 103, "setparam", K_SOFT, Some("setparam");
    DELEGATE_KEYWORD = 104, "delegate", K_SOFT, Some("delegate");
    IMPORT_KEYWORD = 105, "import", K_SOFT, Some("import");
    WHERE_KEYWORD = 106, "where", K_SOFT, Some("where");
    BY_KEYWORD = 107, "by", K_SOFT, Some("by");
    GET_KEYWORD = 108, "get", K_SOFT, Some("get");
    SET_KEYWORD = 109, "set", K_SOFT, Some("set");
    CONSTRUCTOR_KEYWORD = 110, "constructor", K_SOFT, Some("constructor");
    INIT_KEYWORD = 111, "init", K_SOFT, Some("init");
    CONTEXT_KEYWORD = 112, "context", K_SOFT, Some("context");
    ABSTRACT_KEYWORD = 113, "abstract", K_SOFT_MOD, Some("abstract");
    ENUM_KEYWORD = 114, "enum", K_SOFT_MOD, Some("enum");
    CONTRACT_KEYWORD = 115, "contract", K_SOFT_MOD, Some("contract");
    OPEN_KEYWORD = 116, "open", K_SOFT_MOD, Some("open");
    INNER_KEYWORD = 117, "inner", K_SOFT_MOD, Some("inner");
    OVERRIDE_KEYWORD = 118, "override", K_SOFT_MOD, Some("override");
    PRIVATE_KEYWORD = 119, "private", K_SOFT_MOD, Some("private");
    PUBLIC_KEYWORD = 120, "public", K_SOFT_MOD, Some("public");
    INTERNAL_KEYWORD = 121, "internal", K_SOFT_MOD, Some("internal");
    PROTECTED_KEYWORD = 122, "protected", K_SOFT_MOD, Some("protected");
    CATCH_KEYWORD = 123, "catch", K_SOFT, Some("catch");
    OUT_KEYWORD = 124, "out", K_SOFT_MOD, Some("out");
    VARARG_KEYWORD = 125, "vararg", K_SOFT_MOD, Some("vararg");
    REIFIED_KEYWORD = 126, "reified", K_SOFT_MOD, Some("reified");
    DYNAMIC_KEYWORD = 127, "dynamic", K_SOFT, Some("dynamic");
    COMPANION_KEYWORD = 128, "companion", K_SOFT_MOD, Some("companion");
    SEALED_KEYWORD = 129, "sealed", K_SOFT_MOD, Some("sealed");
    FINALLY_KEYWORD = 130, "finally", K_SOFT, Some("finally");
    FINAL_KEYWORD = 131, "final", K_SOFT_MOD, Some("final");
    LATEINIT_KEYWORD = 132, "lateinit", K_SOFT_MOD, Some("lateinit");
    DATA_KEYWORD = 133, "data", K_SOFT_MOD, Some("data");
    VALUE_KEYWORD = 134, "value", K_SOFT_MOD, Some("value");
    INLINE_KEYWORD = 135, "inline", K_SOFT_MOD, Some("inline");
    NOINLINE_KEYWORD = 136, "noinline", K_SOFT_MOD, Some("noinline");
    TAILREC_KEYWORD = 137, "tailrec", K_SOFT_MOD, Some("tailrec");
    EXTERNAL_KEYWORD = 138, "external", K_SOFT_MOD, Some("external");
    ANNOTATION_KEYWORD = 139, "annotation", K_SOFT_MOD, Some("annotation");
    CROSSINLINE_KEYWORD = 140, "crossinline", K_SOFT_MOD, Some("crossinline");
    OPERATOR_KEYWORD = 141, "operator", K_SOFT_MOD, Some("operator");
    INFIX_KEYWORD = 142, "infix", K_SOFT_MOD, Some("infix");
    CONST_KEYWORD = 143, "const", K_SOFT_MOD, Some("const");
    SUSPEND_KEYWORD = 144, "suspend", K_SOFT_MOD, Some("suspend");
    EXPECT_KEYWORD = 145, "expect", K_SOFT_MOD, Some("expect");
    ACTUAL_KEYWORD = 146, "actual", K_SOFT_MOD, Some("actual");
    INTERPOLATION_PREFIX = 147, "INTERPOLATION_PREFIX", KT, None;
    ALL_KEYWORD = 148, "all", K_SOFT, Some("all");

    // The platform's TokenType.WHITE_SPACE and TokenType.BAD_CHARACTER (not KtTokens).
    WHITE_SPACE = 160, "WHITE_SPACE", 0, None;
    BAD_CHARACTER = 161, "BAD_CHARACTER", 0, None;

    // KDocTokens.KDOC (= KtTokens.DOC_COMMENT): the Kotlin lexer's doc comment token, a lazily parsed "KDoc" element.
    DOC_COMMENT = 170, "KDoc", LAZY, None;

    // KDocTokens: what the KDoc lexer produces inside a doc comment.
    KDOC_START = 180, "KDOC_START", KT | KDOC, None;
    KDOC_END = 181, "KDOC_END", KT | KDOC, None;
    KDOC_LEADING_ASTERISK = 182, "KDOC_LEADING_ASTERISK", KT | KDOC, None;
    KDOC_TEXT = 183, "KDOC_TEXT", KT | KDOC, None;
    KDOC_CODE_BLOCK_TEXT = 184, "KDOC_CODE_BLOCK_TEXT", KT | KDOC, None;
    KDOC_TAG_NAME = 185, "KDOC_TAG_NAME", KT | KDOC, None;
    KDOC_MARKDOWN_ESCAPED_CHAR = 186, "KDOC_MARKDOWN_ESCAPED_CHAR", KT | KDOC, None;
    KDOC_MARKDOWN_INLINE_LINK = 187, "KDOC_MARKDOWN_INLINE_LINK", KT | KDOC, None;
    KDOC_LPAR = 188, "KDOC_LPAR", KT | KDOC, None;
    KDOC_RPAR = 189, "KDOC_RPAR", KT | KDOC, None;
    KDOC_CODE_SPAN_TEXT = 190, "KDOC_CODE_SPAN_TEXT", KT | KDOC, None;
    // KDocTokens.MARKDOWN_LINK: a lazily parsed "KDOC_MARKDOWN_LINK" element, reparsed with the Kotlin lexer.
    KDOC_MARKDOWN_LINK = 191, "KDOC_MARKDOWN_LINK", LAZY, None;
}

impl TokenKind {
    /// `instanceof KtToken` (Kotlin and KDoc tokens; not WHITE_SPACE, BAD_CHARACTER or the lazy KDoc types).
    pub const fn is_kt_token(self) -> bool {
        self.flags() & KT != 0
    }

    /// `instanceof KtSingleValueToken`.
    pub const fn is_single_value(self) -> bool {
        self.flags() & SINGLE != 0
    }

    /// `instanceof KtKeywordToken` (hard, soft and modifier keywords, plus `AS_SAFE`, `NOT_IN`, `NOT_IS`).
    pub const fn is_keyword(self) -> bool {
        self.flags() & KEYWORD != 0
    }

    /// `KtKeywordToken.isSoft()`.
    pub const fn is_soft_keyword(self) -> bool {
        self.flags() & SOFT != 0
    }

    /// `instanceof KtModifierKeywordToken`.
    pub const fn is_modifier_keyword(self) -> bool {
        self.flags() & MODIFIER != 0
    }

    /// An `ILazyParseableElementType` among the tokens: the tree expands it into a composite node.
    pub const fn is_lazy(self) -> bool {
        self.flags() & LAZY != 0
    }

    /// A KDoc lexer token.
    pub const fn is_kdoc(self) -> bool {
        self.flags() & KDOC != 0
    }

    /// `KtToken.tokenId` for Kotlin tokens, `KtTokens.INVALID_Id` (0) for everything else
    /// (what `AbstractKotlinParsing.getTokenId()` switches on).
    pub const fn token_id(self) -> i32 {
        let id = self as i32;
        if id < 160 { id } else { INVALID_ID }
    }

    /// `AbstractKotlinParsing.SOFT_KEYWORD_TEXTS.get(text)`: the soft keyword whose value is `text`.
    pub fn soft_keyword_by_text(text: &[u8]) -> Option<TokenKind> {
        crate::counters::add(crate::counters::Counter::SoftKeywordLookup, 1);
        // A byte `match` over `SOFT_KEYWORDS`' values (the Java `HashMap`); the test checks it against the set.
        Some(match text {
            b"file" => FILE_KEYWORD,
            b"import" => IMPORT_KEYWORD,
            b"where" => WHERE_KEYWORD,
            b"by" => BY_KEYWORD,
            b"get" => GET_KEYWORD,
            b"set" => SET_KEYWORD,
            b"abstract" => ABSTRACT_KEYWORD,
            b"enum" => ENUM_KEYWORD,
            b"contract" => CONTRACT_KEYWORD,
            b"open" => OPEN_KEYWORD,
            b"inner" => INNER_KEYWORD,
            b"override" => OVERRIDE_KEYWORD,
            b"private" => PRIVATE_KEYWORD,
            b"public" => PUBLIC_KEYWORD,
            b"internal" => INTERNAL_KEYWORD,
            b"protected" => PROTECTED_KEYWORD,
            b"catch" => CATCH_KEYWORD,
            b"finally" => FINALLY_KEYWORD,
            b"out" => OUT_KEYWORD,
            b"final" => FINAL_KEYWORD,
            b"vararg" => VARARG_KEYWORD,
            b"reified" => REIFIED_KEYWORD,
            b"dynamic" => DYNAMIC_KEYWORD,
            b"companion" => COMPANION_KEYWORD,
            b"constructor" => CONSTRUCTOR_KEYWORD,
            b"init" => INIT_KEYWORD,
            b"sealed" => SEALED_KEYWORD,
            b"field" => FIELD_KEYWORD,
            b"property" => PROPERTY_KEYWORD,
            b"receiver" => RECEIVER_KEYWORD,
            b"param" => PARAM_KEYWORD,
            b"setparam" => SETPARAM_KEYWORD,
            b"delegate" => DELEGATE_KEYWORD,
            b"all" => ALL_KEYWORD,
            b"lateinit" => LATEINIT_KEYWORD,
            b"data" => DATA_KEYWORD,
            b"inline" => INLINE_KEYWORD,
            b"noinline" => NOINLINE_KEYWORD,
            b"tailrec" => TAILREC_KEYWORD,
            b"external" => EXTERNAL_KEYWORD,
            b"annotation" => ANNOTATION_KEYWORD,
            b"crossinline" => CROSSINLINE_KEYWORD,
            b"const" => CONST_KEYWORD,
            b"operator" => OPERATOR_KEYWORD,
            b"infix" => INFIX_KEYWORD,
            b"suspend" => SUSPEND_KEYWORD,
            b"expect" => EXPECT_KEYWORD,
            b"actual" => ACTUAL_KEYWORD,
            b"value" => VALUE_KEYWORD,
            b"context" => CONTEXT_KEYWORD,
            _ => return None,
        })
    }

    /// `KotlinExpressionParsing.KEYWORD_TEXTS.get(text)`: the hard keyword whose `toString()` (debug name) is `text`.
    pub fn keyword_by_debug_name(text: &[u8]) -> Option<TokenKind> {
        // A byte `match` over `KEYWORDS`' debug names; the test checks it against the set.
        Some(match text {
            b"package" => PACKAGE_KEYWORD,
            b"as" => AS_KEYWORD,
            b"typealias" => TYPE_ALIAS_KEYWORD,
            b"class" => CLASS_KEYWORD,
            b"interface" => INTERFACE_KEYWORD,
            b"this" => THIS_KEYWORD,
            b"super" => SUPER_KEYWORD,
            b"val" => VAL_KEYWORD,
            b"var" => VAR_KEYWORD,
            b"fun" => FUN_KEYWORD,
            b"for" => FOR_KEYWORD,
            b"null" => NULL_KEYWORD,
            b"true" => TRUE_KEYWORD,
            b"false" => FALSE_KEYWORD,
            b"is" => IS_KEYWORD,
            b"in" => IN_KEYWORD,
            b"throw" => THROW_KEYWORD,
            b"return" => RETURN_KEYWORD,
            b"break" => BREAK_KEYWORD,
            b"continue" => CONTINUE_KEYWORD,
            b"object" => OBJECT_KEYWORD,
            b"if" => IF_KEYWORD,
            b"else" => ELSE_KEYWORD,
            b"while" => WHILE_KEYWORD,
            b"do" => DO_KEYWORD,
            b"try" => TRY_KEYWORD,
            b"when" => WHEN_KEYWORD,
            b"NOT_IN" => NOT_IN,
            b"NOT_IS" => NOT_IS,
            b"AS_SAFE" => AS_SAFE,
            b"typeof" => TYPEOF_KEYWORD,
            _ => return None,
        })
    }

    /// Whether this token is whitespace or a comment for the Kotlin `PsiBuilder` (`KtTokens.WHITESPACES` +
    /// `KtTokens.COMMENTS`).
    pub const fn is_whitespace_or_comment(self) -> bool {
        matches!(
            self,
            TokenKind::WHITE_SPACE
                | TokenKind::EOL_COMMENT
                | TokenKind::BLOCK_COMMENT
                | TokenKind::DOC_COMMENT
                | TokenKind::SHEBANG_COMMENT
        )
    }
}

/// `KtTokens.INVALID_Id`.
pub const INVALID_ID: i32 = 0;

/// An element type: what `IElementType` is in the Java code, a token type or a composite node type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ElementType {
    Token(TokenKind),
    Node(NodeKind),
}

impl ElementType {
    /// `IElementType.toString()`.
    pub const fn debug_name(self) -> &'static str {
        match self {
            ElementType::Token(t) => t.debug_name(),
            ElementType::Node(n) => n.debug_name(),
        }
    }

    pub const fn token(self) -> Option<TokenKind> {
        match self {
            ElementType::Token(t) => Some(t),
            ElementType::Node(_) => None,
        }
    }

    pub const fn node(self) -> Option<NodeKind> {
        match self {
            ElementType::Node(n) => Some(n),
            ElementType::Token(_) => None,
        }
    }

    /// A dense number: a token's discriminant (below 256), 256 + a node type's.
    pub const fn index(self) -> usize {
        match self {
            ElementType::Token(t) => t as usize,
            ElementType::Node(n) => 256 + n as usize,
        }
    }

    /// The element type of `index()` (`None` for a number that is none).
    #[inline]
    pub fn from_index(index: usize) -> Option<ElementType> {
        if index < 256 {
            TOKEN_BY_ID[index].map(ElementType::Token)
        } else {
            NodeKind::ALL.get(index - 256).map(|&n| ElementType::Node(n))
        }
    }
}

/// Each token type at its discriminant.
const TOKEN_BY_ID: [Option<TokenKind>; 256] = {
    let mut table = [None; 256];
    let mut i = 0;
    while i < TokenKind::ALL.len() {
        table[TokenKind::ALL[i] as usize] = Some(TokenKind::ALL[i]);
        i += 1;
    }
    table
};

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

/// Lets the ported code compare a possibly-null token type with a token as Java does: `tt() == LBRACE`.
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

/// `com.intellij.psi.tree.TokenSet`: a set of element types (tokens and node types), built at compile time.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct TokenSet {
    bits: [u64; 8],
}

/// Anything `TokenSet::contains` accepts: a token, a node type, an element type, or a possibly-null one (null is in no
/// set, as in Java).
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
    /// `TokenSet.EMPTY`.
    pub const EMPTY: TokenSet = TokenSet { bits: [0; 8] };

    /// `TokenSet.create(tokens...)`.
    pub const fn create(tokens: &[TokenKind]) -> TokenSet {
        let mut bits = [0u64; 8];
        let mut i = 0;
        while i < tokens.len() {
            let index = tokens[i] as usize;
            bits[index / 64] |= 1 << (index % 64);
            i += 1;
        }
        TokenSet { bits }
    }

    /// `TokenSet.create(nodeTypes...)`.
    pub const fn create_nodes(nodes: &[NodeKind]) -> TokenSet {
        let mut bits = [0u64; 8];
        let mut i = 0;
        while i < nodes.len() {
            let index = ElementType::Node(nodes[i]).index();
            bits[index / 64] |= 1 << (index % 64);
            i += 1;
        }
        TokenSet { bits }
    }

    /// `TokenSet.orSet(sets...)`.
    pub const fn or_set(sets: &[TokenSet]) -> TokenSet {
        let mut bits = [0u64; 8];
        let mut i = 0;
        while i < sets.len() {
            let mut w = 0;
            while w < 8 {
                bits[w] |= sets[i].bits[w];
                w += 1;
            }
            i += 1;
        }
        TokenSet { bits }
    }

    /// `TokenSet.andSet(a, b)`.
    pub const fn and_set(a: TokenSet, b: TokenSet) -> TokenSet {
        let mut bits = [0u64; 8];
        let mut w = 0;
        while w < 8 {
            bits[w] = a.bits[w] & b.bits[w];
            w += 1;
        }
        TokenSet { bits }
    }

    /// `TokenSet.andNot(a, b)`.
    pub const fn and_not(a: TokenSet, b: TokenSet) -> TokenSet {
        let mut bits = [0u64; 8];
        let mut w = 0;
        while w < 8 {
            bits[w] = a.bits[w] & !b.bits[w];
            w += 1;
        }
        TokenSet { bits }
    }

    /// `TokenSet.contains(t)`; a null (`None`) element type is in no set.
    #[inline]
    pub fn contains(&self, t: impl TokenSetMember) -> bool {
        match t.element_type() {
            Some(e) => {
                let index = e.index();
                self.bits[index / 64] & (1 << (index % 64)) != 0
            }
            None => false,
        }
    }

    /// The tokens of this set in discriminant order (`TokenSet.getTypes()` restricted to tokens).
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

/// `KtTokens.KEYWORDS`.
pub const KEYWORDS: TokenSet = TokenSet::create(&[
    PACKAGE_KEYWORD,
    AS_KEYWORD,
    TYPE_ALIAS_KEYWORD,
    CLASS_KEYWORD,
    INTERFACE_KEYWORD,
    THIS_KEYWORD,
    SUPER_KEYWORD,
    VAL_KEYWORD,
    VAR_KEYWORD,
    FUN_KEYWORD,
    FOR_KEYWORD,
    NULL_KEYWORD,
    TRUE_KEYWORD,
    FALSE_KEYWORD,
    IS_KEYWORD,
    IN_KEYWORD,
    THROW_KEYWORD,
    RETURN_KEYWORD,
    BREAK_KEYWORD,
    CONTINUE_KEYWORD,
    OBJECT_KEYWORD,
    IF_KEYWORD,
    ELSE_KEYWORD,
    WHILE_KEYWORD,
    DO_KEYWORD,
    TRY_KEYWORD,
    WHEN_KEYWORD,
    NOT_IN,
    NOT_IS,
    AS_SAFE,
    TYPEOF_KEYWORD,
]);

/// `KtTokens.SOFT_KEYWORDS`.
pub const SOFT_KEYWORDS: TokenSet = TokenSet::create(&[
    FILE_KEYWORD,
    IMPORT_KEYWORD,
    WHERE_KEYWORD,
    BY_KEYWORD,
    GET_KEYWORD,
    SET_KEYWORD,
    ABSTRACT_KEYWORD,
    ENUM_KEYWORD,
    CONTRACT_KEYWORD,
    OPEN_KEYWORD,
    INNER_KEYWORD,
    OVERRIDE_KEYWORD,
    PRIVATE_KEYWORD,
    PUBLIC_KEYWORD,
    INTERNAL_KEYWORD,
    PROTECTED_KEYWORD,
    CATCH_KEYWORD,
    FINALLY_KEYWORD,
    OUT_KEYWORD,
    FINAL_KEYWORD,
    VARARG_KEYWORD,
    REIFIED_KEYWORD,
    DYNAMIC_KEYWORD,
    COMPANION_KEYWORD,
    CONSTRUCTOR_KEYWORD,
    INIT_KEYWORD,
    SEALED_KEYWORD,
    FIELD_KEYWORD,
    PROPERTY_KEYWORD,
    RECEIVER_KEYWORD,
    PARAM_KEYWORD,
    SETPARAM_KEYWORD,
    DELEGATE_KEYWORD,
    ALL_KEYWORD,
    LATEINIT_KEYWORD,
    DATA_KEYWORD,
    INLINE_KEYWORD,
    NOINLINE_KEYWORD,
    TAILREC_KEYWORD,
    EXTERNAL_KEYWORD,
    ANNOTATION_KEYWORD,
    CROSSINLINE_KEYWORD,
    CONST_KEYWORD,
    OPERATOR_KEYWORD,
    INFIX_KEYWORD,
    SUSPEND_KEYWORD,
    EXPECT_KEYWORD,
    ACTUAL_KEYWORD,
    VALUE_KEYWORD,
    CONTEXT_KEYWORD,
]);

/// `KtTokens.MODIFIER_KEYWORDS_ARRAY`, the canonical modifier order.
pub const MODIFIER_KEYWORDS_ARRAY: [TokenKind; 32] = [
    PUBLIC_KEYWORD,
    PROTECTED_KEYWORD,
    PRIVATE_KEYWORD,
    INTERNAL_KEYWORD,
    EXPECT_KEYWORD,
    ACTUAL_KEYWORD,
    FINAL_KEYWORD,
    OPEN_KEYWORD,
    ABSTRACT_KEYWORD,
    SEALED_KEYWORD,
    CONST_KEYWORD,
    EXTERNAL_KEYWORD,
    OVERRIDE_KEYWORD,
    LATEINIT_KEYWORD,
    TAILREC_KEYWORD,
    VARARG_KEYWORD,
    SUSPEND_KEYWORD,
    INNER_KEYWORD,
    ENUM_KEYWORD,
    ANNOTATION_KEYWORD,
    FUN_KEYWORD,
    COMPANION_KEYWORD,
    INLINE_KEYWORD,
    VALUE_KEYWORD,
    INFIX_KEYWORD,
    OPERATOR_KEYWORD,
    DATA_KEYWORD,
    OUT_KEYWORD,
    IN_KEYWORD,
    REIFIED_KEYWORD,
    NOINLINE_KEYWORD,
    CROSSINLINE_KEYWORD,
];

/// `KtTokens.MODIFIER_KEYWORDS`.
pub const MODIFIER_KEYWORDS: TokenSet = TokenSet::create(&MODIFIER_KEYWORDS_ARRAY);
/// `KtTokens.TYPE_MODIFIER_KEYWORDS`.
pub const TYPE_MODIFIER_KEYWORDS: TokenSet = TokenSet::create(&[SUSPEND_KEYWORD]);
/// `KtTokens.TYPE_ARGUMENT_MODIFIER_KEYWORDS`.
pub const TYPE_ARGUMENT_MODIFIER_KEYWORDS: TokenSet = TokenSet::create(&[IN_KEYWORD, OUT_KEYWORD]);
/// `KtTokens.RESERVED_VALUE_PARAMETER_MODIFIER_KEYWORDS`.
pub const RESERVED_VALUE_PARAMETER_MODIFIER_KEYWORDS: TokenSet = TokenSet::create(&[OUT_KEYWORD, VARARG_KEYWORD]);
/// `KtTokens.VISIBILITY_MODIFIERS`.
pub const VISIBILITY_MODIFIERS: TokenSet =
    TokenSet::create(&[PRIVATE_KEYWORD, PUBLIC_KEYWORD, INTERNAL_KEYWORD, PROTECTED_KEYWORD]);
/// `KtTokens.MODALITY_MODIFIERS`.
pub const MODALITY_MODIFIERS: TokenSet =
    TokenSet::create(&[ABSTRACT_KEYWORD, FINAL_KEYWORD, SEALED_KEYWORD, OPEN_KEYWORD]);
/// `KtTokens.WHITESPACES`.
pub const WHITESPACES: TokenSet = TokenSet::create(&[WHITE_SPACE]);
/// `KtTokens.COMMENTS` (the doc comment is a comment for the parser).
pub const COMMENTS: TokenSet = TokenSet::create(&[EOL_COMMENT, BLOCK_COMMENT, DOC_COMMENT, SHEBANG_COMMENT]);
/// `KtTokens.WHITE_SPACE_OR_COMMENT_BIT_SET`.
pub const WHITE_SPACE_OR_COMMENT_BIT_SET: TokenSet = TokenSet::or_set(&[COMMENTS, WHITESPACES]);
/// `KtTokens.STRINGS`.
pub const STRINGS: TokenSet = TokenSet::create(&[CHARACTER_LITERAL, REGULAR_STRING_PART]);
/// `KtTokens.OPERATIONS`.
pub const OPERATIONS: TokenSet = TokenSet::create(&[
    AS_KEYWORD,
    AS_SAFE,
    IS_KEYWORD,
    IN_KEYWORD,
    DOT,
    PLUSPLUS,
    MINUSMINUS,
    EXCLEXCL,
    MUL,
    PLUS,
    MINUS,
    EXCL,
    DIV,
    PERC,
    LT,
    GT,
    LTEQ,
    GTEQ,
    EQEQEQ,
    EXCLEQEQEQ,
    EQEQ,
    EXCLEQ,
    ANDAND,
    OROR,
    SAFE_ACCESS,
    ELVIS,
    RANGE,
    RANGE_UNTIL,
    EQ,
    MULTEQ,
    DIVEQ,
    PERCEQ,
    PLUSEQ,
    MINUSEQ,
    NOT_IN,
    NOT_IS,
    IDENTIFIER,
]);
/// `KtTokens.AUGMENTED_ASSIGNMENTS`.
pub const AUGMENTED_ASSIGNMENTS: TokenSet = TokenSet::create(&[PLUSEQ, MINUSEQ, MULTEQ, PERCEQ, DIVEQ]);
/// `KtTokens.ALL_ASSIGNMENTS`.
pub const ALL_ASSIGNMENTS: TokenSet = TokenSet::create(&[EQ, PLUSEQ, MINUSEQ, MULTEQ, PERCEQ, DIVEQ]);
/// `KtTokens.INCREMENT_AND_DECREMENT`.
pub const INCREMENT_AND_DECREMENT: TokenSet = TokenSet::create(&[PLUSPLUS, MINUSMINUS]);
/// `KtTokens.QUALIFIED_ACCESS`.
pub const QUALIFIED_ACCESS: TokenSet =
    TokenSet::create_nodes(&[NodeKind::DOT_QUALIFIED_EXPRESSION, NodeKind::SAFE_ACCESS_EXPRESSION]);
/// `KtTokens.VAL_VAR`.
pub const VAL_VAR: TokenSet = TokenSet::create(&[VAL_KEYWORD, VAR_KEYWORD]);

/// `KtTokenSets.POSTFIX_OPERATIONS`.
pub const POSTFIX_OPERATIONS: TokenSet = TokenSet::create(&[PLUSPLUS, MINUSMINUS, EXCLEXCL, DOT, SAFE_ACCESS]);
/// `KtTokenSets.PREFIX_OPERATIONS`.
pub const PREFIX_OPERATIONS: TokenSet = TokenSet::create(&[MINUS, PLUS, MINUSMINUS, PLUSPLUS, EXCL]);

/// `KDocTokens.CONTENT_TOKENS`.
pub const KDOC_CONTENT_TOKENS: TokenSet = TokenSet::create(&[
    KDOC_TEXT,
    KDOC_CODE_BLOCK_TEXT,
    KDOC_CODE_SPAN_TEXT,
    KDOC_TAG_NAME,
    KDOC_MARKDOWN_LINK,
    KDOC_MARKDOWN_ESCAPED_CHAR,
    KDOC_MARKDOWN_INLINE_LINK,
    KDOC_LPAR,
    KDOC_RPAR,
]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_keywords_are_soft_and_hard_keywords_are_not() {
        for t in SOFT_KEYWORDS.iter() {
            assert!(t.is_soft_keyword() && t.is_keyword(), "{t:?}");
        }
        for t in KEYWORDS.iter() {
            assert!(t.is_keyword() && !t.is_soft_keyword(), "{t:?}");
        }
        for t in MODIFIER_KEYWORDS.iter() {
            assert!(t.is_modifier_keyword(), "{t:?}");
        }
        // KtModifierKeywordToken: the canonical array plus CONTRACT_KEYWORD, which is a modifier token but not in it.
        assert_eq!(
            TokenKind::ALL.iter().filter(|t| t.is_modifier_keyword()).count(),
            MODIFIER_KEYWORDS_ARRAY.len() + 1
        );
        assert_eq!(TokenKind::soft_keyword_by_text(b"get"), Some(GET_KEYWORD));
        assert_eq!(TokenKind::keyword_by_debug_name(b"AS_SAFE"), Some(AS_SAFE));
        assert_eq!(LPAR.token_id(), 52);
        assert_eq!(WHITE_SPACE.token_id(), INVALID_ID);
        assert!(TokenSet::EMPTY.iter().next().is_none());
        assert!(!KEYWORDS.contains(None::<TokenKind>));
        assert!(QUALIFIED_ACCESS.contains(NodeKind::SAFE_ACCESS_EXPRESSION));
    }

    /// The byte `match`es of `soft_keyword_by_text` and `keyword_by_debug_name` answer as the scans of
    /// `SOFT_KEYWORDS` and `KEYWORDS` they replace, for every token's value and debug name and for near misses.
    #[test]
    fn keyword_lookups_agree_with_the_keyword_sets() {
        let soft = |text: &[u8]| SOFT_KEYWORDS.iter().find(|t| t.value().is_some_and(|v| v.as_bytes() == text));
        let hard = |text: &[u8]| KEYWORDS.iter().find(|t| t.debug_name().as_bytes() == text);
        let mut inputs: Vec<Vec<u8>> = vec![b"".to_vec(), b"\xff".to_vec(), b"g\xffet".to_vec(), b"`get`".to_vec()];
        for t in TokenKind::ALL {
            for text in t.value().into_iter().chain([t.debug_name()]) {
                let text = text.as_bytes();
                inputs.push(text.to_vec());
                inputs.push(text[..text.len() - 1].to_vec());
                inputs.push([text, b"x"].concat());
                inputs.push(text.to_ascii_uppercase());
            }
        }
        for text in &inputs {
            assert_eq!(TokenKind::soft_keyword_by_text(text), soft(text), "{:?}", String::from_utf8_lossy(text));
            assert_eq!(TokenKind::keyword_by_debug_name(text), hard(text), "{:?}", String::from_utf8_lossy(text));
        }
        assert_eq!(
            SOFT_KEYWORDS
                .iter()
                .filter(|t| TokenKind::soft_keyword_by_text(t.value().unwrap().as_bytes()) == Some(*t))
                .count(),
            SOFT_KEYWORDS.iter().count()
        );
        assert_eq!(
            KEYWORDS.iter().filter(|t| TokenKind::keyword_by_debug_name(t.debug_name().as_bytes()) == Some(*t)).count(),
            KEYWORDS.iter().count()
        );
    }
}
