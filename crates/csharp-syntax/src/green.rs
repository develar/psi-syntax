//! The green tree of Roslyn's internal syntax (`Syntax/InternalSyntax`, `GreenNode`) as an arena: tokens with their
//! leading and trailing trivia, trivia (structured or not), nodes and lists, each a record addressed by a `Green`
//! (or a `Trivia`), and the diagnostics attached to them (`SyntaxDiagnosticInfo`: an offset from the element's start and
//! a width). Records are never changed: what Roslyn does by creating a new green node (`WithDiagnosticsGreen`,
//! `TokenWithLeadingTrivia`, `SyntaxFirstTokenReplacer`, ...) appends a new record, and the old one stays, garbage, until
//! the file ends. The buffers belong to the `Parser` that reuses the arena from file to file (`clear`).
//!
//! Widths are UTF-8 bytes (Roslyn's are UTF-16 code units): every width and offset of a tree is a sum of widths, so a
//! tree in bytes is Roslyn's tree in UTF-16 with each position mapped to bytes.

use crate::error_code::{ErrorCode, MessageID, XmlParseErrorCode};
use crate::facts;
use crate::language_version::LanguageVersion;
use crate::syntax::node_type;
use crate::syntax_kind::SyntaxKind;

/// A green element: null (a slot or list Roslyn leaves `null`), a token, a node or a list (`GreenNode.IsList`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Green(u32);

const TAG_SHIFT: u32 = 30;
const INDEX_MASK: u32 = (1 << TAG_SHIFT) - 1;
const TAG_TOKEN: u32 = 1;
const TAG_NODE: u32 = 2;
const TAG_LIST: u32 = 3;

impl Green {
    /// `null`.
    pub const NULL: Green = Green(0);

    #[inline]
    fn new(tag: u32, index: usize) -> Green {
        Green((tag << TAG_SHIFT) | (index as u32 & INDEX_MASK))
    }

    #[inline]
    fn tag(self) -> u32 {
        self.0 >> TAG_SHIFT
    }

    #[inline]
    fn index(self) -> usize {
        (self.0 & INDEX_MASK) as usize
    }

    #[inline]
    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    /// `GreenNode.IsToken`.
    #[inline]
    pub fn is_token(self) -> bool {
        self.tag() == TAG_TOKEN
    }

    /// A node (not a token, not a list).
    #[inline]
    pub fn is_node(self) -> bool {
        self.tag() == TAG_NODE
    }

    /// `GreenNode.IsList`.
    #[inline]
    pub fn is_list(self) -> bool {
        self.tag() == TAG_LIST
    }

    /// `node ?? other`.
    #[inline]
    pub fn or(self, other: Green) -> Green {
        if self.is_null() { other } else { self }
    }
}

/// A trivia record: `SyntaxTrivia` (a kind and a text) or a structured trivia (a node: a directive, a documentation
/// comment, skipped tokens), as an element of a token's leading or trailing trivia.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Trivia(u32);

/// A token's leading or trailing trivia: a run of `Trivia` (Roslyn's `GreenNode` of one trivia or a list of them).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct TriviaList {
    start: u32,
    len: u32,
}

impl TriviaList {
    /// No trivia (`null`).
    pub const EMPTY: TriviaList = TriviaList { start: 0, len: 0 };

    pub fn len(self) -> usize {
        self.len as usize
    }

    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

/// The diagnostics of an element: a run of indices into the arena's diagnostics.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
struct DiagList {
    start: u32,
    len: u32,
}

/// The index of a `DiagnosticInfo` in the arena.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DiagnosticId(u32);

/// `SyntaxDiagnosticInfo` (or `XmlSyntaxDiagnosticInfo` when `xml` is set): a code, its arguments, and where it is,
/// relative to the *start* (not the full start) of the element it is attached to.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DiagnosticInfo {
    pub code: ErrorCode,
    /// The `XmlParseErrorCode` of a `WRN_XMLParseError` (`XmlSyntaxDiagnosticInfo`): its message is the code's
    /// message, formatted with the arguments, inside `WRN_XMLParseError`'s.
    pub xml: Option<XmlParseErrorCode>,
    pub offset: i32,
    pub width: i32,
    args_start: u32,
    args_len: u32,
}

/// An argument of a diagnostic, formatted as `string.Format` formats the object Roslyn passes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DiagArg {
    /// A string constant (`SyntaxFacts.GetText(kind)`, `"..."`).
    Str(&'static str),
    /// `token.Text` (`token.ToString()`): the text of a token of the arena.
    TokenText(Green),
    /// A string the arena keeps (`GreenArena::intern`).
    Owned { start: u32, len: u32 },
    /// An integer (`(int)code`, a count).
    Int(i64),
    /// `MessageID.Localize()`: the feature's English name.
    Feature(MessageID),
    /// `CSharpRequiredLanguageVersion`: `ToDisplayString()` of the version.
    Version(LanguageVersion),
    /// A `SyntaxKind` boxed as an object: `ToString()`, its name.
    Kind(SyntaxKind),
    /// An embedded `DiagnosticInfo` (`WRN_ErrorOverride`'s first argument): its message.
    Info(DiagnosticId),
}

/// How a token's text is found.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TokenText {
    /// `width` bytes of the source at `start`.
    Source { start: u32, width: u32 },
    /// `SyntaxFacts.GetText(kind)`, what `SyntaxFactory.Token(kind)` gives a token it makes.
    Fixed,
    /// A string the arena keeps (`GreenArena::intern`), for a token whose text is built (Roslyn's
    /// `SyntaxFactory.Literal(..., text, ...)` with a new string).
    Owned { start: u32, len: u32 },
}

// Roslyn's `GreenNode.NodeFlags` (the same bits).
/// The element has a token that is not missing (`IsNotMissing`): set on every token but a missing one, inherited.
pub(crate) const IS_NOT_MISSING: u16 = 1 << 0;
pub(crate) const CONTAINS_DIAGNOSTICS: u16 = 1 << 7;
pub(crate) const CONTAINS_DIRECTIVES: u16 = 1 << 8;
pub(crate) const CONTAINS_SKIPPED_TEXT: u16 = 1 << 9;
pub(crate) const CONTAINS_STRUCTURED_TRIVIA: u16 = 1 << 10;
const INHERIT_MASK: u16 =
    IS_NOT_MISSING | CONTAINS_DIAGNOSTICS | CONTAINS_DIRECTIVES | CONTAINS_SKIPPED_TEXT | CONTAINS_STRUCTURED_TRIVIA;
