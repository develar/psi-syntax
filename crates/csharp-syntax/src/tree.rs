//! The C# tree: `psi_builder`'s preorder column tree over `CSharp` with Roslyn's diagnostics (`Tree`), how a text is
//! parsed (`FileKind`: language version, parser entry, documentation mode) and the reusable `Parser`.

use std::ops::Deref;
use std::sync::Arc;

use psi_builder::TreeBuilder;

pub use psi_builder::tree::{Children, NodeId, Preorder};

use crate::error_code::{ErrorCode, Severity};
use crate::flatten::Flattener;
use crate::green::{Green, GreenArena, SyntaxListPool};
use crate::language::CSharp;
use crate::language_version::LanguageVersion;
use crate::lexer::{Lexer, LexerBuffers, LexerMode, LexerOptions};
use crate::parser::LanguageParser;
use crate::syntax_kind::SyntaxKind;
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
        }
    }
}

/// How a text is parsed: `CSharpParseOptions(languageVersion, documentationMode, SourceCodeKind.Regular)` with no
/// preprocessor symbols, through an entry. The default is a file at `LanguageVersion.Latest` with documentation
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
    /// The lexer's options of this kind (`ParseName` ignores the kind's: it parses at the defaults).
    pub(crate) fn lexer_options(self) -> LexerOptions {
        if self.entry == Entry::Name {
            return LexerOptions::default();
        }
        LexerOptions { version: self.version, doc_mode: self.doc_mode }
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
    /// `GetMessage(CultureInfo.InvariantCulture)`.
    pub message: String,
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

/// A parsed C# text: `psi_builder`'s tree (`Deref`: ids in preorder, `kind`, `range`, `children`, `leaves`, ...,
/// `error_message(id)`, the first message on a node) and its diagnostics in Roslyn's order.
pub struct Tree {
    tree: psi_builder::Tree<CSharp>,
    diagnostics: Box<[Diagnostic]>,
    /// The entry returned null (`ParseMemberDeclaration` of text that is no member): the tree is an empty root.
    no_result: bool,
}

impl Deref for Tree {
    type Target = psi_builder::Tree<CSharp>;

    fn deref(&self) -> &psi_builder::Tree<CSharp> {
        &self.tree
    }
}

impl Tree {
    /// Parses `text` with a fresh `Parser` (keep a `Parser` per thread to reuse its buffers).
    pub fn parse(text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        Parser::new().parse(text, file_kind)
    }

    /// The tree of the green `root` of `arena` over `source` (tests build green trees by hand).
    #[doc(hidden)]
    pub fn from_green(arena: &GreenArena, root: Green, source: &[u8], file_kind: FileKind) -> Tree {
        let source: Arc<[u8]> = Arc::from(source);
        let mut builder = TreeBuilder::new();
        let (tree, diagnostics) = Flattener::default().flatten(arena, root, &source, &mut builder, file_kind);
        Tree { tree, diagnostics: diagnostics.into_boxed_slice(), no_result: false }
    }

    /// The tree of lexed `tokens` of `arena` as the children of a root, as `Parser::lex` builds it (tests).
    #[doc(hidden)]
    pub fn from_tokens(arena: &GreenArena, tokens: &[Green], source: &[u8], file_kind: FileKind) -> Tree {
        let source: Arc<[u8]> = Arc::from(source);
        let mut builder = TreeBuilder::new();
        let (tree, diagnostics) = Flattener::default().flatten_tokens(arena, tokens, &source, &mut builder, file_kind);
        Tree { tree, diagnostics: diagnostics.into_boxed_slice(), no_result: false }
    }

    /// `GetDiagnostics()`, in Roslyn's order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
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

    /// The heap bytes of the tree and its diagnostics.
    pub fn heap_bytes(&self) -> usize {
        self.tree.heap_bytes()
            + self.diagnostics.len() * size_of::<Diagnostic>()
            + self.diagnostics.iter().map(|d| d.message.capacity()).sum::<usize>()
    }
}

/// A reusable C# parser: owns the green arena, the lexer's and the parsers' buffers and the tree builder, cleared and
/// reused from one file to the next. Keep one per thread; `Tree::parse` makes a fresh one per call.
#[derive(Default)]
pub struct Parser {
    arena: GreenArena,
    lexer: LexerBuffers,
    syntax_parser: SyntaxParserBuffers,
    pool: SyntaxListPool,
    builder: TreeBuilder<CSharp>,
    flattener: Flattener,
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// Parses `text` as `file_kind` says.
    pub fn parse(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        let source: Arc<[u8]> = Arc::from(text.as_ref());
        let mut arena = std::mem::take(&mut self.arena);
        arena.clear();
        let mut lexer = Lexer::new(&source, file_kind.lexer_options(), arena, std::mem::take(&mut self.lexer));
        let buffers = std::mem::take(&mut self.syntax_parser);
        let pool = std::mem::take(&mut self.pool);
        let (root, buffers, pool) = {
            let mut parser = LanguageParser::new(&mut lexer, LexerMode::SYNTAX, buffers, pool);
            let root = parser.parse_entry(file_kind.entry);
            let (buffers, pool) = parser.into_buffers();
            (root, buffers, pool)
        };
        self.syntax_parser = buffers;
        self.pool = pool;
        let (arena, lexer_buffers) = lexer.into_parts();
        self.lexer = lexer_buffers;
        self.finish(arena, root, &source, file_kind)
    }

    /// The lexer's tokens of `text` in `LexerMode.Syntax` up to and with the end of file (`SyntaxFactory.ParseTokens`),
    /// as the children of a root, with their diagnostics in token order: what `ksdump --lex` prints.
    pub fn lex(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        let source: Arc<[u8]> = Arc::from(text.as_ref());
        let mut arena = std::mem::take(&mut self.arena);
        arena.clear();
        let mut lexer = Lexer::new(&source, file_kind.lexer_options(), arena, std::mem::take(&mut self.lexer));
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
        let (tree, diagnostics) = self.flattener.flatten_tokens(&arena, &tokens, &source, &mut self.builder, file_kind);
        self.arena = arena;
        Tree { tree, diagnostics: diagnostics.into_boxed_slice(), no_result: false }
    }

    /// The number of tokens `lex` makes of `text`, without a tree: the lexer alone, what `ksdump --lex --bench` times.
    pub fn lex_count(&mut self, text: &[u8], file_kind: FileKind) -> usize {
        let mut arena = std::mem::take(&mut self.arena);
        arena.clear();
        let mut lexer = Lexer::new(text, file_kind.lexer_options(), arena, std::mem::take(&mut self.lexer));
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
        count
    }

    fn finish(&mut self, arena: GreenArena, root: Green, source: &Arc<[u8]>, file_kind: FileKind) -> Tree {
        let no_result = root.is_null();
        let (tree, diagnostics) = if no_result {
            let id = self.builder.open(None, psi_builder::ElementType::Node(crate::NodeKind::CompilationUnit), 0);
            self.builder.close(id, 0);
            (self.builder.finish(source.clone(), file_kind), Vec::new())
        } else {
            self.flattener.flatten(&arena, root, source, &mut self.builder, file_kind)
        };
        self.arena = arena;
        Tree { tree, diagnostics: diagnostics.into_boxed_slice(), no_result }
    }
}

// A tree is shared across threads as is, a parser moves to the thread that keeps it.
const _: () = {
    const fn send_sync<T: Send + Sync + 'static>() {}
    const fn send<T: Send + 'static>() {}
    send_sync::<Tree>();
    send::<Parser>();
};
