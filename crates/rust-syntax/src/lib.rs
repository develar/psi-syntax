//! Rust read by rust-analyzer's parser, the parse each consumer of psi-syntax shares: accio's extraction and Branch
//! Deck's summaries, highlighting and navigation read one tree of one parse of one text, and each walks it its own way
//! (decision 2026-11-08-rust-and-js-parse-entries of accio). rust-analyzer's grammar is the language's own, published
//! weekly as `ra_ap_syntax`: lossless, error-resilient, with a typed AST that says what each node is, so what a node
//! declares is `ra_ap_syntax::ast`'s answer and no consumer keeps a list of node kinds. This crate holds what is not
//! the grammar's: the text the parser reads (`utf8_text`), the edition (`EDITION`), the tree built where it is dropped
//! (`parse`, `parse_with_errors`, `parse_items`), the stack the parser's recursion takes (`guarded`, `nesting`), where
//! an item's own text starts (`start_after_attributes`), and, behind the feature `macros`, a `macro_rules!` expanded as
//! rust-analyzer expands it (`macros`).
//!
//! `ra_ap_syntax` and `rowan` are re-exported, so a consumer names the tree's types through this crate and builds with
//! its pins.

#[cfg(feature = "macros")]
pub mod macros;
pub mod nesting;

pub use ra_ap_syntax;
pub use rowan;

use ra_ap_syntax::{Edition, SourceFile, SyntaxError, SyntaxNode, TextRange, TextSize};

/// The edition a file is parsed with. 2021, not 2024: 2024 makes `gen` a keyword, and crates of the older editions
/// that name a function, a field or a module `gen` (`rand`'s `Rng::gen`) read as broken items. An edition-2015 crate
/// that names a function `try` or calls `try!` reads as broken under 2021 too (objc 0.2's `exception::try`, 2 files of
/// the 6 703 of accio's crates corpus, its decision 62). A crate's own edition is its manifest's, which only a reader of
/// the repository knows; a blob cache keys on it then, since one blob parses differently under two.
pub const EDITION: Edition = Edition::Edition2021;

/// The text the parser reads: `src` itself when it is UTF-8 with LF line ends and no byte order mark, else `src`
/// with each byte of an invalid sequence and of a leading byte order mark replaced by a space and the `\r` of each
/// CRLF by `\n`, so every offset of the tree is an offset of `src` (a lossy `\u{FFFD}` is three bytes and moved every
/// offset after it). rustc rejects a file that is no UTF-8, and skips the mark, which rust-analyzer's parser reads as
/// a stray token; the extraction still reads what parses around the bytes.
pub fn utf8_text(src: &[u8]) -> std::borrow::Cow<'_, str> {
    const BOM: &[u8] = b"\xEF\xBB\xBF";
    let crlf = memchr::memmem::find(src, b"\r\n").is_some();
    let bom = src.starts_with(BOM);
    match std::str::from_utf8(src) {
        Ok(text) if !crlf && !bom => std::borrow::Cow::Borrowed(text),
        _ => {
            let mut bytes = Vec::with_capacity(src.len());
            for chunk in src.utf8_chunks() {
                bytes.extend_from_slice(chunk.valid().as_bytes());
                bytes.extend(std::iter::repeat_n(b' ', chunk.invalid().len()));
            }
            if bom {
                bytes[..BOM.len()].fill(b' ');
            }
            // rustc reads a CRLF line end as LF: a string's `\` continues on the next line (`"a\<CR><LF>b"`), which
            // rust-analyzer's validation, reading `\<CR>`, calls an unknown escape. The `\r` becomes a second `\n`.
            if crlf {
                for i in memchr::memmem::find_iter(src, b"\r\n") {
                    bytes[i] = b'\n';
                }
            }
            std::borrow::Cow::Owned(String::from_utf8(bytes).expect("valid UTF-8 and spaces"))
        }
    }
}

/// `work`, a parse of `src` and the walks of its tree and its drop, where the stack holds the parser's recursion
/// (`nesting::run`): None for a file nested deeper than any code (`nesting::MAX_LEVELS`), which every caller answers as
/// a file that does not parse. Every parse of a consumer runs inside one, the reads of the tree and its drop too: the
/// drop of a rowan tree recurses as deep as the tree, and an overflow aborts the process past any `catch_unwind`.
pub fn guarded<R: Send>(src: &[u8], work: impl FnOnce() -> R + Send) -> Option<R> {
    nesting::run(&utf8_text(src), work)
}

/// The tree of `src`, without its errors: rust-analyzer's parser and the tree builder below. Run under `guarded`, but
/// for a test's small input.
pub fn parse(src: &[u8]) -> SyntaxNode {
    let (green, _) = build(&utf8_text(src));
    SyntaxNode::new_root(green)
}

/// The tree of `src` and its syntax errors in source order: the lexer's, the parser's and those of rust-analyzer's
/// validation, which rejects what parses and rustc does not accept (`let` outside a condition, a bad escape). A file
/// nested deeper than `nesting::INLINE_LEVELS` reports the lexer's and the parser's errors without the validation,
/// whose `Parse` drops its tree on a thread of rust-analyzer's with a 2 MiB stack. Run under `guarded`.
#[allow(clippy::disallowed_methods)] // the one parse that validates (clippy.toml)
pub fn parse_with_errors(src: &[u8]) -> (SyntaxNode, Vec<SyntaxError>) {
    let text = utf8_text(src);
    let (root, mut errors) = if nesting::levels(&text) <= nesting::INLINE_LEVELS {
        let parse = SourceFile::parse(&text, EDITION);
        (parse.syntax_node(), parse.errors())
    } else {
        let (green, errors) = build(&text);
        (SyntaxNode::new_root(green), errors)
    };
    errors.sort_by_key(|e| e.range().start());
    (root, errors)
}