/// Not Roslyn's: a token whose text is in `strings` (`TokenText::Owned`).
const OWNED_TEXT: u16 = 1 << 13;
/// Not Roslyn's: a token whose text is `SyntaxFacts.GetText(kind)` (`TokenText::Fixed`).
const FIXED_TEXT: u16 = 1 << 14;

const NO_VALUE: u32 = u32::MAX;

#[derive(Copy, Clone, Debug)]
struct TokenData {
    kind: SyntaxKind,
    /// `ContextualKind`: the contextual keyword an identifier token spells, else `kind`.
    contextual: SyntaxKind,
    flags: u16,
    /// The text's source offset (`TokenText::Source`) or `strings` offset (`OWNED_TEXT`); unused for `FIXED_TEXT`.
    start: u32,
    width: u32,
    leading: TriviaList,
    trailing: TriviaList,
    leading_width: u32,
    trailing_width: u32,
    diags: DiagList,
    /// The `ValueText` of an identifier or literal whose value is not its text (`@class`, `a`): an index into
    /// `values`, or `NO_VALUE`.
    value: u32,
}

#[derive(Copy, Clone, Debug)]
struct NodeData {
    kind: SyntaxKind,
    flags: u16,
    slot_start: u32,
    slot_count: u32,
    full_width: u32,
    diags: DiagList,
    /// The boolean fields of the node type (`syntax::types::X::IS_ACTIVE`, ...).
    extra: u32,
}

#[derive(Copy, Clone, Debug)]
struct TriviaData {
    kind: SyntaxKind,
    flags: u16,
    start: u32,
    width: u32,
    /// The structure of a structured trivia (a node), else null.
    structure: Green,
    diags: DiagList,
}

/// The green arena of one file: owned by a `Parser`, cleared per file.
#[derive(Default)]
pub struct GreenArena {
    tokens: Vec<TokenData>,
    nodes: Vec<NodeData>,
    trivia: Vec<TriviaData>,
    slots: Vec<Green>,
    trivia_refs: Vec<Trivia>,
    diag_refs: Vec<u32>,
    diagnostics: Vec<DiagnosticInfo>,
    args: Vec<DiagArg>,
    /// Strings the parser or lexer built: owned token texts, value texts, diagnostic arguments.
    strings: String,
    /// `(start, len)` in `strings` of each token value (`TokenData::value`).
    values: Vec<(u32, u32)>,
    /// Scratch of the walks (`enumerate_nodes`, the token replacers), kept to reuse its buffer.
    stack: Vec<Green>,
}

impl GreenArena {
    pub fn new() -> GreenArena {
        GreenArena::default()
    }

    /// Forgets every record, keeping the buffers.
    pub fn clear(&mut self) {
        self.tokens.clear();
        self.nodes.clear();
        self.trivia.clear();
        self.slots.clear();
        self.trivia_refs.clear();
        self.diag_refs.clear();
        self.diagnostics.clear();
        self.args.clear();
        self.strings.clear();
        self.values.clear();
        self.stack.clear();
    }

    /// The bytes the arena's buffers hold (by capacity): garbage included, what a parse costs beyond its tree.
    pub fn heap_bytes(&self) -> usize {
        self.tokens.capacity() * size_of::<TokenData>()
            + self.nodes.capacity() * size_of::<NodeData>()
            + self.trivia.capacity() * size_of::<TriviaData>()
            + self.slots.capacity() * size_of::<Green>()
            + self.trivia_refs.capacity() * size_of::<Trivia>()
            + self.diag_refs.capacity() * size_of::<u32>()
            + self.diagnostics.capacity() * size_of::<DiagnosticInfo>()
            + self.args.capacity() * size_of::<DiagArg>()
            + self.strings.capacity()
            + self.values.capacity() * size_of::<(u32, u32)>()
    }

    /// The number of records of each kind (tokens, nodes and lists, trivia) so far, garbage included.
    pub fn record_counts(&self) -> (usize, usize, usize) {
        (self.tokens.len(), self.nodes.len(), self.trivia.len())
    }

    /// Keeps `text` in the arena: its offset and length for `TokenText::Owned`, `DiagArg::Owned` or a value.
    pub fn intern(&mut self, text: &str) -> (u32, u32) {
        let start = self.strings.len() as u32;
        self.strings.push_str(text);
        (start, text.len() as u32)
    }

