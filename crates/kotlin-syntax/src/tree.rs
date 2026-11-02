//! The Kotlin tree: `psi_builder`'s preorder column tree and parse driver over Kotlin's types (`Tree`, `Parser`), how
//! a file is parsed (`FileKind`) and what each chameleon is reparsed with (`LazyKind`): a doc comment with the KDoc
//! lexer and parser, a markdown link inside it with the Kotlin lexer and `KDocLinkParser`, a collapsed block or lambda
//! with `KotlinParser.parseBlockExpression` / `parseLambdaExpression` over its text.

use std::borrow::Cow;

use crate::Kotlin;
#[cfg(test)]
use crate::builder::SemanticWhitespaceAwarePsiBuilderImpl;
use psi_builder::document::{blank_byte_order_mark, has_byte_order_mark};

pub use psi_builder::tree::{Children, LeafIndex, NodeId, ParseOptions, Preorder};

/// The text the compiler and IntelliJ parse of a file's bytes, at the same offsets (`psi_builder::document`): a UTF-8
/// byte order mark as three spaces, the `\r` of a `\r\n` as a space and a lone `\r` as the `\n` it stands for. The
/// lexer reads a `\r` as no line end, so a CRLF file parsed as it is loses every newline-sensitive construct
/// (`val x =⏎ f()` its initializer); `src` itself when it holds neither.
pub fn document_text(src: &[u8]) -> Cow<'_, [u8]> {
    if !has_byte_order_mark(src) && !src.contains(&b'\r') {
        return Cow::Borrowed(src);
    }
    let mut text = src.to_vec();
    blank_byte_order_mark(&mut text);
    for (i, &b) in src.iter().enumerate() {
        if b == b'\r' {
            text[i] = if src.get(i + 1) == Some(&b'\n') { b' ' } else { b'\n' };
        }
    }
    Cow::Owned(text)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(tree: &Tree) -> Vec<Option<crate::NodeKind>> {
        tree.descendants(tree.root()).map(|id| tree.node_kind(id)).collect()
    }

    #[test]
    fn the_document_text_reads_a_mark_and_carriage_returns_as_the_compiler() {
        let cases: [(&[u8], &[u8]); 3] = [
            (b"\xEF\xBB\xBFpackage a\nval x =\n  f()\n", b"   package a\nval x =\n  f()\n"),
            (b"package a\r\nval x =\r\n  f()\r\n", b"package a \nval x = \n  f() \n"),
            (b"package a\rval x =\r  f()\r", b"package a\nval x =\n  f()\n"),
        ];
        let lf = Tree::parse("package a\nval x =\n  f()\n", FileKind::Kt);
        for (src, expected) in cases {
            assert_ne!(
                kinds(&Tree::parse(src, FileKind::Kt)),
                kinds(&lf),
                "{src:?} parsed as it is reads as the compiler"
            );
            let text = document_text(src);
            assert_eq!(&*text, expected);
            let tree = Tree::parse(&*text, FileKind::Kt);
            assert_eq!(tree.errors().len(), 0, "{src:?}");
            if !has_byte_order_mark(src) {
                assert_eq!(kinds(&tree), kinds(&lf), "{src:?}");
            }
        }
        assert!(matches!(document_text(b"val x = 1\n"), Cow::Borrowed(_)));
    }
}
