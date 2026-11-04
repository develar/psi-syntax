//! The C# tree: `psi_builder`'s preorder column tree over `CSharp` with Roslyn's diagnostics (`Tree`), how a text is
//! parsed (`FileKind`: language version, parser entry, documentation mode) and the reusable `Parser`.

use std::ops::Deref;
use std::sync::Arc;

use psi_builder::TreeBuilder;
use psi_builder::stack::{StackGuard, StackUse};

pub use psi_builder::tree::{Children, NodeId, Preorder};

use crate::error_code::{ErrorCode, Severity};
use crate::flatten::{Flat, Flattener, Values};
use crate::green::{Green, GreenArena, SyntaxListPool};
use crate::language::CSharp;
use crate::language_version::LanguageVersion;
use crate::lexer::{Lexer, LexerBuffers, LexerMode, LexerOptions, PreprocessorSymbols};
use crate::parser::LanguageParser;
use crate::syntax_kind::{SyntaxKind, TokenKind};
use crate::syntax_parser::SyntaxParserBuffers;

/// `DocumentationMode`: whether documentation comments are parsed into structured trivia (`Parse`, the default of
/// `CSharpParseOptions`) or lexed as comments (`None`); `Diagnose` also reports their XML errors.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub enum DocumentationMode {
    None,
    #[default]
    Parse,
    Diagnose,
}

impl DocumentationMode {
    /// The name of a list line's DOC column: `parse`, `none`, `diagnose`.
    pub fn from_name(name: &str) -> Option<DocumentationMode> {
        Some(match name {
            "parse" => DocumentationMode::Parse,
            "none" => DocumentationMode::None,
            "diagnose" => DocumentationMode::Diagnose,
            _ => return None,
        })
    }
}

/// The parser entry a text is parsed with: a file (`CSharpSyntaxTree.ParseText`), or, for the goldens of Roslyn's
/// parsing tests, one of `SyntaxFactory`'s `Parse*` methods with `consumeFullText: true`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Entry {
    /// `ParseCompilationUnit`, a `.cs` file.
    #[default]
    CompilationUnit,
    #[doc(hidden)]
    Expression,
    #[doc(hidden)]
    Statement,
    /// `ParseMemberDeclaration` (null when nothing resembles a member).
    #[doc(hidden)]
    Member,
    #[doc(hidden)]
    TypeName,
    /// `ParseName`, which takes no options: it parses at `CSharpParseOptions.Default` whatever the file kind says.
    #[doc(hidden)]
    Name,
    #[doc(hidden)]
    ParameterList,
    #[doc(hidden)]
    BracketedParameterList,
    #[doc(hidden)]
    ArgumentList,
    #[doc(hidden)]
    BracketedArgumentList,
    #[doc(hidden)]
    AttributeArgumentList,
    /// `SyntaxFactory.ParseTokens`: the lexer's tokens, not a parse (`Parser::lex`); its dump is `dump::lex_to_string`.
    #[doc(hidden)]
    Tokens,
}

impl Entry {
    /// The entry of a list line's ENTRY column, as `tools/csharp-oracle` names it (`compilation-unit`, `expression`,
    /// `member`, `type-name`, ...).
    pub fn from_name(name: &str) -> Option<Entry> {
        Some(match name {
            "compilation-unit" => Entry::CompilationUnit,
            "expression" => Entry::Expression,
            "statement" => Entry::Statement,
            "member" => Entry::Member,
            "type-name" => Entry::TypeName,
            "name" => Entry::Name,
            "parameter-list" => Entry::ParameterList,
            "bracketed-parameter-list" => Entry::BracketedParameterList,
            "argument-list" => Entry::ArgumentList,
            "bracketed-argument-list" => Entry::BracketedArgumentList,
            "attribute-argument-list" => Entry::AttributeArgumentList,
            "tokens" => Entry::Tokens,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Entry::CompilationUnit => "compilation-unit",
            Entry::Expression => "expression",
            Entry::Statement => "statement",
            Entry::Member => "member",
            Entry::TypeName => "type-name",
            Entry::Name => "name",
            Entry::ParameterList => "parameter-list",
            Entry::BracketedParameterList => "bracketed-parameter-list",
            Entry::ArgumentList => "argument-list",
            Entry::BracketedArgumentList => "bracketed-argument-list",
            Entry::AttributeArgumentList => "attribute-argument-list",
            Entry::Tokens => "tokens",
        }
    }
}

/// How a text is parsed: `CSharpParseOptions(languageVersion, documentationMode, SourceCodeKind.Regular)`, through an
/// entry; the preprocessor symbols are `Parser::parse_with_symbols`' (none for `Parser::parse`). The default is a file at `LanguageVersion.Latest` with documentation
/// comments parsed, as `CSharpSyntaxTree.ParseText(text)` parses it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct FileKind {
    pub version: LanguageVersion,
    #[doc(hidden)]
    pub entry: Entry,
    pub doc_mode: DocumentationMode,
}