    /// A string `intern` kept.
    pub fn interned(&self, start: u32, len: u32) -> &str {
        self.strings.get(start as usize..(start as usize + len as usize)).unwrap_or("")
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Trivia

    /// A `SyntaxTrivia` of `kind` over `width` bytes of the source at `start` (`SyntaxFactory.Whitespace`, `Comment`,
    /// `EndOfLine`, `DisabledText`, ...).
    pub fn trivia(&mut self, kind: SyntaxKind, start: u32, width: u32) -> Trivia {
        let flags = if kind == SyntaxKind::PreprocessingMessageTrivia { CONTAINS_SKIPPED_TEXT } else { 0 };
        self.push_trivia(TriviaData { kind, flags, start, width, structure: Green::NULL, diags: DiagList::default() })
    }

    /// A structured trivia: the trivia whose structure is `node` (a directive, a documentation comment, a
    /// `SkippedTokensTrivia`), as Roslyn puts a `StructuredTriviaSyntax` into a trivia list.
    pub fn structured_trivia(&mut self, node: Green) -> Trivia {
        let kind = self.kind(node);
        let flags = self.flags(node) & INHERIT_MASK;
        let width = self.full_width(node);
        self.push_trivia(TriviaData { kind, flags, start: 0, width, structure: node, diags: DiagList::default() })
    }

    /// `trivia` with `diagnostics` (`WithDiagnosticsGreen` of a trivia): a new record. A structured trivia is its
    /// structure in Roslyn's green tree, so the structure gets them (a new node, then a new trivia over it).
    pub fn trivia_with_diagnostics(&mut self, trivia: Trivia, diagnostics: &[DiagnosticId]) -> Trivia {
        let mut data = self.trivia[trivia.0 as usize];
        if !data.structure.is_null() {
            let structure = self.with_diagnostics(data.structure, diagnostics);
            return self.structured_trivia(structure);
        }
        data.diags = self.push_diag_list(diagnostics.iter().map(|d| d.0));
        data.flags &= !CONTAINS_DIAGNOSTICS;
        if data.diags.len != 0 {
            data.flags |= CONTAINS_DIAGNOSTICS;
        }
        self.push_trivia(data)
    }

    fn push_trivia(&mut self, data: TriviaData) -> Trivia {
        let id = Trivia(self.trivia.len() as u32);
        self.trivia.push(data);
        id
    }

    pub fn trivia_kind(&self, trivia: Trivia) -> SyntaxKind {
        self.trivia[trivia.0 as usize].kind
    }

    /// The trivia's full width (its text's, or its structure's full width).
    pub fn trivia_width(&self, trivia: Trivia) -> u32 {
        self.trivia[trivia.0 as usize].width
    }

    /// The source offset of an unstructured trivia's text.
    pub fn trivia_start(&self, trivia: Trivia) -> u32 {
        self.trivia[trivia.0 as usize].start
    }

    /// `GetStructure()`: the node of a structured trivia, else null.
    pub fn trivia_structure(&self, trivia: Trivia) -> Green {
        self.trivia[trivia.0 as usize].structure
    }

    /// `IsSkippedTokensTrivia`.
    pub fn is_skipped_tokens_trivia(&self, trivia: Trivia) -> bool {
        self.trivia[trivia.0 as usize].kind == SyntaxKind::SkippedTokensTrivia
    }

    pub fn trivia_contains_diagnostics(&self, trivia: Trivia) -> bool {
        self.trivia[trivia.0 as usize].flags & CONTAINS_DIAGNOSTICS != 0
    }

    /// The diagnostics attached to an unstructured trivia (a structured trivia's are its structure's).
    pub fn trivia_diagnostics(&self, trivia: Trivia) -> impl ExactSizeIterator<Item = DiagnosticId> + '_ {
        self.diag_slice(self.trivia[trivia.0 as usize].diags).iter().map(|&d| DiagnosticId(d))
    }

    /// A trivia list of `items` in order (`SyntaxListBuilder.ToListNode()` of trivia).
    pub fn trivia_list(&mut self, items: &[Trivia]) -> TriviaList {
        if items.is_empty() {
            return TriviaList::EMPTY;
        }
        let start = self.trivia_refs.len() as u32;
        self.trivia_refs.extend_from_slice(items);
        TriviaList { start, len: items.len() as u32 }
    }

    /// The trivia of a list, in order.
    pub fn trivia_items(&self, list: TriviaList) -> &[Trivia] {
        self.trivia_refs.get(list.start as usize..(list.start + list.len) as usize).unwrap_or(&[])
    }

    /// The trivia list of `first` followed by `second` (`SyntaxList.Concat`).
    pub fn concat_trivia(&mut self, first: TriviaList, second: TriviaList) -> TriviaList {
        if first.is_empty() {
            return second;
        }
        if second.is_empty() {
            return first;
        }
        let start = self.trivia_refs.len() as u32;
        self.trivia_refs.extend_from_within(first.start as usize..(first.start + first.len) as usize);
        self.trivia_refs.extend_from_within(second.start as usize..(second.start + second.len) as usize);
        TriviaList { start, len: first.len + second.len }
    }

