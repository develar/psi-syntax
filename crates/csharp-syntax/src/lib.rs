//! A lossless C# lexer and parser, ported from Roslyn's (`Microsoft.CodeAnalysis.CSharp` 5.9.0, see AGENTS.md), whose
//! tree is byte-equal to Roslyn's syntax tree. The parsers build Roslyn's green tree in an arena (`green`), which is
//! flattened into `psi_builder`'s preorder column tree over the `CSharp` language.
//!
//! Generated from Roslyn's sources (`tools/csharp-gen/gen.py`, `tools/csharp-unicode`): `syntax_kind` (`SyntaxKind`,
//! `TokenKind`, `NodeKind`), `syntax` (the node types of `Syntax.xml` and their green factories), `error_code` (the
//! diagnostics, their messages and the features' versions), `unicode` (the lexer's character classes). Ported by hand:
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
pub mod syntax;
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
pub use tree::{Diagnostic, DocumentationMode, Entry, FileKind, NodeId, Parser, Tree};