impl FileKind {
    /// A file at `version`, documentation comments parsed.
    pub fn file(version: LanguageVersion) -> FileKind {
        FileKind { version, ..FileKind::default() }
    }
}

impl FileKind {
    /// The lexer's options of this kind with `symbols` defined (`ParseName` ignores the kind's and the symbols: it
    /// parses at the defaults).
    pub(crate) fn lexer_options(self, symbols: &PreprocessorSymbols) -> LexerOptions<'_> {
        if self.entry == Entry::Name {
            return LexerOptions::default();
        }
        LexerOptions { version: self.version, doc_mode: self.doc_mode, preprocessor_symbols: symbols }
    }
}

/// A diagnostic of a tree: the node it is attached to, its code, its span in bytes and its English message, as
/// `SyntaxTree.GetDiagnostics()` reports it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    /// The tree node (a node, a token leaf, a trivia leaf or a structured trivia's root) the diagnostic is on.
    pub node: NodeId,
    pub code: ErrorCode,
    /// The location's `SourceSpan`, UTF-8 byte offsets.
    pub start: u32,
    pub end: u32,
    /// `GetMessage(CultureInfo.InvariantCulture)`, a lone surrogate (half of a supplementary character, from a token
    /// of one or a code unit the lexer reports) as U+FFFD.
    pub message: String,
    /// The lone surrogates of `message`: the byte offset of their U+FFFD and the UTF-16 code unit, in order (the dump
    /// prints them as `\u{XXXX}`, as the oracle prints .NET's string).
    pub lone_surrogates: Vec<(u32, u16)>,
}

impl Diagnostic {
    /// `Id`: `CS` and the code's number in four digits (`CS1002`).
    pub fn id(&self) -> String {
        format!("CS{:04}", self.code.number())
    }

    pub fn severity(&self) -> Severity {
        self.code.severity()
    }
}

/// The boolean fields of a directive's node (`syntax::types::IfDirectiveTriviaSyntax::IS_ACTIVE` and the same bits of
/// every directive type): `IsActive` (the directive is read: it is in no branch the preprocessor does not take), and for
/// `#if`, `#elif` and `#else` `BranchTaken` (the branch after it is taken) and, but for `#else`, `ConditionValue`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct DirectiveFlags(u8);

impl DirectiveFlags {
    /// `DirectiveTriviaSyntax.IsActive`.
    pub fn is_active(self) -> bool {
        u32::from(self.0) & crate::syntax::types::IfDirectiveTriviaSyntax::IS_ACTIVE != 0
    }

    /// `BranchTaken` of an `#if`, `#elif` or `#else`; false for every other directive.
    pub fn branch_taken(self) -> bool {
        u32::from(self.0) & crate::syntax::types::IfDirectiveTriviaSyntax::BRANCH_TAKEN != 0
    }

    /// `ConditionValue` of an `#if` or `#elif`; false for every other directive.
    pub fn condition_value(self) -> bool {
        u32::from(self.0) & crate::syntax::types::IfDirectiveTriviaSyntax::CONDITION_VALUE != 0
    }
}

/// A parsed C# text: `psi_builder`'s tree (`Deref`: ids in preorder, `kind`, `range`, `children`, `leaves`, ...,
/// `error_message(id)`, the first message on a node), its diagnostics in Roslyn's order and its tokens' value texts
/// (`value_text`).
pub struct Tree {
    tree: psi_builder::Tree<CSharp>,
    diagnostics: Box<[Diagnostic]>,
    /// The token leaves whose `ValueText` is not their text, in id order, and the end of each one's value in
    /// `value_strings` (`flatten::Values`).
    values: Box<[(u32, u32)]>,
    value_strings: Box<str>,
    /// Each directive's node id and its boolean fields, in id order (`directives`).
    directives: Box<[(u32, u8)]>,
    /// The entry returned null (`ParseMemberDeclaration` of text that is no member): the tree is an empty root.
    no_result: bool,
    /// The tokens of `Parser::lex` (`Entry::Tokens`) as the root's children, not a parse.
    lexed: bool,
}