    fn trivia_list_width_and_flags(&self, list: TriviaList) -> (u32, u16) {
        let (mut width, mut flags) = (0u32, 0u16);
        for t in self.trivia_items(list) {
            let d = &self.trivia[t.0 as usize];
            width = width.wrapping_add(d.width);
            flags |= d.flags & INHERIT_MASK;
        }
        (width, flags)
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Tokens

    /// A token (`SyntaxFactory.Token`, `Identifier`, `Literal`, `BadToken`, ...): `kind`, its `ContextualKind`, its
    /// text and trivia. A missing token is made with `missing_token_with_trivia`.
    pub fn create_token(
        &mut self,
        leading: TriviaList,
        kind: SyntaxKind,
        contextual_kind: SyntaxKind,
        text: TokenText,
        trailing: TriviaList,
    ) -> Green {
        let (start, width, text_flag) = match text {
            TokenText::Source { start, width } => (start, width, 0),
            TokenText::Fixed => (0, facts::get_text(kind).len() as u32, FIXED_TEXT),
            TokenText::Owned { start, len } => (start, len, OWNED_TEXT),
        };
        self.push_token(kind, contextual_kind, IS_NOT_MISSING | text_flag, start, width, leading, trailing)
    }

    /// `SyntaxFactory.Token(kind)`: a token of the fixed text `SyntaxFacts.GetText(kind)`, no trivia.
    pub fn token(&mut self, kind: SyntaxKind) -> Green {
        self.create_token(TriviaList::EMPTY, kind, kind, TokenText::Fixed, TriviaList::EMPTY)
    }

    /// `SyntaxFactory.Token(leading, kind, trailing)`: a token of the fixed text with trivia (e.g. the `>>` the parser
    /// merges from two `>`).
    pub fn token_with_trivia(&mut self, leading: TriviaList, kind: SyntaxKind, trailing: TriviaList) -> Green {
        self.create_token(leading, kind, kind, TokenText::Fixed, trailing)
    }

    /// `SyntaxFactory.MissingToken(kind)`.
    pub fn missing_token(&mut self, kind: SyntaxKind) -> Green {
        self.missing_token_with_trivia(TriviaList::EMPTY, kind, TriviaList::EMPTY)
    }

    /// `SyntaxFactory.MissingToken(leading, kind, trailing)`: no text, `IsMissing`.
    pub fn missing_token_with_trivia(&mut self, leading: TriviaList, kind: SyntaxKind, trailing: TriviaList) -> Green {
        self.push_token(kind, kind, FIXED_TEXT, 0, 0, leading, trailing)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_token(
        &mut self,
        kind: SyntaxKind,
        contextual: SyntaxKind,
        own_flags: u16,
        start: u32,
        width: u32,
        leading: TriviaList,
        trailing: TriviaList,
    ) -> Green {
        let (leading_width, leading_flags) = self.trivia_list_width_and_flags(leading);
        let (trailing_width, trailing_flags) = self.trivia_list_width_and_flags(trailing);
        // `SyntaxTokenWithTrivia` inherits its trivia's flags; a missing token clears `IsNotMissing` after that.
        let mut flags = ((leading_flags | trailing_flags) & INHERIT_MASK) | own_flags;
        if own_flags & IS_NOT_MISSING == 0 {
            flags &= !IS_NOT_MISSING;
        }
        let id = Green::new(TAG_TOKEN, self.tokens.len());
        self.tokens.push(TokenData {
            kind,
            contextual,
            flags,
            start,
            width,
            leading,
            trailing,
            leading_width,
            trailing_width,
            diags: DiagList::default(),
            value: NO_VALUE,
        });
        id
    }

    #[inline]
    fn token_data(&self, token: Green) -> &TokenData {
        &self.tokens[token.index()]
    }

    /// A copy of `token` with `f` applied, flags recomputed from its trivia and diagnostics.
    fn copy_token(&mut self, token: Green, f: impl FnOnce(&mut TokenData)) -> Green {
        if !token.is_token() {
            return token;
        }
        let mut data = *self.token_data(token);
        f(&mut data);
        let (leading_width, leading_flags) = self.trivia_list_width_and_flags(data.leading);
        let (trailing_width, trailing_flags) = self.trivia_list_width_and_flags(data.trailing);
        data.leading_width = leading_width;
        data.trailing_width = trailing_width;
        let own = data.flags & (IS_NOT_MISSING | OWNED_TEXT | FIXED_TEXT);
        let mut flags = ((leading_flags | trailing_flags) & INHERIT_MASK) | own;
        if own & IS_NOT_MISSING == 0 {
            flags &= !IS_NOT_MISSING;
        }
        if data.diags.len != 0 {
            flags |= CONTAINS_DIAGNOSTICS;
        }
        data.flags = flags;
        let id = Green::new(TAG_TOKEN, self.tokens.len());
        self.tokens.push(data);
        id
    }

    /// `TokenWithLeadingTrivia`: a copy of `token` (its diagnostics kept) with `leading` as its leading trivia.
    pub fn token_with_leading_trivia(&mut self, token: Green, leading: TriviaList) -> Green {
        self.copy_token(token, |d| d.leading = leading)
    }

    /// `TokenWithTrailingTrivia`.
    pub fn token_with_trailing_trivia(&mut self, token: Green, trailing: TriviaList) -> Green {
        self.copy_token(token, |d| d.trailing = trailing)
    }

    /// A copy of `token` of another kind and contextual kind (what `ConvertToKeyword` and `ConvertToIdentifier` make),
    /// its text, trivia, value and diagnostics kept.
    pub fn token_with_kind(&mut self, token: Green, kind: SyntaxKind, contextual_kind: SyntaxKind) -> Green {
        self.copy_token(token, |d| {
            d.kind = kind;
            d.contextual = contextual_kind;
        })
    }

    /// Sets the `ValueText` of a token just made (the lexer's identifiers and literals whose value is not their text).
    pub fn set_token_value(&mut self, token: Green, value: &str) {
        if !token.is_token() {
            return;
        }
        let (start, len) = self.intern(value);
        let index = self.values.len() as u32;
        self.values.push((start, len));
        self.tokens[token.index()].value = index;
    }

    /// The token's `ContextualKind` (`kind` for a token that is no contextual keyword).
    pub fn contextual_kind(&self, token: Green) -> SyntaxKind {
        if token.is_token() { self.token_data(token).contextual } else { self.kind(token) }
    }

    /// The token's leading trivia.
    pub fn leading_trivia(&self, token: Green) -> TriviaList {
        if token.is_token() { self.token_data(token).leading } else { TriviaList::EMPTY }
    }

    /// The token's trailing trivia.
    pub fn trailing_trivia(&self, token: Green) -> TriviaList {
        if token.is_token() { self.token_data(token).trailing } else { TriviaList::EMPTY }
    }

    /// `SyntaxToken.Text` as bytes: the source's for a lexed token, `SyntaxFacts.GetText(kind)` for a fixed one, empty
    /// for a missing one.
    pub fn token_text<'s>(&'s self, token: Green, source: &'s [u8]) -> &'s [u8] {
        if !token.is_token() {
            return b"";
        }
        let d = self.token_data(token);
        if d.flags & IS_NOT_MISSING == 0 {
            return b"";
        }
        if d.flags & FIXED_TEXT != 0 {
            return facts::get_text(d.kind).as_bytes();
        }
        let range = d.start as usize..d.start as usize + d.width as usize;
        if d.flags & OWNED_TEXT != 0 {
            return self.strings.as_bytes().get(range).unwrap_or(b"");
        }
        source.get(range).unwrap_or(b"")
    }

    /// Where a token's text is in the source (`start`, `width`): `None` for a missing token and for one whose text is
    /// not the source's (`TokenText::Fixed`, `TokenText::Owned`).
    pub fn token_source_range(&self, token: Green) -> Option<(u32, u32)> {
        if !token.is_token() {
            return None;
        }
        let d = self.token_data(token);
        (d.flags & (IS_NOT_MISSING | FIXED_TEXT | OWNED_TEXT) == IS_NOT_MISSING).then_some((d.start, d.width))
    }

    /// `SyntaxToken.ValueText` as bytes: the value the lexer set, else the text.
    pub fn token_value_text<'s>(&'s self, token: Green, source: &'s [u8]) -> &'s [u8] {
        if token.is_token() {
            let d = self.token_data(token);
            if let Some(&(start, len)) = self.values.get(d.value as usize) {
                return self.strings.as_bytes().get(start as usize..(start + len) as usize).unwrap_or(b"");
            }
        }
        self.token_text(token, source)
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Nodes and lists

    /// A node of `kind` over `slots` (null slots included) with the boolean fields `extra`: what the generated
    /// factories (`syntax.rs`) call; its full width and flags are its children's (`AdjustFlagsAndWidth`).
    pub fn create_node(&mut self, kind: SyntaxKind, slots: &[Green], extra: u32) -> Green {
        let own = self.own_node_flags(kind);
        self.push_node(TAG_NODE, kind, own, slots, extra, DiagList::default())
    }

    /// `SyntaxListBuilder.ToListNode()`: null for no item, the item itself for one, else a list.
    pub fn list(&mut self, items: &[Green]) -> Green {
        match items {
            [] => Green::NULL,
            [item] => *item,
            _ => self.push_node(TAG_LIST, SyntaxKind::List, 0, items, 0, DiagList::default()),
        }
    }

    fn push_node(
        &mut self,
        tag: u32,
        kind: SyntaxKind,
        own: u16,
        slots: &[Green],
        extra: u32,
        diags: DiagList,
    ) -> Green {
        let mut flags = own;
        let mut full_width = 0u32;
        for &s in slots {
            if !s.is_null() {
                flags |= self.flags(s) & INHERIT_MASK;
                full_width = full_width.wrapping_add(self.full_width(s));
            }
        }
        if diags.len != 0 {
            flags |= CONTAINS_DIAGNOSTICS;
        }
        let slot_start = self.slots.len() as u32;
        self.slots.extend_from_slice(slots);
        let id = Green::new(tag, self.nodes.len());
        self.nodes.push(NodeData { kind, flags, slot_start, slot_count: slots.len() as u32, full_width, diags, extra });
        id
    }

    /// A copy of a node or list with other slots (the rewriters' `Update`), its kind, boolean fields and diagnostics
    /// kept.
    pub fn update_node(&mut self, node: Green, slots: &[Green]) -> Green {
        if !(node.is_node() || node.is_list()) {
            return node;
        }
        let d = self.nodes[node.index()];
        let own = if node.is_node() { self.own_node_flags(d.kind) } else { 0 };
        self.push_node(node.tag(), d.kind, own, slots, d.extra, d.diags)
    }

    /// The flags a node of `kind` sets itself: `ContainsDirectives` (a directive's generated constructor),
    /// `ContainsStructuredTrivia` (`StructuredTriviaSyntax`) and `ContainsSkippedText` (`SkippedTokensTrivia`).
    fn own_node_flags(&self, kind: SyntaxKind) -> u16 {
        let Some(n) = kind.node_kind() else { return 0 };
        let info = node_type(n);
        let mut flags = 0;
        if info.is_directive {
            flags |= CONTAINS_DIRECTIVES;
        }
        if info.is_structured_trivia {
            flags |= CONTAINS_STRUCTURED_TRIVIA;
        }
        if kind == SyntaxKind::SkippedTokensTrivia {
            flags |= CONTAINS_SKIPPED_TEXT;
        }
        flags
    }

    /// The node's boolean fields (`syntax::types::X::IS_ACTIVE` bits).
    pub fn extra(&self, node: Green) -> u32 {
        if node.is_node() { self.nodes[node.index()].extra } else { 0 }
    }

    /// `SlotCount`: a node's or list's slots (null ones included), 0 for a token.
    pub fn slot_count(&self, element: Green) -> usize {
        if element.is_node() || element.is_list() { self.nodes[element.index()].slot_count as usize } else { 0 }
    }

    /// `GetSlot(index)`: null past the end.
    pub fn slot(&self, element: Green, index: usize) -> Green {
        if !(element.is_node() || element.is_list()) {
            return Green::NULL;
        }
        let d = &self.nodes[element.index()];
        if index >= d.slot_count as usize {
            return Green::NULL;
        }
        self.slots[d.slot_start as usize + index]
    }

    /// The slots of a node or list.
    pub fn slots(&self, element: Green) -> &[Green] {
        if !(element.is_node() || element.is_list()) {
            return &[];
        }
        let d = &self.nodes[element.index()];
        &self.slots[d.slot_start as usize..(d.slot_start + d.slot_count) as usize]
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Common properties

    /// `Kind` (`SyntaxKind::List` for a list, `None` for null).
    #[inline]
    pub fn kind(&self, element: Green) -> SyntaxKind {
        match element.tag() {
            TAG_TOKEN => self.tokens[element.index()].kind,
            TAG_NODE | TAG_LIST => self.nodes[element.index()].kind,
            _ => SyntaxKind::None,
        }
    }

    #[inline]
    fn flags(&self, element: Green) -> u16 {
        match element.tag() {
            TAG_TOKEN => self.tokens[element.index()].flags,
            TAG_NODE | TAG_LIST => self.nodes[element.index()].flags,
            _ => 0,
        }
    }

    /// `IsMissing`: a missing token, or a node with no token that is not missing.
    pub fn is_missing(&self, element: Green) -> bool {
        !element.is_null() && self.flags(element) & IS_NOT_MISSING == 0
    }

    /// `ContainsDiagnostics`: the element or anything in it (trivia included) has diagnostics.
    pub fn contains_diagnostics(&self, element: Green) -> bool {
        self.flags(element) & CONTAINS_DIAGNOSTICS != 0
    }

    /// `ContainsSkippedText`.
    pub fn contains_skipped_text(&self, element: Green) -> bool {
        self.flags(element) & CONTAINS_SKIPPED_TEXT != 0
    }

    /// `ContainsDirectives`.
    pub fn contains_directives(&self, element: Green) -> bool {
        self.flags(element) & CONTAINS_DIRECTIVES != 0
    }

    /// `ContainsStructuredTrivia`.
    pub fn contains_structured_trivia(&self, element: Green) -> bool {
        self.flags(element) & CONTAINS_STRUCTURED_TRIVIA != 0
    }

    /// `FullWidth`: trivia included.
    #[inline]
    pub fn full_width(&self, element: Green) -> u32 {
        match element.tag() {
            TAG_TOKEN => {
                let d = &self.tokens[element.index()];
                d.leading_width.wrapping_add(d.width).wrapping_add(d.trailing_width)
            }
            TAG_NODE | TAG_LIST => self.nodes[element.index()].full_width,
            _ => 0,
        }
    }

    /// `Width`: the full width less the first token's leading and the last token's trailing trivia.
    pub fn width(&self, element: Green) -> u32 {
        if element.is_token() {
            return self.tokens[element.index()].width;
        }
        self.full_width(element)
            .wrapping_sub(self.leading_trivia_width(element))
            .wrapping_sub(self.trailing_trivia_width(element))
    }

    /// `GetLeadingTriviaWidth()`: of the first terminal (0 for an element of no width).
    pub fn leading_trivia_width(&self, element: Green) -> u32 {
        if element.is_token() {
            return self.tokens[element.index()].leading_width;
        }
        if self.full_width(element) == 0 {
            return 0;
        }
        let first = self.first_terminal(element);
        if first.is_token() { self.tokens[first.index()].leading_width } else { 0 }
    }

    /// `GetTrailingTriviaWidth()`: of the last terminal.
    pub fn trailing_trivia_width(&self, element: Green) -> u32 {
        if element.is_token() {
            return self.tokens[element.index()].trailing_width;
        }
        if self.full_width(element) == 0 {
            return 0;
        }
        let last = self.last_terminal(element);
        if last.is_token() { self.tokens[last.index()].trailing_width } else { 0 }
    }

    /// `GetFirstTerminal()`: down the first non-null slot while there is one; a token, or a node or list without slots,
    /// or null.
    pub fn first_terminal(&self, element: Green) -> Green {
        let mut node = element;
        while self.slot_count(node) > 0 {
            node = self.slots(node).iter().copied().find(|s| !s.is_null()).unwrap_or(Green::NULL);
        }
        node
    }

    /// `GetLastTerminal()`.
    pub fn last_terminal(&self, element: Green) -> Green {
        let mut node = element;
        while self.slot_count(node) > 0 {
            node = self.slots(node).iter().rev().copied().find(|s| !s.is_null()).unwrap_or(Green::NULL);
        }
        node
    }

    /// `GetLastNonmissingTerminal()`.
    pub fn last_nonmissing_terminal(&self, element: Green) -> Green {
        let mut node = element;
        while self.slot_count(node) > 0 {
            node = self
                .slots(node)
                .iter()
                .rev()
                .copied()
                .find(|&s| !s.is_null() && !self.is_missing(s))
                .unwrap_or(Green::NULL);
        }
        node
    }

    /// `GetFirstToken()` (null when the first terminal is no token).
    pub fn first_token(&self, element: Green) -> Green {
        let t = self.first_terminal(element);
        if t.is_token() { t } else { Green::NULL }
    }

    /// `GetLastToken()`.
    pub fn last_token(&self, element: Green) -> Green {
        let t = self.last_terminal(element);
        if t.is_token() { t } else { Green::NULL }
    }

    /// `GetLastNonmissingToken()`.
    pub fn last_nonmissing_token(&self, element: Green) -> Green {
        let t = self.last_nonmissing_terminal(element);
        if t.is_token() { t } else { Green::NULL }
    }

    /// `ChildNodesAndTokens()` of a node: its non-null slots, a list slot's elements in its place.
    pub fn child_nodes_and_tokens(&self, element: Green, out: &mut Vec<Green>) {
        out.clear();
        for &s in self.slots(element) {
            if s.is_null() {
                continue;
            }
            if s.is_list() {
                out.extend(self.slots(s).iter().copied().filter(|c| !c.is_null()));
            } else {
                out.push(s);
            }
        }
    }

    /// `EnumerateNodes()`: the element, then every node, list element and token under it in depth-first preorder,
    /// lists flattened (`ChildNodesAndTokens`), not into trivia, appended to `out` (cleared first); iterative.
    pub fn enumerate_nodes(&mut self, element: Green, out: &mut Vec<Green>) {
        out.clear();
        let mut stack = std::mem::take(&mut self.stack);
        stack.clear();
        stack.push(element);
        while let Some(node) = stack.pop() {
            out.push(node);
            if node.is_token() {
                continue;
            }
            let before = stack.len();
            for &s in self.slots(node) {
                if s.is_null() {
                    continue;
                }
                if s.is_list() && !node.is_list() {
                    stack.extend(self.slots(s).iter().copied().filter(|c| !c.is_null()));
                } else {
                    stack.push(s);
                }
            }
            stack[before..].reverse();
        }
        self.stack = stack;
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Diagnostics

    /// A diagnostic (`new SyntaxDiagnosticInfo(offset, width, code, args)`), not attached yet.
    pub fn make_diagnostic(&mut self, offset: i32, width: i32, code: ErrorCode, args: &[DiagArg]) -> DiagnosticId {
        self.make_diagnostic_with(offset, width, code, None, args)
    }

    /// `new XmlSyntaxDiagnosticInfo(offset, width, code, args)`: a `WRN_XMLParseError` with its XML code.
    pub fn make_xml_diagnostic(
        &mut self,
        offset: i32,
        width: i32,
        code: XmlParseErrorCode,
        args: &[DiagArg],
    ) -> DiagnosticId {
        self.make_diagnostic_with(offset, width, ErrorCode::WRN_XMLParseError, Some(code), args)
    }

    fn make_diagnostic_with(
        &mut self,
        offset: i32,
        width: i32,
        code: ErrorCode,
        xml: Option<XmlParseErrorCode>,
        args: &[DiagArg],
    ) -> DiagnosticId {
        let args_start = self.args.len() as u32;
        self.args.extend_from_slice(args);
        let id = DiagnosticId(self.diagnostics.len() as u32);
        self.diagnostics.push(DiagnosticInfo { code, xml, offset, width, args_start, args_len: args.len() as u32 });
        id
    }

    /// A diagnostic as `info` with another offset (`SyntaxDiagnosticInfo.WithOffset`, the replacers' shifts).
    pub fn diagnostic_with_offset(&mut self, info: DiagnosticId, offset: i32) -> DiagnosticId {
        let mut d = self.diagnostics[info.0 as usize];
        d.offset = offset;
        let id = DiagnosticId(self.diagnostics.len() as u32);
        self.diagnostics.push(d);
        id
    }

    pub fn diagnostic(&self, id: DiagnosticId) -> &DiagnosticInfo {
        &self.diagnostics[id.0 as usize]
    }

    /// A diagnostic's arguments.
    pub fn diagnostic_args(&self, id: DiagnosticId) -> &[DiagArg] {
        let d = &self.diagnostics[id.0 as usize];
        &self.args[d.args_start as usize..(d.args_start + d.args_len) as usize]
    }

    fn diag_slice(&self, list: DiagList) -> &[u32] {
        self.diag_refs.get(list.start as usize..(list.start + list.len) as usize).unwrap_or(&[])
    }

    fn push_diag_list(&mut self, ids: impl Iterator<Item = u32>) -> DiagList {
        let start = self.diag_refs.len() as u32;
        self.diag_refs.extend(ids);
        DiagList { start, len: self.diag_refs.len() as u32 - start }
    }

    /// `GetDiagnostics()`: the diagnostics attached to the element itself, in order.
    pub fn diagnostics(&self, element: Green) -> impl ExactSizeIterator<Item = DiagnosticId> + '_ {
        let list = match element.tag() {
            TAG_TOKEN => self.tokens[element.index()].diags,
            TAG_NODE | TAG_LIST => self.nodes[element.index()].diags,
            _ => DiagList::default(),
        };
        self.diag_slice(list).iter().map(|&d| DiagnosticId(d))
    }

    /// `WithDiagnosticsGreen(diagnostics)`: a copy of the element with exactly these diagnostics.
    pub fn with_diagnostics(&mut self, element: Green, diagnostics: &[DiagnosticId]) -> Green {
        let list = self.push_diag_list(diagnostics.iter().map(|d| d.0));
        self.with_diag_list(element, list)
    }

    fn with_diag_list(&mut self, element: Green, list: DiagList) -> Green {
        match element.tag() {
            TAG_TOKEN => self.copy_token(element, |d| d.diags = list),
            TAG_NODE | TAG_LIST => {
                let d = self.nodes[element.index()];
                let own = if element.is_node() { self.own_node_flags(d.kind) } else { 0 };
                let slots = d.slot_start as usize..(d.slot_start + d.slot_count) as usize;
                let mut flags = own;
                for &s in &self.slots[slots.clone()] {
                    flags |= self.flags(s) & INHERIT_MASK;
                }
                if list.len != 0 {
                    flags |= CONTAINS_DIAGNOSTICS;
                }
                let id = Green::new(element.tag(), self.nodes.len());
                // the copy shares the slots: they are never changed
                self.nodes.push(NodeData { flags, diags: list, ..d });
                id
            }
            _ => element,
        }
    }

    /// `SyntaxParser.WithAdditionalDiagnostics`: a copy with the element's diagnostics followed by `diagnostics`.
    pub fn with_additional_diagnostics(&mut self, element: Green, diagnostics: &[DiagnosticId]) -> Green {
        if element.is_null() {
            return element;
        }
        let existing: Vec<u32> = self.diagnostics(element).map(|d| d.0).collect();
        let list = self.push_diag_list(existing.into_iter().chain(diagnostics.iter().map(|d| d.0)));
        self.with_diag_list(element, list)
    }

    // ---------------------------------------------------------------------------------------------------------------
    // Rewriters

    /// `SyntaxFirstTokenReplacer.Replace(root, oldToken, newToken, diagnosticOffsetDelta)`: `root` with its first token
    /// replaced by `new_token`; every node on the way down is copied with its diagnostics' offsets shifted by `delta`
    /// (the new token's own were set by its maker). The first token is the first terminal of the first non-null slots.
    pub fn replace_first_token(&mut self, root: Green, new_token: Green, delta: i32) -> Green {
        if root.is_token() || root.is_null() {
            return new_token;
        }
        let mut path = std::mem::take(&mut self.stack);
        path.clear();
        let mut node = root;
        while !node.is_token() && !node.is_null() {
            path.push(node);
            node = self.slots(node).iter().copied().find(|s| !s.is_null()).unwrap_or(Green::NULL);
        }
        let mut child = new_token;
        let mut slots: Vec<Green> = Vec::new();
        while let Some(parent) = path.pop() {
            slots.clear();
            slots.extend_from_slice(self.slots(parent));
            if let Some(first) = slots.iter_mut().find(|s| !s.is_null()) {
                *first = child;
            }
            let updated = self.update_node(parent, &slots);
            // lists are visited through their elements: only nodes get the shift
            child = if parent.is_node() { self.shift_diagnostics(updated, delta) } else { updated };
        }
        self.stack = path;
        child
    }

    /// `UpdateDiagnosticOffset`: a copy whose diagnostics are moved by `delta`, or the element when it has none.
    fn shift_diagnostics(&mut self, element: Green, delta: i32) -> Green {
        let ids: Vec<DiagnosticId> = self.diagnostics(element).collect();
        if ids.is_empty() {
            return element;
        }
        let shifted: Vec<DiagnosticId> = ids
            .into_iter()
            .map(|d| {
                let offset = self.diagnostic(d).offset.wrapping_add(delta);
                self.diagnostic_with_offset(d, offset)
            })
            .collect();
        self.with_diagnostics(element, &shifted)
    }

    /// `SyntaxLastTokenReplacer.Replace(root, newToken)`: `root` with its last token (`GetLastToken()`, the last
    /// terminal of the last non-null slots) replaced by `new_token`; nodes on the way copied with their diagnostics.
    pub fn replace_last_token(&mut self, root: Green, new_token: Green) -> Green {
        if root.is_token() || root.is_null() {
            return new_token;
        }
        let mut path = std::mem::take(&mut self.stack);
        path.clear();
        let mut node = root;
        while !node.is_token() && !node.is_null() {
            path.push(node);
            node = self.slots(node).iter().rev().copied().find(|s| !s.is_null()).unwrap_or(Green::NULL);
        }
        let mut child = new_token;
        let mut slots: Vec<Green> = Vec::new();
        while let Some(parent) = path.pop() {
            slots.clear();
            slots.extend_from_slice(self.slots(parent));
            if let Some(last) = slots.iter_mut().rev().find(|s| !s.is_null()) {
                *last = child;
            }
            child = self.update_node(parent, &slots);
        }
        self.stack = path;
        child
    }
}

/// `SyntaxListBuilder`: the elements of a list being built (`GreenArena::list` makes the list).
pub type SyntaxListBuilder = Vec<Green>;

/// `SyntaxListPool`: builders reused instead of allocated per list.
#[derive(Default)]
pub struct SyntaxListPool {
    free: Vec<SyntaxListBuilder>,
}

impl SyntaxListPool {
    /// `Allocate()`: an empty builder.
    pub fn allocate(&mut self) -> SyntaxListBuilder {
        self.free.pop().unwrap_or_default()
    }

    /// `Free(builder)`: returns it, emptied, for reuse.
    pub fn free(&mut self, mut builder: SyntaxListBuilder) {
        builder.clear();
        self.free.push(builder);
    }

    /// `ToListAndFree(builder)`: the list of its elements; the builder is returned.
    pub fn to_list_and_free(&mut self, arena: &mut GreenArena, builder: SyntaxListBuilder) -> Green {
        let list = arena.list(&builder);
        self.free(builder);
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `a b` lexed by hand: an identifier with a trailing space, then an identifier and the end of file.
    fn two_identifiers(arena: &mut GreenArena) -> (Green, Green, Green) {
        let space = arena.trivia(SyntaxKind::WhitespaceTrivia, 1, 1);
        let trailing = arena.trivia_list(&[space]);
        let a = arena.create_token(
            TriviaList::EMPTY,
            SyntaxKind::IdentifierToken,
            SyntaxKind::IdentifierToken,
            TokenText::Source { start: 0, width: 1 },
            trailing,
        );
        let b = arena.create_token(
            TriviaList::EMPTY,
            SyntaxKind::IdentifierToken,
            SyntaxKind::IdentifierToken,
            TokenText::Source { start: 2, width: 1 },
            TriviaList::EMPTY,
        );
        let eof = arena.token(SyntaxKind::EndOfFileToken);
        (a, b, eof)
    }

    #[test]
    fn widths_flags_and_terminals() {
        let mut arena = GreenArena::new();
        let (a, b, _) = two_identifiers(&mut arena);
        assert_eq!((arena.full_width(a), arena.width(a), arena.trailing_trivia_width(a)), (2, 1, 1));
        assert_eq!(arena.token_text(a, b"a b"), b"a");
        let left = arena.identifier_name(a);
        let right = arena.identifier_name(b);
        let dot = arena.missing_token(SyntaxKind::DotToken);
        let name = arena.qualified_name(left, dot, right);
        assert_eq!(arena.kind(name), SyntaxKind::QualifiedName);
        assert_eq!((arena.full_width(name), arena.width(name)), (3, 3));
        assert_eq!(arena.first_token(name), a);
        assert_eq!(arena.last_token(name), b);
        assert!(arena.is_missing(dot) && !arena.is_missing(name));
        assert_eq!(arena.token_text(dot, b"a b"), b"");
        let missing = arena.identifier_name(dot);
        assert!(arena.is_missing(missing));
        assert_eq!(arena.last_nonmissing_token(name), b);
        let list = arena.list(&[left, right]);
        assert!(list.is_list() && arena.full_width(list) == 3);
        assert_eq!(arena.list(&[left]), left);
        assert!(arena.list(&[]).is_null());
        let mut nodes = Vec::new();
        arena.enumerate_nodes(name, &mut nodes);
        assert_eq!(nodes, [name, left, a, dot, right, b]);
    }

    #[test]
    fn copies_keep_the_original() {
        let mut arena = GreenArena::new();
        let (a, b, eof) = two_identifiers(&mut arena);
        let error = arena.make_diagnostic(0, 1, ErrorCode::ERR_SyntaxError, &[DiagArg::Str(";")]);
        let a2 = arena.with_additional_diagnostics(a, &[error]);
        assert!(arena.contains_diagnostics(a2) && !arena.contains_diagnostics(a));
        assert_eq!(arena.diagnostics(a2).collect::<Vec<_>>(), [error]);
        let name = arena.identifier_name(a2);
        assert!(arena.contains_diagnostics(name));
        // a skipped token as leading trivia of the end of file (`AddLeadingSkippedSyntax`)
        let skipped = arena.skipped_tokens_trivia(b);
        let trivia = arena.structured_trivia(skipped);
        let leading = arena.trivia_list(&[trivia]);
        let eof2 = arena.token_with_leading_trivia(eof, leading);
        assert!(arena.contains_skipped_text(eof2) && arena.contains_structured_trivia(eof2));
        assert_eq!((arena.full_width(eof2), arena.leading_trivia_width(eof2)), (1, 1));
        assert_eq!(arena.leading_trivia(eof), TriviaList::EMPTY);
        // the first-token replacer shifts the diagnostics of the nodes it copies, not the new token's
        let node_error = arena.make_diagnostic(0, 1, ErrorCode::ERR_SemicolonExpected, &[]);
        let with_error = arena.with_diagnostics(name, &[node_error]);
        let replaced = arena.replace_first_token(with_error, eof2, -5);
        assert_eq!(arena.first_token(replaced), eof2);
        let shifted: Vec<_> = arena.diagnostics(replaced).map(|d| arena.diagnostic(d).offset).collect();
        assert_eq!(shifted, [-5]);
        assert_eq!(arena.diagnostic(node_error).offset, 0);
        let last = arena.replace_last_token(with_error, b);
        assert_eq!(arena.last_token(last), b);
        assert_eq!(arena.diagnostics(last).count(), 1);
    }
}
