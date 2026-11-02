//! The goldens of Roslyn's own parsing tests (`testdata/csharp`, vendored by `tools/csharp-oracle`'s `goldens` mode):
//! each `NAME.cs` parsed at its manifest line's version, entry and documentation mode, its dump against
//! `NAME.tree.txt`. Prints the pass count and the first differing line of the first failures. The same for the reduced
//! corpus cases of `tests/oracle`, which must all pass.
//!
//! `KS_STRICT=1` (as `scripts/gate.sh` runs it) fails on any differing golden. Without `testdata/csharp/manifest.tsv`
//! the test passes and says so.

use std::path::PathBuf;

use csharp_syntax::dump::tree_to_string;
use csharp_syntax::{DocumentationMode, Entry, FileKind, LanguageVersion, Parser};

/// Whether `KS_STRICT=1` fails on a differing golden: on since the parser is ported (phase 4 of the C# port).
const STRICT_ENABLED: bool = true;

/// How many failures print their first differing line.
const SHOWN: usize = 20;

/// `testdata/csharp`, or the directory `CSHARP_TESTDATA` names (goldens being vendored elsewhere).
fn testdata() -> PathBuf {
    match std::env::var_os("CSHARP_TESTDATA") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/csharp"),
    }
}

/// A manifest line: `NAME<TAB>VERSION<TAB>ENTRY<TAB>DOC<TAB>SOURCE`.
fn file_kind(columns: &[&str]) -> Option<FileKind> {
    Some(FileKind {
        version: LanguageVersion::from_name(columns.get(1)?)?,
        entry: Entry::from_name(columns.get(2)?)?,
        doc_mode: DocumentationMode::from_name(columns.get(3)?)?,
    })
}

/// Parses every file of `dir`'s `manifest.tsv` and compares its dump: the pass count, the total and each failure's
/// first differing line; `None` without a manifest.
fn compare(dir: &std::path::Path) -> Option<(usize, usize, Vec<String>)> {
    let manifest = std::fs::read_to_string(dir.join("manifest.tsv")).ok()?;
    let mut parser = Parser::new();
    let (mut passed, mut total) = (0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for line in manifest.lines().filter(|l| !l.trim().is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let name = columns[0];
        total += 1;
        let Some(kind) = file_kind(&columns) else {
            failures.push(format!("{name}: cannot read the manifest line"));
            continue;
        };
        let Ok(text) = std::fs::read(dir.join(format!("{name}.cs"))) else {
            failures.push(format!("{name}: no {name}.cs"));
            continue;
        };
        let expected = std::fs::read_to_string(dir.join(format!("{name}.tree.txt"))).ok();
        let tree = parser.parse(&text, kind);
        let ours = if tree.has_no_result() { None } else { Some(tree_to_string(&tree)) };
        if ours == expected {
            passed += 1;
            continue;
        }
        let first = match (&ours, &expected) {
            (Some(o), Some(e)) => {
                let at = o
                    .lines()
                    .zip(e.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or(o.lines().count().min(e.lines().count()));
                format!(
                    "line {}: ours `{}`, Roslyn's `{}`",
                    at + 1,
                    o.lines().nth(at).unwrap_or("<end>"),
                    e.lines().nth(at).unwrap_or("<end>")
                )
            }
            (None, _) => "we parsed nothing".to_string(),
            (_, None) => "Roslyn parsed nothing".to_string(),
        };
        failures.push(format!("{name}: {first}"));
    }
    Some((passed, total, failures))
}

#[test]
fn goldens() {
    let Some((passed, total, failures)) = compare(&testdata()) else {
        println!("goldens: no testdata/csharp/manifest.tsv, nothing to compare");
        return;
    };
    println!("goldens: {passed} of {total} pass");
    for f in failures.iter().take(SHOWN) {
        println!("  {f}");
    }
    let strict = std::env::var("KS_STRICT").is_ok_and(|v| v == "1");
    assert!(!(STRICT_ENABLED && strict) || failures.is_empty(), "{} goldens differ", failures.len());
}

/// The cases a corpus found, reduced (`tests/oracle`: `NAME.cs` and the oracle's `NAME.tree.txt`, made with
/// `tools/csharp-oracle/oracle.sh dump` at its `manifest.tsv` line): each must equal Roslyn's dump.
#[test]
fn regressions() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let (passed, total, failures) = compare(&dir).expect("tests/oracle/manifest.tsv");
    println!("regressions: {passed} of {total} pass");
    assert!(failures.is_empty(), "{failures:#?}");
}
