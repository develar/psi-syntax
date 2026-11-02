//! The language-neutral half of the PSI ports (see AGENTS.md): IntelliJ's `PsiBuilderImpl` with its marker
//! production and edge binders (`builder`), the preorder column tree a builder binds into and its consumer API, with
//! the parse driver that expands chameleons in place (`tree`), the PSI debug dump's layout (`dump`), token sets
//! (`token_set`) and what lexers share (`lexer`). A language (`kotlin-syntax`, `java-syntax`) implements `Language`
//! and brings its tokens, node types, lexers, builder layers and parsers.

pub mod builder;
pub mod counters;
pub mod dump;
pub mod language;
pub mod lexer;
pub mod token_set;
pub mod tree;

pub use builder::{AsPsiBuilder, Marker, Nesting, PsiBuilderImpl, WhitespacesAndCommentsBinder};
pub use language::{ElementType, ElementTypeOf, Language, NodeType, TokenType};
pub use lexer::Lexed;
pub use token_set::ElementTypeSet;
pub use tree::{Chameleon, NodeId, ParseOptions, Parser, Tree};