impl Deref for Tree {
    type Target = psi_builder::Tree<CSharp>;

    fn deref(&self) -> &psi_builder::Tree<CSharp> {
        &self.tree
    }
}

/// The token kinds whose `ValueText` is a value the port does not keep (a number's, a character's, a string's
/// content): `Tree::value_text` has none for them, unless the lexer kept it (a directive's string) or the token is
/// missing. `tools/csharp-oracle`'s `values --kept` leaves out the same kinds (`Values.cs`, `NotKept`).
fn value_not_kept(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::NumericLiteralToken
            | TokenKind::CharacterLiteralToken
            | TokenKind::StringLiteralToken
            | TokenKind::Utf8StringLiteralToken
            | TokenKind::SingleLineRawStringLiteralToken
            | TokenKind::MultiLineRawStringLiteralToken
            | TokenKind::Utf8SingleLineRawStringLiteralToken
            | TokenKind::Utf8MultiLineRawStringLiteralToken
            | TokenKind::InterpolatedStringToken
            | TokenKind::InterpolatedStringTextToken
    )
}

impl Tree {
    /// The tree of what a walk made, with the entry's flags.
    fn of(flat: Flat, no_result: bool, lexed: bool) -> Tree {
        Tree {
            tree: flat.tree,
            diagnostics: flat.diagnostics.into_boxed_slice(),
            values: flat.values.ends.into_boxed_slice(),
            value_strings: flat.values.text.into_boxed_str(),
            directives: flat.directives.into_boxed_slice(),
            no_result,
            lexed,
        }
    }

    /// Parses `text` with a fresh `Parser` (keep a `Parser` per thread to reuse its buffers).
    pub fn parse(text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        Parser::once().parse(text, file_kind)
    }

    /// The tree of the green `root` of `arena` over `source` (tests build green trees by hand).
    #[doc(hidden)]
    pub fn from_green(arena: &GreenArena, root: Green, source: &[u8], file_kind: FileKind) -> Tree {
        let source: Arc<[u8]> = Arc::from(source);
        let mut builder = TreeBuilder::new();
        Tree::of(Flattener::default().flatten(arena, root, &source, &mut builder, file_kind, false), false, false)
    }

    /// The tree of lexed `tokens` of `arena` as the children of a root, as `Parser::lex` builds it (tests).
    #[doc(hidden)]
    pub fn from_tokens(arena: &GreenArena, tokens: &[Green], source: &[u8], file_kind: FileKind) -> Tree {
        let source: Arc<[u8]> = Arc::from(source);
        let mut builder = TreeBuilder::new();
        Tree::of(Flattener::default().flatten_tokens(arena, tokens, &source, &mut builder, file_kind), false, true)
    }

    /// `GetDiagnostics()`, in Roslyn's order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// `SyntaxToken.ValueText` of the token leaf `id`: the identifier's name without a verbatim `@` and with its
    /// unicode escapes (`\uXXXX`, `\UXXXXXXXX`) decoded (`@class` is `class`; in a documentation comment's name
    /// attribute the `@` is part of the value, as in Roslyn: `<param name="@a"/>` is `@a`), an XML entity's character
    /// (`&lt;` is `<`), a cref's `{` and `}` as `<` and `>`, a directive string's content (`#line 1 "a.cs"`); else the
    /// token's text (keywords, punctuation, XML text, an identifier without escapes, a missing token's empty text,
    /// a missing literal's included). `None` for a node, a trivia leaf, a token whose text is not UTF-8, and a literal
    /// whose value the port does not keep: every numeric literal (also in a directive: `#line 010` and `#pragma
    /// warning disable 0168` have none), and character, string (but a directive's), raw string and interpolated
    /// text tokens; their `ValueText` is the value, which no tree shows. Only values that differ from the text are
    /// stored, in a table searched by id.
    pub fn value_text(&self, id: NodeId) -> Option<&str> {
        let kind = self.tree.token_kind(id)?;
        if crate::dump::is_trivia(kind) {
            return None;
        }
        if let Ok(i) = self.values.binary_search_by_key(&id.0, |&(leaf, _)| leaf) {
            let start = if i == 0 { 0 } else { self.values[i - 1].1 as usize };
            return self.value_strings.get(start..self.values[i].1 as usize);
        }
        // a missing token's `ValueText` is its `Text`, empty (`MissingTokenWithTrivia`), whatever its kind
        if self.tree.is_missing(id) {
            return Some("");
        }
        if value_not_kept(kind) {
            return None;
        }
        self.tree.utf8_text(id)
    }

