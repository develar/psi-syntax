//! The goldens of `testdata/java` and the text each Java test parses of them.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use java_syntax::{Entry, LanguageLevel};
pub use psi_check::trim;

pub fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java")
}

/// A line of `testdata/java/manifest.tsv`: `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM`.
pub struct Golden {
    /// The source file, relative to `testdata/java`.
    pub path: String,
    pub level: LanguageLevel,
    pub entry: Entry,
    /// Whether the test trims the text (`String.trim()`).
    pub trim: bool,
}

impl Golden {
    /// The file name, as the dump's first line names it.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The text the golden parses: line separators made `\n`, trimmed when the test trims it.
    pub fn source_text(&self) -> Vec<u8> {
        let raw = std::fs::read(testdata().join(&self.path)).expect("read golden source");
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
        if self.trim { trim(text) } else { text }
    }

    /// The file without its `\r`s, untrimmed.
    pub fn text_without_cr(&self) -> Vec<u8> {
        let mut text: Vec<u8> = std::fs::read(testdata().join(&self.path)).expect("read golden");
        text.retain(|&b| b != b'\r');
        text
    }

    /// The golden's dump with `suffix` (`.txt`, `_node.txt`) in place of `.java`.
    pub fn dump(&self, suffix: &str) -> String {
        std::fs::read_to_string(testdata().join(self.path.replace(".java", suffix))).expect("read golden dump")
    }
}

/// Every golden of the manifest, in its order; a level or entry it does not name is `HIGHEST` or a file.
pub fn goldens() -> Vec<Golden> {
    let manifest = std::fs::read_to_string(testdata().join("manifest.tsv")).expect("read manifest.tsv");
    manifest
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            Golden {
                path: columns[0].to_string(),
                level: LanguageLevel::from_name(columns[1]).unwrap_or(LanguageLevel::HIGHEST),
                entry: Entry::from_name(columns[2]).unwrap_or(Entry::File),
                trim: columns[3] == "1",
            }
        })
        .collect()
}

/// Whether `JAVA_STRICT=1` or `KS_STRICT=1` (the gate's) asks a test to fail on any difference.
pub fn strict() -> bool {
    ["JAVA_STRICT", "KS_STRICT"].iter().any(|k| std::env::var_os(k).is_some_and(|v| v == "1"))
}
