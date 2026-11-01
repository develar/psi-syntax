//! Every golden pair of the compiler's parsing tests: our `psi_to_string` against the `.txt`. Prints the pass count
//! and, for each failure, the first differing line. Fails only with `KS_STRICT=1` while the parser is incomplete.

mod common;

use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::{FileKind, Tree};

#[test]
fn goldens() {
    let goldens = common::goldens();
    let mut passed = 0;
    let mut failures = Vec::new();
    for golden in &goldens {
        let text = common::source_text(golden);
        let name = common::file_name(golden);
        let tree = Tree::parse(text.as_bytes(), FileKind::from_file_name(&name));
        let actual = psi_to_string(&tree, &name);
        let actual = actual.trim_end();
        let expected = common::expected(golden);
        if actual == expected {
            passed += 1;
            continue;
        }
        let a: Vec<&str> = actual.lines().collect();
        let e: Vec<&str> = expected.lines().collect();
        let line = (0..a.len().max(e.len())).find(|&i| a.get(i) != e.get(i)).unwrap_or(0);
        failures.push((
            format!(
                "{}:{}: want {:?} got {:?}",
                golden.name,
                line + 1,
                e.get(line).map_or("<end>", |l| l.trim()),
                a.get(line).map_or("<end>", |l| l.trim())
            ),
            common::is_known_stale(golden),
        ));
    }
    for (failure, _) in &failures {
        println!("FAIL {failure}");
    }
    let stale = failures.iter().filter(|(_, stale)| *stale).count();
    println!("goldens: {passed}/{} pass ({stale} failing known stale, see common::KNOWN_STALE)", goldens.len());
    assert_eq!(goldens.len(), 789, "the vendored goldens");
    if std::env::var_os("KS_STRICT").is_some_and(|v| v == "1") {
        let real = failures.len() - stale;
        assert_eq!(real, 0, "{real} of {} goldens differ", goldens.len());
    }
}
