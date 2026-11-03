//! The golden pairs under testdata/{psi,spec} and the source text each Kotlin test parses.

#![allow(dead_code)]

pub mod getters;

use std::path::{Path, PathBuf};

pub struct Golden {
    /// The source file, relative to testdata.
    pub name: String,
    pub source: PathBuf,
    /// The expected dump (`.txt`).
    pub expected: PathBuf,
}

pub fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

/// Every `.kt`/`.kts` with a `.txt` beside it, sorted.
pub fn goldens() -> Vec<Golden> {
    let root = testdata();
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
            if ext != "kt" && ext != "kts" {
                continue;
            }
            let expected = path.with_extension("txt");
            if expected.exists() {
                let name = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().into_owned();
                out.push(Golden { name, source: path, expected });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Line separators converted to `\n` (`FileUtil.loadFile(file, convertLineSeparators = true)`).
fn convert_line_separators(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The text the Kotlin test parses. The PSI tests (`AbstractPsiBasedTest`) parse `readText().trim()`; the spec tests
/// (`AbstractParsingTestSpec`) parse the file with line separators converted, `trim { it <= ' ' }`, and the
/// `/* KOTLIN PSI SPEC TEST ... */` header removed with the newlines after it (`CommonParser.testInfoFilter`).
pub fn source_text(golden: &Golden) -> String {
    let raw = std::fs::read(&golden.source).expect("read source");
    let text = String::from_utf8_lossy(&raw).into_owned();
    if golden.name.starts_with("spec") {
        let text = convert_line_separators(&text);
        let text = text.trim_matches(|c: char| c <= ' ');
        remove_spec_header(text)
    } else {
        text.trim().to_owned()
    }
}

/// `testInfoPattern`: `/\*\s+? \* KOTLIN (PSI|...) (NOT LINKED )?SPEC TEST \((POSITIVE|NEGATIVE)\)\n[\s\S]*?\n\s+\*/(?:\n)*`.
fn remove_spec_header(text: &str) -> String {
    let Some(start) = text.find("/*") else { return text.to_owned() };
    let header_start = &text[start..];
    let first_line_end = header_start.find('\n').unwrap_or(header_start.len());
    if !header_start[..first_line_end.min(header_start.len())].starts_with("/*")
        || !header_start.contains("SPEC TEST (")
    {
        return text.to_owned();
    }
    // the first "*/" preceded by a line break and whitespace
    let mut search = 0;
    while let Some(i) = header_start[search..].find("*/") {
        let close = search + i;
        let before = &header_start[..close];
        let trimmed = before.trim_end_matches([' ', '\t']);
        if trimmed.len() < before.len() && trimmed.ends_with('\n') || before.ends_with("\n") {
            let mut end = close + 2;
            while header_start[end..].starts_with('\n') {
                end += 1;
            }
            return format!("{}{}", &text[..start], &header_start[end..]);
        }
        search = close + 2;
    }
    text.to_owned()
}

/// The file name the dump's first line shows.
pub fn file_name(golden: &Golden) -> String {
    golden.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn expected(golden: &Golden) -> String {
    let text = std::fs::read_to_string(&golden.expected).expect("read expected");
    text.trim_end().to_owned()
}

/// `DebugUtil.fixWhiteSpaces`.
pub fn fix_white_spaces(text: &[u8]) -> String {
    String::from_utf8_lossy(text).replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t")
}

/// Goldens the Kotlin 2.4.21 parser does not reproduce, with the reason. Their `.txt` is never compared by the
/// compiler's own test, and the PSI oracle prints something else for them.
pub const KNOWN_STALE: &[(&str, &str)] = &[(
    "spec/linked/expressions/when-expression/p-6/neg/1.2.kt",
    "an `EXCEPTION: compiler` spec test: TestExceptionsComparator runs it, the .txt is a dump of other text",
)];

pub fn is_known_stale(golden: &Golden) -> bool {
    KNOWN_STALE.iter().any(|(name, _)| *name == golden.name)
}