    /// Every directive of the tree (`DirectiveTriviaSyntax`: `#if`, `#elif`, `#else`, `#endif`, `#define`, `#region`, ...,
    /// the root of its structured trivia), in document order, with the boolean fields Roslyn's node has
    /// (`DirectiveFlags`): what the lexer decided for the preprocessor symbols the tree was parsed with. A directive in a
    /// branch Roslyn does not take is one too, inactive (Roslyn lexes the directives of disabled text for their nesting).
    pub fn directives(&self) -> impl ExactSizeIterator<Item = (NodeId, DirectiveFlags)> + '_ {
        self.directives.iter().map(|&(id, bits)| (NodeId(id), DirectiveFlags(bits)))
    }

    /// `directives`' flags of the node `id`, None when it is no directive.
    pub fn directive_flags(&self, id: NodeId) -> Option<DirectiveFlags> {
        let i = self.directives.binary_search_by_key(&id.0, |&(d, _)| d).ok()?;
        Some(DirectiveFlags(self.directives[i].1))
    }

    /// The tree without its diagnostics.
    pub fn as_psi(&self) -> &psi_builder::Tree<CSharp> {
        &self.tree
    }

    /// Whether the entry returned null (`Entry::Member` on text that is no member): there is nothing to dump.
    #[doc(hidden)]
    pub fn has_no_result(&self) -> bool {
        self.no_result
    }

    /// Whether the tree holds the lexer's tokens (`Parser::lex`, `Entry::Tokens`) rather than a parse.
    #[doc(hidden)]
    pub fn is_lexed(&self) -> bool {
        self.lexed
    }

    /// The heap bytes of the tree, its diagnostics and its value texts.
    pub fn heap_bytes(&self) -> usize {
        self.tree.heap_bytes()
            + self.diagnostics.len() * size_of::<Diagnostic>()
            + self.diagnostics.iter().map(|d| d.message.capacity()).sum::<usize>()
            + self.values.len() * size_of::<(u32, u32)>()
            + self.value_strings.len()
            + self.directives.len() * size_of::<(u32, u8)>()
    }
}

/// A reusable C# parser: owns the green arena, the lexer's and the parsers' buffers and the tree builder, cleared and
/// reused from one file to the next. Keep one per thread; `Tree::parse` makes a fresh one per call. After a parse
/// whose buffers outgrew the retained bytes (`psi_builder::buffers::RETAINED_BYTES` by default,
/// `set_retained_bytes`) they are shrunk to half of them (`shrink_to`).
pub struct Parser {
    arena: GreenArena,
    lexer: LexerBuffers,
    syntax_parser: SyntaxParserBuffers,
    pool: SyntaxListPool,
    builder: TreeBuilder<CSharp>,
    flattener: Flattener,
    /// What `set_arena_budget` set in place of `green::budget_for`.
    arena_budget: Option<usize>,
    /// What `set_retained_bytes` set.
    retained_bytes: usize,
    /// `heap_bytes` when the last parse ended, before it shrank to `retained_bytes`.
    grown_bytes: usize,
    /// The deepest `_recursionDepth` and the most stack the stack checks of the last parse saw (`stack_use`).
    stack_use: StackUse,
}

impl Default for Parser {
    fn default() -> Parser {
        Parser {
            arena: GreenArena::default(),
            lexer: LexerBuffers::default(),
            syntax_parser: SyntaxParserBuffers::default(),
            pool: SyntaxListPool::default(),
            builder: TreeBuilder::default(),
            flattener: Flattener::default(),
            arena_budget: None,
            retained_bytes: psi_builder::buffers::RETAINED_BYTES,
            grown_bytes: 0,
            stack_use: StackUse::default(),
        }
    }
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// A parser for one parse, which it never shrinks after (its buffers go with it).
    fn once() -> Parser {
        Parser { retained_bytes: usize::MAX, ..Parser::default() }
    }

