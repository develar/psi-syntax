//! A lossless Kotlin lexer and parser, ported from the Kotlin compiler (see AGENTS.md): `Tree::parse` builds the tree
//! PSI builds, `dump::psi_to_string` prints what `DebugUtil.psiToString(file, true, false)` prints. The builder, the
//! tree and the parse driver are `psi_builder`'s, over the `Kotlin` language.

pub mod builder;
pub mod counters;
pub mod dump;
pub mod kdoc;
mod language;
pub mod lexer;
pub mod node_kind;
pub mod parsing;
pub mod tokens;
pub mod tree;
mod unicode;

pub use language::Kotlin;
pub use node_kind::NodeKind;
/// The builder, tree and driver this crate is built on, for generic code over languages.
pub use psi_builder;
pub use tokens::{ElementType, TokenKind, TokenSet};
pub use tree::{FileKind, LeafIndex, NodeId, ParseOptions, Parser, Tree, document_text};
