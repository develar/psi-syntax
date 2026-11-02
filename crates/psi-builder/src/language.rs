//! What a language gives the builder, the tree and the parse driver: its token and node types (`IElementType`), its
//! binders, its whitespace and comments (`ParserDefinition.getWhitespaceTokens`, `getCommentTokens`), its left-bound
//! types (`IElementType.isLeftBound`, `WhitespaceOrCommentBindingPolicy`), its lazily parsed elements
//! (`ILazyParseableElementType`) and how its files and chameleons are lexed and parsed.

use std::fmt::Debug;
use std::hash::Hash;

use crate::builder::{PsiBuilderImpl, WhitespacesAndCommentsBinder};
use crate::lexer::Lexed;
use crate::tree::Chameleon;

/// A token type: what the lexer emits, a leaf of the tree. Its `index` is below 256.
pub trait TokenType: Copy + Eq + Hash + Debug + Send + Sync + 'static {
    /// A dense number below 256 (the tree's `kinds` column and `ElementTypeSet` store it).
    fn index(self) -> u8;
    /// The token type of `index()`, `None` for a number that is none.
    fn from_index(index: usize) -> Option<Self>;
    /// `IElementType.toString()`: what the PSI dump prints.
    fn debug_name(self) -> &'static str;
}

/// A composite node type. Its `index` is below 256 (the tree stores it as 256 + index).
pub trait NodeType: Copy + Eq + Hash + Debug + Send + Sync + 'static {
    fn index(self) -> u8;
    fn from_index(index: usize) -> Option<Self>;
    fn debug_name(self) -> &'static str;
}

/// An element type: what `IElementType` is in the Java code, a token type or a composite node type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ElementType<T, N> {
    Token(T),
    Node(N),
}

/// The element type of a language.
pub type ElementTypeOf<L> = ElementType<<L as Language>::Token, <L as Language>::Node>;

impl<T: TokenType, N: NodeType> ElementType<T, N> {
    /// `IElementType.toString()`.
    #[inline]
    pub fn debug_name(self) -> &'static str {
        match self {
            ElementType::Token(t) => t.debug_name(),
            ElementType::Node(n) => n.debug_name(),
        }
    }

    #[inline]
    pub fn token(self) -> Option<T> {
        match self {
            ElementType::Token(t) => Some(t),
            ElementType::Node(_) => None,
        }
    }

    #[inline]
    pub fn node(self) -> Option<N> {
        match self {
            ElementType::Node(n) => Some(n),
            ElementType::Token(_) => None,
        }
    }

    /// A dense number: a token's index (below 256), 256 + a node type's.
    #[inline]
    pub fn index(self) -> usize {
        match self {
            ElementType::Token(t) => t.index() as usize,
            ElementType::Node(n) => 256 + n.index() as usize,
        }
    }

    /// The element type of `index()` (`None` for a number that is none).
    #[inline]
    pub fn from_index(index: usize) -> Option<Self> {
        if index < 256 {
            T::from_index(index).map(ElementType::Token)
        } else {
            N::from_index(index - 256).map(ElementType::Node)
        }
    }
}

