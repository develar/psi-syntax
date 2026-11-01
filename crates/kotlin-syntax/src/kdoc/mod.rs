//! KDoc: the lexer of a doc comment's text (`KDoc.flex`, `KDocLexer`) and its parsers (`KDocParser`,
//! `KDocLinkParser`). The tree expands each doc comment token with them, as PSI expands the lazy `KDoc` element.

pub mod lexer;
pub(crate) mod parser;
