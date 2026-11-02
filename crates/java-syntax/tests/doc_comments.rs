//! `JavaDocParser` alone, before the file parsers: every golden of `testdata/java/manifest.tsv` is parsed flat (all
//! tokens under the file root, no parser), so its doc comments are expanded as chameleons by `JavaDocParser`, and each
//! `PsiDocComment` subtree of our dump is compared with the golden's, in order. Prints the counts and the first
//! difference of each mismatch; fails with `JAVA_STRICT=1` or `KS_STRICT=1` (the gate's) unless all compared comments match.

use std::path::{Path, PathBuf};

use java_syntax::dump::psi_to_string;
use java_syntax::{Entry, FileKind, LanguageLevel, NodeKind, Parser};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java")
}

/// The text a golden parses: line separators made `\n`, trimmed (`String.trim()`) when the test trims it.
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

/// The `PsiDocComment` subtrees of a dump, each with its lines de-indented to the comment's own.
fn doc_comments(dump: &str) -> Vec<Vec<String>> {
    let lines: Vec<&str> = dump.lines().collect();
    let mut comments = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let indent = line.len() - line.trim_start().len();
        if !line.trim_start().starts_with("PsiDocComment") {
            i += 1;
            continue;
        }
        let mut comment = vec![line[indent..].to_string()];
        i += 1;
        while i < lines.len() {
            let inner = lines[i];
            let inner_indent = inner.len() - inner.trim_start().len();
            if inner_indent <= indent {
                break;
            }
            comment.push(inner[indent..].to_string());
            i += 1;
        }
        comments.push(comment);
    }
    comments
}

#[test]
fn doc_comments_of_the_goldens() {
    let root = testdata();
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).expect("read manifest.tsv");
    let (mut files, mut compared, mut equal) = (0, 0, 0);
    for line in manifest.lines().filter(|l| !l.is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let path = columns[0];
        let level = LanguageLevel::from_name(columns[1]).unwrap_or(LanguageLevel::HIGHEST);
        let text = source_text(&root.join(path), columns[3] == "1");
        let want_dump = std::fs::read_to_string(root.join(path.replace(".java", ".txt"))).expect("read .txt");
        let want = doc_comments(&want_dump);
        if want.is_empty() {
            continue;
        }
        files += 1;
        let name = path.rsplit('/').next().unwrap_or(path);
        let tree = Parser::new().parse_with(&text, FileKind { level, entry: Entry::File }, |mut builder, _| {
            let file = builder.mark();
            while !builder.eof() {
                builder.advance_lexer();
            }
            builder.done(file, NodeKind::JAVA_FILE);
            builder
        });
        let got_dump = psi_to_string(&tree, name);
        let got = doc_comments(&got_dump);
        for (k, want_comment) in want.iter().enumerate() {
            compared += 1;
            let Some(got_comment) = got.get(k) else {
                println!("MISSING {path} comment {k}");
                continue;
            };
            if got_comment == want_comment {
                equal += 1;
            } else {
                let at = want_comment.iter().zip(got_comment).position(|(w, g)| w != g);
                match at {
                    Some(at) => println!(
                        "MISMATCH {path} comment {k} line {}: want {:?} got {:?}",
                        at + 1,
                        want_comment[at],
                        got_comment[at]
                    ),
                    None => {
                        println!("MISMATCH {path} comment {k}: {} vs {} lines", want_comment.len(), got_comment.len())
                    }
                }
            }
        }
    }
    println!("java doc comments: {equal}/{compared} equal in {files} goldens");
    if ["JAVA_STRICT", "KS_STRICT"].iter().any(|k| std::env::var_os(k).is_some_and(|v| v == "1")) {
        assert_eq!(equal, compared);
    }
}
