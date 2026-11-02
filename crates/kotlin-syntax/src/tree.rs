//! The Kotlin tree: `psi_builder`'s preorder column tree and parse driver over Kotlin's types (`Tree`, `Parser`), how
//! a file is parsed (`FileKind`) and what each chameleon is reparsed with (`LazyKind`): a doc comment with the KDoc
//! lexer and parser, a markdown link inside it with the Kotlin lexer and `KDocLinkParser`, a collapsed block or lambda
//! with `KotlinParser.parseBlockExpression` / `parseLambdaExpression` over its text.

use crate::Kotlin;
#[cfg(test)]
use crate::builder::SemanticWhitespaceAwarePsiBuilderImpl;

pub use psi_builder::tree::{Children, NodeId, ParseOptions, Preorder};

/// A parsed Kotlin file.
pub type Tree = psi_builder::Tree<Kotlin>;

/// A reusable Kotlin parser: owns the buffers every builder of a parse works in (the file's tokens, a chameleon's
/// tokens, the marker pools, the productions, the state stacks), cleared and reused from one file to the next instead
/// of freed. Keep one per thread; `Tree::parse` makes a fresh one per call.
pub type Parser = psi_builder::Parser<Kotlin>;

/// How a file is parsed: `KotlinParser.parse` runs `parseFile` for `.kt` and `parseScript` for anything else.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FileKind {
    Kt,
    Kts,
}

impl FileKind {
    /// `FileUtilRt.getExtension(name)` empty or `kt` is a file, any other extension a script (case-sensitive).
    pub fn from_file_name(name: &str) -> FileKind {
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        match base.rfind('.') {
            None => FileKind::Kt,
            Some(dot) => {
                let extension = &base[dot + 1..];
                if extension.is_empty() || extension == "kt" { FileKind::Kt } else { FileKind::Kts }
            }
        }
    }
}

/// What a collapsed or lazy element is reparsed with when the tree expands it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LazyKind {
    /// `BlockExpressionElementType`: `KotlinParser.parseBlockExpression`.
    Block,
    /// `LambdaExpressionElementType`: `KotlinParser.parseLambdaExpression`.
    Lambda,
    /// `KDocTokens.KDOC`: `KDocLexer` + `KDocParser`.
    KDoc,
    /// `KDocTokens.MARKDOWN_LINK`: `KotlinLexer` + `KDocLinkParser`.
    KDocLink,
}

/// `Parser::parse_with` on a fresh parser with `parse` run on the semantic layer (tests drive the builder by hand).
#[cfg(test)]
pub(crate) fn parse_with(
    text: &[u8],
    file_kind: FileKind,
    parse: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilderImpl),
) -> Tree {
    Parser::new().parse_with(text, file_kind, |frame| {
        frame.reset_layer();
        parse(frame);
    })
}

// A tree is shared across threads as is, a parser moves to the thread that keeps it.
const _: () = {
    const fn send_sync<T: Send + Sync + 'static>() {}
    const fn send<T: Send + 'static>() {}
    send_sync::<Tree>();
    send::<Parser>();
};