/// The items the body of an item macro writes (`cfg_net! { pub struct Socket { io: PollEvented<u32> } }`, tokio's
/// gates), read as a file of its own: None for a body that is no list of items without an error
/// (`lazy_static! { static ref X: T = …; }`, an expression), whose meaning only its macro knows. Its offsets are those
/// of `text`. Run under `guarded` with the file around it.
pub fn parse_items(text: &str) -> Option<SyntaxNode> {
    let (green, errors) = build(text);
    errors.is_empty().then(|| SyntaxNode::new_root(green))
}

/// rust-analyzer's parse of `text` into a green tree, with the lexer's and the parser's errors: what
/// `SourceFile::parse` does before its validation, without the `Parse` that holds the result. A `Parse` drops its tree
/// on one thread of rust-analyzer's, `ParseNodeDropper`, whose 2 MiB stack the recursive drop of a deep tree overflows
/// (one 20 000 blocks deep aborted the process as the parse itself did, `nesting`); this tree is dropped where it is
/// built, on a stack `nesting::run` sized for the file. The tree is built node by node (`Builder`), never through
/// rowan's `GreenNodeBuilder`.
#[allow(clippy::disallowed_methods)] // the one parse that builds its own tree (clippy.toml)
fn build(text: &str) -> (ra_ap_syntax::GreenNode, Vec<SyntaxError>) {
    use ra_ap_parser::StrStep;
    use rowan::Language;
    let kind = ra_ap_syntax::RustLanguage::kind_to_raw;
    let lexed = ra_ap_parser::LexedStr::new(EDITION, text);
    let output = ra_ap_parser::TopEntryPoint::SourceFile.parse(&lexed.to_input(EDITION));
    let mut builder = Builder::default();
    let mut errors = Vec::new();
    lexed.intersperse_trivia(&output, &mut |step| match step {
        StrStep::Token { kind: k, text } => {
            builder.children.push(rowan::NodeOrToken::Token(rowan::GreenToken::new(kind(k), text)))
        }
        StrStep::Enter { kind: k } => builder.parents.push((kind(k), builder.children.len())),
        StrStep::Exit => builder.finish_node(),
        StrStep::Error { msg, pos } => {
            errors.push(SyntaxError::new_at_offset(msg.to_owned(), TextSize::new(pos as u32)))
        }
    });
    for (i, msg) in lexed.errors() {
        let range = lexed.text_range(i);
        errors.push(SyntaxError::new(
            msg.to_owned(),
            TextRange::new(TextSize::new(range.start as u32), TextSize::new(range.end as u32)),
        ));
    }
    (builder.finish(), errors)
}

/// A green tree built from the parser's events, each node from its children as it closes: what rowan's
/// `GreenNodeBuilder` does without its `NodeCache`. The cache shares the nodes of three children or fewer, and each
/// time its table grows it hashes every node it holds again through the whole subtree (`node_cache::node_hash`), which
/// on a file of small nodes nested 40 000 deep (parentheses, `Vec<…>`) took minutes.
#[derive(Default)]
struct Builder {
    /// The open nodes: the kind of each, and where its children start in `children`.
    parents: Vec<(rowan::SyntaxKind, usize)>,
    children: Vec<rowan::NodeOrToken<rowan::GreenNode, rowan::GreenToken>>,
}

impl Builder {
    fn finish_node(&mut self) {
        let (kind, first) = self.parents.pop().expect("an Exit after its Enter");
        let node = rowan::GreenNode::new(kind, self.children.drain(first..));
        self.children.push(rowan::NodeOrToken::Node(node));
    }

    fn finish(mut self) -> ra_ap_syntax::GreenNode {
        assert!(
            self.parents.is_empty() && self.children.len() == 1,
            "the parser closes every node it opens, the file last"
        );
        match self.children.pop() {
            Some(rowan::NodeOrToken::Node(node)) => node,
            _ => unreachable!("the file is a node"),
        }
    }
}

/// Where a declaration or an expression starts: its first token after the outer attributes and the doc and plain
/// comments before it, which rust-analyzer keeps inside the node (`#[test]\n/// Doc\npub fn f()` starts at `pub`,
/// `#[cfg(x)] f(1)` at `f`). A reader of a declaration's
/// attributes reads them backward from there.
pub fn start_after_attributes(node: &ra_ap_syntax::SyntaxNode) -> usize {
    use ra_ap_syntax::SyntaxKind as K;
    node.children_with_tokens()
        .find(|e| !matches!(e.kind(), K::ATTR | K::DOC_COMMENT | K::COMMENT | K::WHITESPACE))
        .map_or(usize::from(node.text_range().start()), |e| usize::from(e.text_range().start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every offset of the tree is an offset of the file: a byte that is no UTF-8 and a byte order mark are spaces, a
    /// CRLF's `\r` a second `\n`.
    #[test]
    fn the_text_keeps_every_offset() {
        assert_eq!(utf8_text(b"fn f() {}"), "fn f() {}");
        assert_eq!(utf8_text(b"\xEF\xBB\xBFfn \xFFf() {}\r\n"), "   fn  f() {}\n\n");
        let root = parse(b"/// doc\n#[test]\nfn f() {}\n");
        let item = root.first_child().unwrap();
        assert_eq!(start_after_attributes(&item), 16);
        assert_eq!(parse_items("pub struct S;").map(|it| it.to_string()), Some("pub struct S;".to_string()));
        assert_eq!(parse_items("static ref X: T = 1;"), None);
        let (_, errors) = parse_with_errors(b"fn f() { let }");
        assert!(!errors.is_empty());
    }
}
