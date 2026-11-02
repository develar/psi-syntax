//! A lossless C# lexer and parser, ported from Roslyn's (`Microsoft.CodeAnalysis.CSharp` 5.9.0, see AGENTS.md), whose
//! tree is byte-equal to Roslyn's syntax tree. The parsers build Roslyn's green tree in an arena (`green`), which is
//! flattened into `psi_builder`'s preorder column tree over the `CSharp` language.
//!
//! Generated from Roslyn's sources (`tools/csharp-gen/gen.py`, `tools/csharp-unicode`): `syntax_kind` (`SyntaxKind`,
//! `TokenKind`, `NodeKind`), `syntax` (the node types of `Syntax.xml` and their green factories), `error_code` (the
//! diagnostics, their messages and the features' versions), `unicode` (the lexer's character classes). Ported by hand:
//! `facts` (`SyntaxKindFacts.cs`), `language_version` (`LanguageVersion.cs`), `green` (`GreenNode` and the internal
//! syntax's copy semantics).

pub mod error_code;
pub mod facts;
pub mod green;
pub mod language_version;
pub mod syntax;
pub mod syntax_kind;
// The lexer's character classes; the lexer port reads them.
#[allow(dead_code)]
mod unicode;

pub use error_code::{ErrorCode, MessageID, Severity, XmlParseErrorCode};
pub use language_version::LanguageVersion;
pub use syntax_kind::{NodeKind, SyntaxKind, TokenKind};
