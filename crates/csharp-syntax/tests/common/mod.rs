//! What the integration tests share: the goldens of `testdata/csharp` as the oracle's per-file line modes read them,
//! and the entries random inputs are parsed at.

#![allow(dead_code)]

use csharp_syntax::{DocumentationMode, Entry, FileKind, LanguageVersion, Parser, PreprocessorSymbols, Tree};

/// `lines` (`dump::value_lines`, `dump::directive_lines`) of every golden of `testdata/csharp`'s `manifest.tsv`, each
/// parsed at its version, entry, documentation mode and preprocessor symbols and named as the oracle's list names it
/// (the manifest's name with `.cs` added), in manifest order; a golden its entry cannot parse has none. `None` without
/// the manifest.
pub fn golden_lines(lines: fn(&str, &Tree) -> String) -> Option<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/csharp");
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).ok()?;
    let mut parser = Parser::new();
    let mut ours = String::new();
    for line in manifest.lines().filter(|l| !l.trim().is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let kind = FileKind {
            version: LanguageVersion::from_name(columns[1]).expect("a version"),
            entry: Entry::from_name(columns[2]).expect("an entry"),
            doc_mode: DocumentationMode::from_name(columns[3]).expect("a documentation mode"),
        };
        let symbols = PreprocessorSymbols::new(columns[4].split(';').filter(|n| !n.is_empty()));
        let name = format!("{}.cs", columns[0]);
        let tree = parser.parse_with_symbols(std::fs::read(root.join(&name)).expect("a golden"), kind, &symbols);
        if !tree.has_no_result() {
            ours.push_str(&lines(&name, &tree));
        }
    }
    Some(ours)
}

/// Asserts that `ours` and `oracle` hold the same lines, showing the first differing ones.
pub fn assert_same_lines(ours: &str, oracle: &str) {
    let differing: Vec<_> = ours.lines().zip(oracle.lines()).filter(|(a, b)| a != b).take(5).collect();
    assert!(
        differing.is_empty() && ours.lines().count() == oracle.lines().count(),
        "{differing:?}, {} lines of ours, {} of the oracle's",
        ours.lines().count(),
        oracle.lines().count()
    );
}

/// The entries random inputs are parsed at, one a round in turn: a compilation unit twice as often; the first five are
/// those of declarations, statements and expressions, the rest the parameter, argument and name lists.
pub const ENTRIES: [Entry; 12] = [
    Entry::CompilationUnit,
    Entry::CompilationUnit,
    Entry::Expression,
    Entry::Statement,
    Entry::Member,
    Entry::TypeName,
    Entry::Name,
    Entry::ParameterList,
    Entry::BracketedParameterList,
    Entry::ArgumentList,
    Entry::BracketedArgumentList,
    Entry::AttributeArgumentList,
];
