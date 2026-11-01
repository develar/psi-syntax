//! Every golden of `testdata/java/manifest.tsv` parsed at its level through its entry: `dump::psi_to_string` against
//! `NAME.txt` and `dump::node_tree_to_string` against `NAME_node.txt`, byte for byte. A parse that reaches a parser
//! method not yet ported (`TODO(port)`, a `todo!` panic) counts apart. Prints the counts and the first differing line
//! of each mismatch; fails with `JAVA_STRICT=1` or `KS_STRICT=1` (the gate's) unless all match.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use java_syntax::dump::{node_tree_to_string, psi_to_string};
use java_syntax::{Entry, FileKind, LanguageLevel, Parser};

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

fn first_difference(want: &str, got: &str) -> String {
    for (i, (w, g)) in want.lines().zip(got.lines()).enumerate() {
        if w != g {
            return format!("line {}: want {w:?} got {g:?}", i + 1);
        }
    }
    format!("lengths: {} vs {} lines", want.lines().count(), got.lines().count())
}

#[test]
fn goldens() {
    let root = testdata();
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).expect("read manifest.tsv");
    // the stubs' panics are counted, not printed
    std::panic::set_hook(Box::new(|_| {}));
    let (mut total, mut psi, mut node, mut unported) = (0, 0, 0, 0);
    for line in manifest.lines().filter(|l| !l.is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let path = columns[0];
        let level = LanguageLevel::from_name(columns[1]).unwrap_or(LanguageLevel::HIGHEST);
        let entry = Entry::from_name(columns[2]).unwrap_or(Entry::File);
        let text = source_text(&root.join(path), columns[3] == "1");
        let name = path.rsplit('/').next().unwrap_or(path);
        total += 1;
        let parsed = catch_unwind(AssertUnwindSafe(|| {
            let tree = Parser::new().parse(&text, FileKind { level, entry });
            (psi_to_string(&tree, name), node_tree_to_string(&tree))
        }));
        let Ok((psi_dump, node_dump)) = parsed else {
            unported += 1;
            continue;
        };
        let want_psi = std::fs::read_to_string(root.join(path.replace(".java", ".txt"))).expect("read .txt");
        let want_node = std::fs::read_to_string(root.join(path.replace(".java", "_node.txt"))).expect("read _node.txt");
        if psi_dump == want_psi.trim_end() {
            psi += 1;
        } else {
            println!("MISMATCH {path} (.txt): {}", first_difference(want_psi.trim_end(), &psi_dump));
        }
        if node_dump == want_node.trim_end() {
            node += 1;
        } else {
            println!("MISMATCH {path} (_node.txt): {}", first_difference(want_node.trim_end(), &node_dump));
        }
    }
    let _ = std::panic::take_hook();
    println!(
        "java goldens: {psi}/{total} .txt, {node}/{total} _node.txt ({unported} reach a parser method not yet ported)"
    );
    if ["JAVA_STRICT", "KS_STRICT"].iter().any(|k| std::env::var_os(k).is_some_and(|v| v == "1")) {
        assert_eq!((psi, node), (total, total));
    }
}