    /// The heap bytes the parser's buffers hold between parses, by capacity: the green arena, the lexer's buffers
    /// (with those of the directive, documentation comment and interpolated string parsers it runs), the token
    /// window, the list builders and the flattener's stacks; the tree builder's columns, while a tree is built (they
    /// become the tree's).
    pub fn heap_bytes(&self) -> usize {
        self.arena.heap_bytes()
            + self.lexer.heap_bytes()
            + self.syntax_parser.heap_bytes()
            + self.pool.heap_bytes()
            + self.builder.heap_bytes()
            + self.flattener.heap_bytes()
    }

    /// Shrinks the buffers to at most `bytes` when they hold more (`heap_bytes`; the directive store's empty stack
    /// keeps one record, and a vector of nested buffers its first element's inline bytes, `Shrink::nested`): each is
    /// emptied and keeps the same share of its capacity (`psi_builder::buffers::Shrink`), so the proportions of the
    /// files that grew them stay. `shrink_to(0)` frees them. A parse after it grows what it needs again.
    pub fn shrink_to(&mut self, bytes: usize) {
        let total = self.heap_bytes();
        if total <= bytes {
            return;
        }
        let shrink = psi_builder::buffers::Shrink::new(bytes, total);
        self.arena.shrink(shrink);
        self.lexer.shrink(shrink);
        self.syntax_parser.shrink(shrink);
        self.pool.shrink(shrink);
        self.builder.shrink(shrink);
        self.flattener.shrink(shrink);
    }

    /// The most the buffers keep from one file to the next (`psi_builder::buffers::RETAINED_BYTES` by default): after a
    /// parse that leaves more than `bytes` in them, they are shrunk to half of it
    /// (`shrink_to(psi_builder::buffers::retained_after(bytes))`); after one that leaves `bytes` or less, they are kept
    /// as they are. `usize::MAX` keeps all, `0` frees all after every file.
    pub fn set_retained_bytes(&mut self, bytes: usize) {
        self.retained_bytes = bytes;
    }

    /// `heap_bytes` when the last parse ended, before it shrank to the retained bytes (measurements).
    #[doc(hidden)]
    pub fn grown_bytes(&self) -> usize {
        self.grown_bytes
    }

    /// The end of a parse: the bytes it grew to; past the retained bytes, the buffers shrunk to half of them
    /// (`buffers::retained_after`).
    fn retain(&mut self) {
        self.grown_bytes = self.heap_bytes();
        if self.grown_bytes > self.retained_bytes {
            self.shrink_to(psi_builder::buffers::retained_after(self.retained_bytes));
        }
    }

    /// Parses with an arena budget of `bytes` instead of `green::budget_for` of the text (tests: a small budget puts
    /// the abandoned parse on ordinary inputs). Not part of the consumer contract.
    #[doc(hidden)]
    pub fn set_arena_budget(&mut self, bytes: usize) {
        self.arena_budget = Some(bytes);
    }

    /// The tokens the parsers of the last parse consumed, again after every reset (measurements).
    #[doc(hidden)]
    pub fn work_tokens(&self) -> u64 {
        self.arena.work()
    }

    /// The deepest `_recursionDepth` and the most stack the stack checks of the last parse saw (see
    /// `psi_builder::stack`; measurements and tests).
    #[doc(hidden)]
    pub fn stack_use(&self) -> StackUse {
        self.stack_use
    }

    /// Parses `text` as `file_kind` says (`Entry::Tokens`: lexes it, `lex`), no preprocessor symbol defined.
    pub fn parse(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        self.parse_with_symbols(text, file_kind, PreprocessorSymbols::none())
    }

    /// Parses `text` as `file_kind` says with `symbols` defined (`CSharpParseOptions.WithPreprocessorSymbols`): an
    /// `#if` reads a name the file neither `#define`s nor `#undef`s before it as defined when `symbols` holds it.
    pub fn parse_with_symbols(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind, symbols: &PreprocessorSymbols) -> Tree {
        if file_kind.entry == Entry::Tokens {
            return self.lex_with_symbols(text, file_kind, symbols);
        }
        let source: Arc<[u8]> = Arc::from(text.as_ref());
        let mut arena = std::mem::take(&mut self.arena);
        arena.clear();
        arena.set_budget(self.arena_budget.unwrap_or_else(|| crate::green::budget_for(source.len())));
        arena.set_work_budget(crate::green::work_budget_for(source.len()));
        let mut lexer = Lexer::new(&source, file_kind.lexer_options(symbols), arena, std::mem::take(&mut self.lexer));
        let buffers = std::mem::take(&mut self.syntax_parser);
        let pool = std::mem::take(&mut self.pool);
        let (root, abandoned, buffers, pool) = {
            let mut parser = LanguageParser::new(&mut lexer, LexerMode::SYNTAX, buffers, pool, StackGuard::here());
            let root = parser.parse_entry(file_kind.entry);
            let abandoned = parser.stack_exhausted_at.is_some() || parser.base.budget_exhausted_at.is_some();
            let bytes = parser.stack.used_to(parser.lowest_frame);
            self.stack_use = StackUse { levels: parser.peak_recursion_depth as u32, bytes };
            let (buffers, pool) = parser.into_buffers();
            (root, abandoned, buffers, pool)
        };
        self.syntax_parser = buffers;
        self.pool = pool;
        let (arena, lexer_buffers) = lexer.into_parts();
        self.lexer = lexer_buffers;
        self.finish(arena, root, &source, file_kind, abandoned)
    }

