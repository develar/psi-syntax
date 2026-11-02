//! A lossless Java lexer and parser, ported from IntelliJ's java-syntax (`com.intellij.java.syntax`, see AGENTS.md):
//! `lexer` and `doc_lexer` are `JavaLexer` and `JavaDocLexer` (complete), `parser` the parsers (ported but for
//! `JavaParserUtil.stoppingBuilder(builder, condition)`, which nothing calls), `dump` the two dumps the Java parsing tests compare. The builder, the
//! tree and the parse driver are `psi_builder`'s, over the `Java` language.

pub mod builder;
pub mod bundle;
pub mod doc_lexer;
pub mod dump;
mod language;
pub mod language_level;
pub mod lexer;
pub mod node_kind;
pub mod parser;
pub mod syntax_element_types;
pub mod tokens;
pub mod tree;
mod unicode;

pub use language::{Java, parse_fragment};
pub use language_level::{JavaFeature, LanguageLevel};
pub use node_kind::NodeKind;
/// The builder, tree and driver this crate is built on, for generic code over languages.
pub use psi_builder;
pub use tokens::{ElementType, TokenKind, TokenSet};
pub use tree::{Entry, FileKind, LazyKind, NodeId, ParseOptions, Parser, Tree};
