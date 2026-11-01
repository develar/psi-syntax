//! Any bytes parse without a panic and the tree covers them: random bytes, random mixes of Kotlin punctuation and
//! keywords, and every golden cut at many points.

mod common;

use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::{FileKind, Tree};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// The leaves of the tree, in order, are the whole text.
fn check_lossless(text: &[u8], kind: FileKind) {
    let tree = Tree::parse(text, kind);
    let mut covered = Vec::with_capacity(text.len());
    for id in tree.tokens() {
        covered.extend_from_slice(tree.text(id));
    }
    assert_eq!(covered, text, "{:?}", String::from_utf8_lossy(text));
    assert_eq!(tree.range(tree.root()), 0..text.len());
    let _ = psi_to_string(&tree, "a.kt");
}

#[test]
fn random_bytes_and_fragments_parse() {
    let mut rng = XorShift(0x9E3779B97F4A7C15);
    let pieces: &[&[u8]] = &[
        b"\"",
        b"\"\"\"",
        b"$",
        b"${",
        b"{",
        b"}",
        b"`",
        b"/*",
        b"*/",
        b"/**",
        b"'",
        b"\\",
        b"\n",
        b"(",
        b")",
        b"<",
        b">",
        b"fun",
        b"val",
        b"class",
        b"when",
        b"->",
        b"?.",
        b"?:",
        b"!!",
        b"@",
        b"[",
        b"]",
        b"#!",
        b" ",
        b"x",
        b"1.",
        b"0x",
        b"\xFF",
        b"\xC3",
        b"\xE2\x82\xAC",
        b"* @param ",
        b"```\n",
        b"[a.b]",
    ];
    for round in 0..3000 {
        let len = (rng.next() % 200) as usize;
        let mut text = Vec::with_capacity(len * 3);
        for _ in 0..len {
            if round % 3 == 0 {
                text.push(rng.next() as u8);
            } else {
                text.extend_from_slice(pieces[(rng.next() % pieces.len() as u64) as usize]);
            }
        }
        check_lossless(&text, if round % 2 == 0 { FileKind::Kt } else { FileKind::Kts });
    }
    for golden in common::goldens() {
        let text = common::source_text(&golden).into_bytes();
        let step = (text.len() / 7).max(1);
        let mut cut = 0;
        while cut <= text.len() {
            check_lossless(&text[..cut], FileKind::Kt);
            cut += step;
        }
    }
}
