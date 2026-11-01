//! Small inputs where the port once differed from the live PSI oracle, each `tests/oracle/NAME.kt` beside the
//! oracle's dump `NAME.txt` (`PsiOracle.java` over the file as it is, unmodified): our `psi_to_string` must equal it.

use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::{FileKind, Tree};
use std::path::Path;

#[test]
fn oracle_cases() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let mut sources: Vec<_> = std::fs::read_dir(&dir)
        .expect("tests/oracle")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "kt" || e == "kts"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty());
    let mut failures = Vec::new();
    for source in &sources {
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let text = std::fs::read(source).expect("read source");
        let expected = std::fs::read_to_string(source.with_extension("txt")).expect("read the oracle dump");
        let actual = psi_to_string(&Tree::parse(&text, FileKind::from_file_name(&name)), &name);
        if actual.trim_end() != expected.trim_end() {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "differ from the oracle: {failures:?}");
}
