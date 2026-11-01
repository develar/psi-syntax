//! The doc comment subtrees of the goldens: our KDoc lexer, KDoc parser, markdown-link parser and builder against
//! every `KDoc` subtree of the expected dumps, in order. Independent of the Kotlin parser: the tree expands each doc
//! comment token wherever it lands. Fails on any difference.

mod common;

use kotlin_syntax::dump::subtree_to_string;
use kotlin_syntax::tokens::{ElementType, TokenKind};
use kotlin_syntax::{FileKind, Tree};

/// The `KDoc` subtrees of a dump, dedented.
fn expected_kdocs(dump: &str) -> Vec<String> {
    let lines: Vec<&str> = dump.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let indent = lines[i].len() - lines[i].trim_start_matches(' ').len();
        if lines[i].trim_start_matches(' ') == "KDoc" {
            let mut subtree = vec![&lines[i][indent..]];
            let mut j = i + 1;
            while j < lines.len() && lines[j].len() - lines[j].trim_start_matches(' ').len() > indent {
                subtree.push(&lines[j][indent..]);
                j += 1;
            }
            out.push(subtree.join("\n"));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn kdoc_subtrees() {
    let mut files = 0;
    let mut subtrees = 0;
    let mut failures = Vec::new();
    for golden in common::goldens() {
        let expected = expected_kdocs(&common::expected(&golden));
        if expected.is_empty() {
            continue;
        }
        files += 1;
        let text = common::source_text(&golden);
        let tree = Tree::parse(text.as_bytes(), FileKind::from_file_name(&common::file_name(&golden)));
        let ours: Vec<String> = tree
            .preorder(tree.root())
            .filter(|&(id, _)| tree.kind(id) == ElementType::Token(TokenKind::DOC_COMMENT))
            .map(|(id, _)| subtree_to_string(&tree, id))
            .collect();
        if ours.len() != expected.len() {
            failures.push(format!("{}: {} doc comments, expected {}", golden.name, ours.len(), expected.len()));
            continue;
        }
        for (k, (a, e)) in ours.iter().zip(&expected).enumerate() {
            if a == e {
                subtrees += 1;
                continue;
            }
            let (al, el): (Vec<&str>, Vec<&str>) = (a.lines().collect(), e.lines().collect());
            let line = (0..al.len().max(el.len())).find(|&i| al.get(i) != el.get(i)).unwrap_or(0);
            failures.push(format!(
                "{} doc comment {k} line {}: want {:?} got {:?}",
                golden.name,
                line + 1,
                el.get(line),
                al.get(line)
            ));
        }
    }
    for f in &failures {
        println!("FAIL {f}");
    }
    println!("kdoc: {subtrees} subtrees equal in {files} goldens with doc comments, {} failures", failures.len());
    assert!(failures.is_empty());
}
