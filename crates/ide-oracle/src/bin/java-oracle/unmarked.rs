//! `java-oracle unmarked`: the tracked `.java` files of a checkout that carry no test markup, the Java analogue of
//! psi-tools' `tools/psi-gate/unmarked.py`. Markup is IDE markup (`<caret>`, `<selection>`, highlighting tags such as
//! `<error descr="...">`, `<expr>`), diagnostics markup (`<!NAME!>...<!>`) and sections of other languages in a
//! multi-file test (`// FILE: A.kt`). The parser reads them as code, so a verdict on such a file says nothing about the
//! parser; the files without markup stay, broken or not.
//!
//! The patterns are over bytes with ASCII classes (`\w` is `[A-Za-z0-9_]`):
//! `<(?:/?caret\w*|/?NAME|rename\b[^>]*)\b|<!\w*(?:[:(][^>]*)?!>|<!>` and, per line, `^// FILE: \S+\.(?!java\s*$)\w+\s*$`.
use anyhow::Result;
use std::io::Write;
use std::path::Path;

/// The tags that take an optional `/` and end at a word boundary (`caret` takes any word characters after it).
const TAGS: [&str; 26] = [
    "before", "after", "change", "selection", "error", "warning", "weak_warning", "info", "lineMarker", "EOLError",
    "EOLWarning", "spot", "symbolName", "fold", "inlineHint", "hint", "ref", "cursor", "block", "html", "inject", "rainbow",
    "TYPO", "text_attr", "caretMarker", "expr",
];

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// `\b` after a word character at `i`.
fn ends_word(data: &[u8], i: usize) -> bool {
    data.get(i).is_none_or(|&b| !is_word(b))
}

fn tag_at(data: &[u8], p: usize) -> bool {
    let after = &data[p + 1..];
    if after.starts_with(b"rename") && ends_word(data, p + 1 + 6) {
        return true;
    }
    let name = p + 1 + usize::from(after.starts_with(b"/"));
    let rest = &data[name..];
    rest.starts_with(b"caret") || TAGS.iter().any(|t| rest.starts_with(t.as_bytes()) && ends_word(data, name + t.len()))
}

/// `<!\w*(?:[:(][^>]*)?!>` or `<!>` at `p`.
fn diagnostic_at(data: &[u8], p: usize) -> bool {
    if !data[p + 1..].starts_with(b"!") {
        return false;
    }
    let start = p + 2;
    let word = start + data[start..].iter().take_while(|&&b| is_word(b)).count();
    if data[start..].starts_with(b">") || data[word..].starts_with(b"!>") {
        return true;
    }
    // `[:(]`, then anything but `>` up to `!>`: the first `>` after it closes the match, preceded by its `!`.
    if matches!(data.get(word), Some(b':' | b'(')) {
        let from = word + 1;
        if let Some(gt) = data[from..].iter().position(|&b| b == b'>') {
            return gt >= 1 && data[from + gt - 1] == b'!';
        }
    }
    false
}

/// `// FILE: NAME.EXT` of an extension other than `java`, `NAME` without whitespace, `EXT` word characters.
fn foreign_line(line: &[u8]) -> bool {
    let Some(rest) = line.strip_prefix(b"// FILE: ") else { return false };
    let Some(dot) = rest.iter().rposition(|&b| b == b'.') else { return false };
    let (name, ext) = (&rest[..dot], &rest[dot + 1..]);
    let word = ext.iter().take_while(|&&b| is_word(b)).count();
    !name.is_empty() && !name.iter().any(|&b| is_space(b)) && word > 0 && ext[word..].iter().all(|&b| is_space(b)) && &ext[..word] != b"java"
}

pub fn has_markup(data: &[u8]) -> bool {
    let markup = data.iter().enumerate().any(|(p, &b)| b == b'<' && (tag_at(data, p) || diagnostic_at(data, p)));
    markup || data.split(|&b| b == b'\n').any(foreign_line)
}

pub fn unmarked(checkout: &Path) -> Result<()> {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for path in ide_oracle::git::tracked(checkout, &["*.java"])? {
        let Ok(data) = std::fs::read(checkout.join(&path)) else { continue };
        if !has_markup(&data) {
            writeln!(out, "{path}")?;
        }
    }
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_found_as_the_patterns_find_it() {
        for marked in [
            "a<caret>b",
            "<caretx",
            "</selection>",
            "<error descr=\"x\">",
            "<rename x>",
            "<!UNRESOLVED!>x<!>",
            "<!A:b c!>",
            "<!>",
            "x\n// FILE: a.kt\n",
            "// FILE: a.java.kts  \r",
        ] {
            assert!(has_markup(marked.as_bytes()), "{marked:?}");
        }
        for clean in ["a < b", "<errors>", "<renamed>", "<!A:b > c!>", "<!a b!>", "// FILE: A.java", "// FILE: A.java \n", "// FILE: .kt", "// FILE: a b.kt"] {
            assert!(!has_markup(clean.as_bytes()), "{clean:?}");
        }
    }
}
