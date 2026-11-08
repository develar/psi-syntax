//! Lossless Starlark syntax, ported from IntelliJ's Bazel plugin.

pub mod dump;
pub mod language;
pub mod lexer;
pub mod node_kind;
pub mod parser;
pub mod psi;
pub mod string;
pub mod token_sets;
pub mod tokens;
pub mod tree;

pub use language::Starlark;
pub use node_kind::NodeKind;
pub use tokens::{ElementType, TokenKind};
pub use tree::{FileKind, LeafIndex, NodeId, Parser, Tree, document_text};