    /// The lexer's tokens of `text` in `LexerMode.Syntax` up to and with the end of file (`SyntaxFactory.ParseTokens`),
    /// as the children of a root, with their diagnostics in token order: what `ksdump --lex` prints.
    pub fn lex(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        self.lex_with_symbols(text, file_kind, PreprocessorSymbols::none())
    }

    /// `lex` with `symbols` defined (`parse_with_symbols`).
    pub fn lex_with_symbols(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind, symbols: &PreprocessorSymbols) -> Tree {
        let source: Arc<[u8]> = Arc::from(text.as_ref());
        let mut arena = std::mem::take(&mut self.arena);
        arena.clear();
        let mut lexer = Lexer::new(&source, file_kind.lexer_options(symbols), arena, std::mem::take(&mut self.lexer));
        let mut tokens: Vec<Green> = Vec::new();
        loop {
            let position = lexer.position();
            let token = lexer.lex(LexerMode::SYNTAX);
            tokens.push(token);
            let eof = lexer.arena_ref().kind(token) == SyntaxKind::EndOfFileToken;
            // a lexer that does not move would loop forever: end there
            if eof || (lexer.position() == position && lexer.arena_ref().full_width(token) == 0) {
                break;
            }
        }
        let (arena, lexer_buffers) = lexer.into_parts();
        self.lexer = lexer_buffers;
        let flat = self.flattener.flatten_tokens(&arena, &tokens, &source, &mut self.builder, file_kind);
        self.arena = arena;
        self.retain();
        Tree::of(flat, false, true)
    }

    /// The number of tokens `lex` makes of `text`, without a tree: the lexer alone, what `ksdump --lex --bench` times.
    pub fn lex_count(&mut self, text: &[u8], file_kind: FileKind) -> usize {
        let mut arena = std::mem::take(&mut self.arena);
        arena.clear();
        let mut lexer = Lexer::new(text, file_kind.lexer_options(PreprocessorSymbols::none()), arena, std::mem::take(&mut self.lexer));
        let mut count = 0;
        loop {
            let position = lexer.position();
            let token = lexer.lex(LexerMode::SYNTAX);
            count += 1;
            let eof = lexer.arena_ref().kind(token) == SyntaxKind::EndOfFileToken;
            if eof || (lexer.position() == position && lexer.arena_ref().full_width(token) == 0) {
                break;
            }
        }
        let (arena, lexer_buffers) = lexer.into_parts();
        self.lexer = lexer_buffers;
        self.arena = arena;
        self.retain();
        count
    }

    fn finish(
        &mut self,
        arena: GreenArena,
        root: Green,
        source: &Arc<[u8]>,
        file_kind: FileKind,
        abandoned: bool,
    ) -> Tree {
        let no_result = root.is_null();
        let flat = if no_result {
            let id = self.builder.open(None, psi_builder::ElementType::Node(crate::NodeKind::CompilationUnit), 0);
            self.builder.close(id, 0);
            let tree = self.builder.finish(source.clone(), file_kind);
            Flat { tree, diagnostics: Vec::new(), values: Values::default(), directives: Vec::new() }
        } else {
            self.flattener.flatten(&arena, root, source, &mut self.builder, file_kind, abandoned)
        };
        self.arena = arena;
        self.retain();
        Tree::of(flat, no_result, false)
    }
}

// A tree is shared across threads as is, a parser moves to the thread that keeps it.
const _: () = {
    const fn send_sync<T: Send + Sync + 'static>() {}
    const fn send<T: Send + 'static>() {}
    send_sync::<Tree>();
    send::<Parser>();
};
