mod common;

use psi_check::{check_leaves_are_text, check_links};
use starlark_syntax::dump::psi_to_string;
use starlark_syntax::{FileKind, Parser};

#[test]
fn intellij_parser_goldens() {
    let goldens = common::goldens();
    let mut parser = Parser::new();
    let mut failures = Vec::new();
    let mut passed = 0;
    for golden in &goldens {
        let text = common::source_text(golden);
        let file_kind = FileKind::from_path(&golden.name).expect("Starlark test file kind");
        let tree = parser.parse(text.as_bytes(), file_kind);
        check_leaves_are_text(&tree, text.as_bytes());
        check_links(&tree, |_, _| {});
        let actual = psi_to_string(&tree);
        let expected = common::expected(golden);
        if actual.trim_end() == expected {
            passed += 1;
            continue;
        }
        let actual: Vec<&str> = actual.lines().collect();
        let expected: Vec<&str> = expected.lines().collect();
        let line =
            (0..actual.len().max(expected.len())).find(|&index| actual.get(index) != expected.get(index)).unwrap_or(0);
        failures.push(format!(
            "{}:{}: want {:?}, got {:?}",
            golden.name,
            line + 1,
            expected.get(line).map_or("<end>", |line| line.trim()),
            actual.get(line).map_or("<end>", |line| line.trim()),
        ));
    }
    for failure in &failures {
        println!("FAIL {failure}");
    }
    println!("Starlark parser goldens: {passed}/{} pass", goldens.len());
    assert_eq!(goldens.len(), 23, "vendored Starlark parser golden pairs");
    assert!(failures.is_empty(), "{} of {} Starlark parser goldens differ", failures.len(), goldens.len());
}
