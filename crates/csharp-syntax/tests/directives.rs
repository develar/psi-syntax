//! `Tree::directives` against Roslyn's `DirectiveTriviaSyntax.IsActive`, `BranchingDirectiveTriviaSyntax.BranchTaken`
//! and `ConditionalDirectiveTriviaSyntax.ConditionValue`: the goldens against the oracle's `directives` lines of them
//! (`tools/csharp-oracle/oracle.sh directives`, README "Directives"); the corpora are the gate's (`ksdump --directives
//! --check`).

mod common;

use csharp_syntax::dump;

/// The goldens of `testdata/csharp` (each at its manifest line's version, entry, documentation mode and preprocessor
/// symbols) against `directives/goldens.directives.txt`, what `tools/csharp-oracle/oracle.sh directives` printed for
/// them (the manifest's lines with `.cs` added to the path, `--root testdata/csharp`).
#[test]
fn golden_directive_lines_equal_the_oracle() {
    let Some(ours) = common::golden_lines(dump::directive_lines) else {
        println!("no testdata/csharp/manifest.tsv, nothing to compare");
        return;
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/directives");
    let oracle = std::fs::read_to_string(dir.join("goldens.directives.txt")).expect("the oracle's lines");
    common::assert_same_lines(&ours, &oracle);
    // every flag both ways: an inactive `#if` whose condition holds, an `#elif` that holds after a taken branch
    for flags in ["A--", "---", "ATV", "AT-", "A-V", "--V"] {
        assert!(oracle.lines().any(|l| l.ends_with(&format!("\t{flags}"))), "no {flags} line");
    }
}
