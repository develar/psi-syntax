use std::path::{Path, PathBuf};

/// One vendored case of the Bazel plugin's parser tests: its source and the PSI dump it must print.
pub struct Golden {
    pub name: String,
    pub source: PathBuf,
    pub expected: PathBuf,
}

pub fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/starlark/parser")
}

/// Every `.bzl` under `testdata/starlark/parser` that has a `.txt` dump beside it, in path order.
pub fn goldens() -> Vec<Golden> {
    let root = testdata();
    let mut out: Vec<Golden> = psi_check::files_under(&root.to_string_lossy(), "bzl")
        .filter_map(|source| {
            let expected = source.with_extension("txt");
            let name = source.strip_prefix(&root).ok()?.to_string_lossy().into_owned();
            expected.exists().then_some(Golden { name, source, expected })
        })
        .collect();
    out.sort_by(|left, right| left.name.cmp(&right.name));
    out
}

/// The case's source as the plugin's test reads it: `JUnit`'s test data is trimmed of its surrounding blank lines.
pub fn source_text(golden: &Golden) -> String {
    std::fs::read_to_string(&golden.source).expect("read Starlark source").trim_matches(|ch: char| ch <= ' ').to_owned()
}

pub fn expected(golden: &Golden) -> String {
    std::fs::read_to_string(&golden.expected).expect("read Starlark golden").trim_end().to_owned()
}
