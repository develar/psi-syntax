//! The lexers against the goldens: for every golden of `testdata/java/manifest.tsv`, the leaf texts of its PSI dump
//! (`NAME.txt`) must be the texts of our tokens, where a doc comment is its JavaDoc lexer tokens (the granularity of
//! the PSI leaves). Where the parser changes the granularity, a leaf may be:
//! - the concatenation of adjacent tokens (the parser collapses `>` `>` into GTGT, `non` `-` `sealed` into
//!   NON_SEALED, runs of JavaDoc data, markdown link texts and labels);
//! - one of the tokens of a reference or type holder: up to 8 adjacent JavaDoc tokens that JavaDocParser collapses into
//!   `DOC_REFERENCE_HOLDER` / `DOC_TYPE_HOLDER`, whose text the tree relexes with `JavaLexer(JDK_1_3)` (a type's through
//!   `JavaTypeEscapeLexer`).
//!
//! Prints how many goldens match, how many of those needed a join or a holder, and the first mismatch of each failure;
//! fails with `KS_STRICT=1` unless all match.

use std::path::{Path, PathBuf};

use java_syntax::doc_lexer::JavaDocLexer;
use java_syntax::lexer::{self, JavaLexer, Lexed};
use java_syntax::{LanguageLevel, TokenKind};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java")
}

/// `DebugUtil.fixWhiteSpaces`: `\n`, `\r`, `\t` escaped.
fn escape(text: &[u8]) -> String {
    String::from_utf8_lossy(text).replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t")
}

/// The leaf texts of a PSI dump, escaped as the dump escapes them: every line `<label>('<text>')` of a leaf class.
fn leaf_texts(dump: &str) -> Vec<String> {
    const LEAF_LABELS: [&str; 9] = [
        "PsiWhiteSpace",
        "PsiJavaToken:",
        "PsiKeyword:",
        "PsiIdentifier:",
        "PsiDocToken:",
        "PsiComment(",
        "PsiFragment:",
        "PsiElement(",
        "PsiSnippetAttributeValue:",
    ];
    let mut out = Vec::new();
    for line in dump.lines() {
        let content = line.trim_start_matches(' ');
        if !LEAF_LABELS.iter().any(|l| content.starts_with(l)) || !content.ends_with("')") {
            continue;
        }
        if let Some(open) = content.find("('") {
            out.push(content[open + 2..content.len() - 2].to_owned());
        }
    }
    out
}

/// Our tokens at the granularity of PSI leaves, escaped; empty tokens are not leaves.
fn token_texts(text: &[u8], level: LanguageLevel) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    let mut lexer = JavaLexer::new(level, text);
    while let Some((kind, start, end)) = lexer.next_token() {
        let token_text = &text[start..end];
        if matches!(kind, TokenKind::DOC_COMMENT | TokenKind::DOC_MARKDOWN_COMMENT) {
            let mut doc = JavaDocLexer::new(true, token_text);
            while let Some((k, s, e)) = doc.next_token() {
                if s < e {
                    out.push((k, escape(&token_text[s..e])));
                }
            }
        } else if start < end {
            out.push((kind, escape(token_text)));
        }
    }
    out
}

/// The leaves a holder over `text` has: its tokens relexed at JDK_1_3, escaped.
fn holder_leaves(text: &str, type_escaped: bool) -> Vec<String> {
    let unescaped = text.replace("\\n", "\n").replace("\\r", "\r").replace("\\t", "\t");
    let bytes = unescaped.as_bytes();
    let mut lexed = Lexed::default();
    if type_escaped {
        lexer::tokenize_type_escaped_into(bytes, LanguageLevel::JDK_1_3, &mut lexed);
    } else {
        lexer::tokenize_into(bytes, LanguageLevel::JDK_1_3, &mut lexed);
    }
    lexed.starts.windows(2).filter(|w| w[0] < w[1]).map(|w| escape(&bytes[w[0] as usize..w[1] as usize])).collect()
}

#[derive(Default)]
struct Stats {
    joins: usize,
    holders: usize,
}

fn compare(expected: &[String], ours: &[(TokenKind, String)], stats: &mut Stats) -> Result<(), String> {
    let (mut i, mut j) = (0, 0);
    'outer: while i < expected.len() && j < ours.len() {
        if expected[i] == ours[j].1 {
            i += 1;
            j += 1;
            continue;
        }
        let mut joined = ours[j].1.clone();
        for k in 2..=ours.len() - j {
            joined.push_str(&ours[j + k - 1].1);
            if joined == expected[i] {
                stats.joins += 1;
                i += 1;
                j += k;
                continue 'outer;
            }
            if !expected[i].starts_with(&joined) {
                break;
            }
        }
        let mut holder = String::new();
        for k in 1..=8.min(ours.len() - j) {
            holder.push_str(&ours[j + k - 1].1);
            for type_escaped in [false, true] {
                let leaves = holder_leaves(&holder, type_escaped);
                if leaves.len() > 1 && expected.get(i..i + leaves.len()) == Some(&leaves[..]) {
                    stats.holders += 1;
                    i += leaves.len();
                    j += k;
                    continue 'outer;
                }
            }
        }
        return Err(format!("leaf {i}: want {:?} got {:?} ({:?})", expected[i], ours[j].1, ours[j].0));
    }
    if i < expected.len() || j < ours.len() {
        return Err(format!("lengths: {} leaves, {} tokens, stopped at {i}/{j}", expected.len(), ours.len()));
    }
    Ok(())
}

/// The text a golden parses: the file with its line separators made `\n`, trimmed (`String.trim()`) when the test
/// trims it.
fn source_text(path: &Path, trim: bool) -> Vec<u8> {
    let raw = std::fs::read(path).expect("read golden source");
    let mut text = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\r' {
            text.push(b'\n');
            if raw.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
        } else {
            text.push(raw[i]);
        }
        i += 1;
    }
    if trim {
        let start = text.iter().position(|&b| b > b' ').unwrap_or(text.len());
        let end = text.iter().rposition(|&b| b > b' ').map_or(start, |e| e + 1);
        text = text[start..end].to_vec();
    }
    text
}

#[test]
fn lexer_leaf_texts() {
    let root = testdata();
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).expect("read manifest.tsv");
    let (mut total, mut matched, mut with_joins, mut with_holders) = (0, 0, 0, 0);
    for line in manifest.lines().filter(|l| !l.is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let (path, level, trim) = (columns[0], columns[1], columns[3] == "1");
        let level = LanguageLevel::from_name(level).unwrap_or(LanguageLevel::HIGHEST);
        let text = source_text(&root.join(path), trim);
        let dump_path = root.join(path.replace(".java", ".txt"));
        let dump = std::fs::read_to_string(&dump_path).expect("read golden dump");
        total += 1;
        let mut stats = Stats::default();
        match compare(&leaf_texts(&dump), &token_texts(&text, level), &mut stats) {
            Ok(()) => {
                matched += 1;
                with_joins += (stats.joins > 0) as usize;
                with_holders += (stats.holders > 0) as usize;
            }
            Err(e) => println!("MISMATCH {path}: {e}"),
        }
    }
    println!(
        "lexer: {matched}/{total} goldens have their leaf texts ({with_joins} with joined tokens, {with_holders} with relexed holders)"
    );
    if std::env::var_os("KS_STRICT").is_some_and(|v| v == "1") {
        assert_eq!(matched, total);
    }
}
