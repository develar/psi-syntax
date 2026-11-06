//! `JavaDocParser` alone, before the file parsers: every golden of `testdata/java/manifest.tsv` is parsed flat (all
//! tokens under the file root, no parser), so its doc comments are expanded as chameleons by `JavaDocParser`, and each
//! `PsiDocComment` subtree of our dump is compared with the golden's, in order. Prints the counts and the first
//! difference of each mismatch; fails with `JAVA_STRICT=1` or `KS_STRICT=1` (the gate's) unless all compared comments match.

mod common;

use java_syntax::dump::psi_to_string;
use java_syntax::{Entry, FileKind, NodeKind, Parser};

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
    let (mut files, mut compared, mut equal) = (0, 0, 0);
    for golden in common::goldens() {
        let (path, level) = (golden.path.as_str(), golden.level);
        let text = golden.source_text();
        let want_dump = golden.dump(".txt");
        let want = doc_comments(&want_dump);
        if want.is_empty() {
            continue;
        }
        files += 1;
        let name = golden.name();
        let tree = Parser::new().parse_with(&text, FileKind { level, entry: Entry::File }, |builder| {
            let file = builder.mark();
            while !builder.eof() {
                builder.advance_lexer();
            }
            builder.done(file, NodeKind::JAVA_FILE);
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
    if common::strict() {
        assert_eq!(equal, compared);
    }
}