/// A language: a marker type (`kotlin_syntax::Kotlin`) whose associated types and functions are what the platform
/// reads from its `ParserDefinition`, its element types and its lazy element types. The builder, the tree and the
/// parse driver are generic over it, so each language's parse is compiled for its own types.
pub trait Language: Sized + 'static {
    type Token: TokenType;
    type Node: NodeType;
    /// The language's `WhitespacesAndCommentsBinder`s (the platform's two defaults among them).
    type Binder: WhitespacesAndCommentsBinder<Self::Token>;
    /// What a chameleon (an `ILazyParseableElementType` the tree expands) is reparsed with.
    type Lazy: Copy + Eq + Debug + Send + Sync + 'static;
    /// How a file is parsed (e.g. a Kotlin file or a script); the tree keeps it.
    type FileKind: Copy + Eq + Debug + Send + Sync + 'static;
    /// What a parse of the language runs on: a `PsiBuilderImpl` (`frame_builder`) with the state of the language's
    /// layers over it (Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl`). A `Parser` keeps one per nested builder and
    /// reuses it in place, from one file and chameleon to the next.
    type Frame: Default + Send + 'static;

    /// The builder of a frame.
    fn frame_builder(frame: &mut Self::Frame) -> &mut PsiBuilderImpl<Self>;

    /// `ParserDefinition.getFileNodeType()`: the root's type when the parser produced no markers.
    const FILE: Self::Node;
    /// `TokenType.ERROR_ELEMENT`.
    const ERROR_ELEMENT: Self::Node;

    /// `PsiBuilderImpl.whitespaceOrComment(type)`: the parser definition's whitespace or comment tokens.
    fn is_whitespace_or_comment(token: Self::Token) -> bool;

    /// `IElementType.isLeftBound()`: an empty marker of this type binds the whitespace before it to its left
    /// (`PsiBuilderImpl.processDone`).
    fn is_left_bound(node: Self::Node) -> bool;

    /// Whether a leaf of this token type is a chameleon the tree expands (an `ILazyParseableElementType` token, e.g.
    /// a doc comment): `bind` stops at it.
    fn is_chameleon_token(token: Self::Token) -> bool;

    /// What a leaf or a collapsed marker of this type is reparsed with when the tree expands it, if it is a chameleon.
    fn chameleon(element_type: ElementTypeOf<Self>) -> Option<Self::Lazy>;

    /// Whether a chameleon is a body the parser collapsed (a code block or lambda): reparsed from the file's own
    /// tokens when they lex alone the same, left unexpanded by body-less parsing, under the nesting limit.
    fn is_body(kind: Self::Lazy) -> bool;

    /// The cursor's derived token type (not in the Java): what a builder layer computes from the raw current token
    /// once per cursor position, e.g. Kotlin's `getJoinedTokenType(raw, 1)`; `PsiBuilderImpl::current_types` caches
    /// it. The raw type by default.
    #[inline]
    fn joined_token_type(
        builder: &PsiBuilderImpl<Self>,
        raw: Option<Self::Token>,
        raw_lookup_steps: i32,
    ) -> Option<Self::Token> {
        let _ = (builder, raw_lookup_steps);
        raw
    }

    /// How deep the parse may nest (`builder::NESTING_LIMIT`, the default): chosen from the deepest file of the
    /// language's corpus and the stack a level of its parsers takes, so a parse fits a 1 MB stack. Not in the Java.
    const NESTING_LIMIT: u32 = crate::builder::NESTING_LIMIT;
    /// How many type argument lists may nest in one builder (`builder::TYPE_ARGUMENTS_LIMIT`, the default).
    const TYPE_ARGUMENTS_LIMIT: u32 = crate::builder::TYPE_ARGUMENTS_LIMIT;
    /// How many constructs parsed twice may nest in one builder (`builder::REPARSED_LIMIT`, the default).
    const REPARSED_LIMIT: u32 = crate::builder::REPARSED_LIMIT;

    /// The block brackets the parsers ask `PsiBuilderImpl::balanced_block_end` for (Java's `{` and `}`), `None` when
    /// they never do: a chameleon nested deeper than `SHARED_BLOCK_DEPTH` takes their table of the file's tokens,
    /// made once per file, instead of scanning or tabulating its own tokens again in every frame.
    const BLOCK_BRACES: Option<(Self::Token, Self::Token)> = None;

    /// How a token changes the bracket depth (+1 opens, -1 closes, 0 neither): where the rest of a construct nested
    /// too deeply ends (`PsiBuilderImpl::skip_too_deeply_nested`). Not in the Java.
    fn bracket_depth_change(token: Self::Token) -> i64;

    /// Lexes `text`, a file of `file_kind` or a chameleon in one, with the language's lexer into `lexed` (cleared
    /// first), as `TokenSequence.performLexing` does (Java's lexer depends on the file's language level).
    fn tokenize_into(text: &[u8], file_kind: Self::FileKind, lexed: &mut Lexed<Self::Token>);

    /// Parses a file on `frame`, whose builder holds the file's tokens; the builder then builds the tree.
    fn parse_file(frame: &mut Self::Frame, file_kind: Self::FileKind);

    /// Lexes and parses a chameleon of `kind` (`ILazyParseableElementType.parseContents`) on `chameleon.frame`, its
    /// builder made by one of the `Chameleon` methods.
    fn parse_chameleon(kind: Self::Lazy, chameleon: &mut Chameleon<'_, Self>);
}
