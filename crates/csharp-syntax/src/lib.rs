//! A lossless C# lexer and parser, ported from Roslyn's (`Microsoft.CodeAnalysis.CSharp` 5.9.0, dotnet/roslyn
//! 35d9211b, see AGENTS.md, "C#"), whose tree is byte-equal to Roslyn's syntax tree: its nodes, tokens, trivia, missing
//! tokens and diagnostics, at UTF-8 byte offsets. The parsers build Roslyn's green tree in an arena (`green`), which is
//! flattened into `psi_builder`'s preorder column tree over the `CSharp` language.
//!
//! ```
//! use csharp_syntax::{FileKind, LanguageVersion, NodeKind, Parser, TokenKind};
//!
//! let mut parser = Parser::new(); // one per thread: it reuses its buffers from file to file
//! let tree = parser.parse("class C { int x = 1_000 }", FileKind::default()); // Latest, doc comments parsed
//! let class = tree.descendants(tree.root()).find(|&n| tree.node_kind(n) == Some(NodeKind::ClassDeclaration));
//! assert!(class.is_some());
//! // a token, its trivia beside it: `is_trailing` says which token a trivia leaf belongs to
//! let space = tree.tokens().find(|&t| tree.token_kind(t) == Some(TokenKind::WhitespaceTrivia)).unwrap();
//! assert!(tree.is_trailing(space)); // the space after `class`
//! // the parser inserted the missing `;`: an empty leaf, and Roslyn's diagnostic with its English message
//! assert!(tree.tokens().any(|t| tree.is_missing(t) && tree.token_kind(t) == Some(TokenKind::SemicolonToken)));
//! assert_eq!(tree.diagnostics()[0].id(), "CS1002");
//! assert_eq!(tree.diagnostics()[0].message, "; expected");
//! // the language version decides the feature checks, as `CSharpParseOptions.LanguageVersion` does
//! let old = parser.parse("class C { int x = 1_000; }", FileKind::file(LanguageVersion::CSharp6));
//! assert_eq!(old.diagnostics()[0].id(), "CS8059");
//! ```
//!
//! The consumer contract (AGENTS.md): a `Parser` per thread (`Send`), a `Tree` that is `Send + Sync`, owns its text and
//! derefs to `psi_builder::Tree<CSharp>` (ids in preorder, `kind`, `range`, `children`, `leaves`, `descendants`, ...);
//! a node's range is its full span, a token a leaf between its leading and trailing trivia, structured trivia
//! (directives, documentation comments, skipped tokens) a subtree; `Tree::diagnostics` is `GetDiagnostics()` in
//! Roslyn's order, `Tree::value_text` a token's `ValueText` (`@class` is `class`). A `Parser` keeps at most its
//! retained bytes of buffers between files (`set_retained_bytes`, `shrink_to`, `heap_bytes`). Any bytes parse without a panic on a 1 MB stack; past Roslyn's nesting, which a fixed depth counter
//! replaces, and past the work and memory budgets, the tree is the one Roslyn returns on an exhausted stack
//! (`ERR_InsufficientStack`). docs/DESIGN.md, "C#", "C# nesting and work" and "C# deviations", says how and where it
//! differs from Roslyn.
//!
//! Reading a tree: `syntax_getters`, the properties of Roslyn's syntax nodes as free functions over the tree and a
//! node's id (`class_declaration::identifier`, `type_declaration::members`, `member_declaration::modifiers`,
//! `has_modifier`, `name::get_unqualified_name`), each the answer Roslyn's property gives on every node of the gate's
//! inputs; `read`, what consumers read beyond them (trivia, `SyntaxNode.Span`, a name's identifier and dotted name, the
//! names a type spells).
//!
//! ```
//! use csharp_syntax::syntax_getters::{class_declaration, has_modifier, type_declaration};
//! use csharp_syntax::{FileKind, NodeKind, Tree, TokenKind};
//!
//! let tree = Tree::parse("public static class C<T> { void M() { } }", FileKind::default());
//! let class = tree.descendants(tree.root()).find(|&n| tree.node_kind(n) == Some(NodeKind::ClassDeclaration)).unwrap();
//! let name = class_declaration::identifier(&tree, class).unwrap();
//! assert_eq!(tree.value_text(name), Some("C"));
//! assert!(has_modifier(&tree, class, TokenKind::StaticKeyword));
//! assert_eq!(type_declaration::arity(&tree, class), 1);
//! assert_eq!(type_declaration::members(&tree, class).count(), 1);
//! ```
//!
//! Generated from Roslyn's sources (`tools/csharp-gen/gen.py`, `tools/csharp-unicode`): `syntax_kind` (`SyntaxKind`,
//! `TokenKind`, `NodeKind`), `syntax` (the node types of `Syntax.xml` and their green factories), `syntax_getters`'s
//! `fields` (a getter per field of Syntax.xml), `error_code` (the diagnostics, their messages and the features'
//! versions), `unicode` (the lexer's character classes). Ported by hand:
//! `facts` (`SyntaxKindFacts.cs`), `language_version` (`LanguageVersion.cs`), `green` (`GreenNode` and the internal
//! syntax's copy semantics), `flatten` (green to `psi_builder::Tree<CSharp>`, as Roslyn's red tree), `tree`, `dump`
//! (`tools/csharp-oracle`'s format), `diagnostic` (`DiagnosticInfo.GetMessage`), `syntax_parser` (`SyntaxParser.cs`),
//! `parser` (`LanguageParser*.cs`, `DocumentationCommentParser.cs`) and `lexer` (`Lexer.cs` and its partial files,
//! `SlidingTextWindow.cs`, `CharacterInfo.cs`, `Directives.cs`, `DirectiveParser.cs`). Also generated: `xml_char_type`
//! (`XmlCharType.cs`'s tables).

pub mod diagnostic;
pub mod dump;
pub mod error_code;
pub mod facts;
mod flatten;
pub mod green;
mod language;
pub mod language_version;
pub mod lexer;
pub mod parser;
pub mod preprocessor;
pub mod read;
pub mod syntax;
pub mod syntax_getters;
pub mod syntax_kind;
pub mod syntax_parser;
pub mod tree;
mod xml_char_type;
// The lexer's character classes.
mod unicode;

pub use error_code::{ErrorCode, MessageID, Severity, XmlParseErrorCode};
pub use language::CSharp;
pub use language_version::LanguageVersion;
/// The tree builder this crate's trees are made with, for generic code over languages.
pub use psi_builder;
pub use syntax_kind::{NodeKind, SyntaxKind, TokenKind};
pub use lexer::PreprocessorSymbols;
pub use tree::{Diagnostic, DirectiveFlags, DocumentationMode, Entry, FileKind, NodeId, Parser, Tree};
