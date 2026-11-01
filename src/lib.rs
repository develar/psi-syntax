//! A lossless Kotlin lexer and parser, ported from the Kotlin compiler (see AGENTS.md): `Tree::parse` builds the tree
//! PSI builds, `dump::psi_to_string` prints what `DebugUtil.psiToString(file, true, false)` prints.

pub mod builder;
pub mod counters;
pub mod dump;
pub mod kdoc;
pub mod lexer;
pub mod node_kind;
pub mod parsing;
pub mod tokens;
pub mod tree;
mod unicode;

pub use node_kind::NodeKind;
pub use tokens::{ElementType, TokenKind, TokenSet};
pub use tree::{FileKind, NodeId, ParseOptions, Parser, Tree};
