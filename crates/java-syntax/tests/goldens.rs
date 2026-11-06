//! Every golden of `testdata/java/manifest.tsv` parsed at its level through its entry: `dump::psi_to_string` against
//! `NAME.txt` and `dump::node_tree_to_string` against `NAME_node.txt`, byte for byte. Prints the counts and the first differing line
//! of each mismatch; fails with `JAVA_STRICT=1` or `KS_STRICT=1` (the gate's) unless all match.

mod common;

use java_syntax::dump::{node_tree_to_string, psi_to_string};
use java_syntax::{FileKind, Parser};

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
    let (mut total, mut psi, mut node) = (0, 0, 0);
    for golden in common::goldens() {
        let (path, name) = (golden.path.as_str(), golden.name());
        let text = golden.source_text();
        total += 1;
        let tree = Parser::new().parse(&text, FileKind { level: golden.level, entry: golden.entry });
        let (psi_dump, node_dump) = (psi_to_string(&tree, name), node_tree_to_string(&tree));
        let (want_psi, want_node) = (golden.dump(".txt"), golden.dump("_node.txt"));
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
    println!("java goldens: {psi}/{total} .txt, {node}/{total} _node.txt");
    if common::strict() {
        assert_eq!((psi, node), (total, total));
    }
}
