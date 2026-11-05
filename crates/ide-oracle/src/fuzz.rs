//! Seeded one-place mutations: an edit at one place of a text (insert, replace or delete from an alphabet of pieces,
//! or truncate), and the scatter corpus of the lexer fuzz, a few such edits on each of `COUNT` inputs drawn from the
//! goldens. Texts are sequences of `char` (code points, as a Python `str` indexes) or of `u8` (bytes, for inputs that
//! need not be UTF-8); every draw is [`PyRandom`]'s.
use crate::random::PyRandom;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// One edit of a text at one place; offsets index the text's elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit<'p, T> {
    Insert { at: usize, piece: &'p [T] },
    /// `at..end` replaced by `piece`.
    Replace { at: usize, end: usize, piece: &'p [T] },
    /// `at..end` removed.
    Delete { at: usize, end: usize },
    /// Everything from `at` removed.
    Truncate { at: usize },
}

impl<T: Clone> Edit<'_, T> {
    pub fn apply(&self, text: &[T]) -> Vec<T> {
        let splice = |at: usize, end: usize, piece: &[T]| {
            let mut out = Vec::with_capacity(text.len() + piece.len());
            out.extend_from_slice(&text[..at]);
            out.extend_from_slice(piece);
            out.extend_from_slice(&text[end..]);
            out
        };
        match *self {
            Edit::Insert { at, piece } => splice(at, at, piece),
            Edit::Replace { at, end, piece } => splice(at, end, piece),
            Edit::Delete { at, end } => splice(at, end, &[]),
            Edit::Truncate { at } => text[..at].to_vec(),
        }
    }
}

/// The lexer fuzz's edits of one text: one to four times, at a uniform place, a piece inserted (60 %), up to three
/// elements replaced by a piece (25 %) or up to five deleted (15 %).
pub fn scatter<T: Clone>(mut text: Vec<T>, pieces: &[Vec<T>], rng: &mut PyRandom) -> Vec<T> {
    for _ in 0..rng.randint(1, 4) {
        let at = rng.randint(0, text.len() as i64) as usize;
        let op = rng.random();
        let edit = if op < 0.6 {
            Edit::Insert { at, piece: rng.choice(pieces) }
        } else if op < 0.85 {
            let end = text.len().min(at + rng.randint(1, 3) as usize);
            Edit::Replace { at, end, piece: rng.choice(pieces) }
        } else {
            let end = text.len().min(at + rng.randint(1, 5) as usize);
            Edit::Delete { at, end }
        };
        text = edit.apply(&text);
    }
    text
}

/// A file's text as Python's `Path.read_text` reads it: strict UTF-8, `\r\n` and `\r` read as `\n`.
pub fn read_text(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {} as UTF-8", path.display()))?;
    Ok(if text.contains('\r') { text.replace("\r\n", "\n").replace('\r', "\n") } else { text })
}

/// The files under `dir` whose name ends with `.EXTENSION`, recursively (symbolic links to directories not followed),
/// in path order: component by component, as Python sorts `Path`s.
pub fn files_with_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let suffix = format!(".{extension}");
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).with_context(|| format!("listing {}", d.display()))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                stack.push(entry.path());
            } else if entry.file_name().to_string_lossy().ends_with(&suffix) && entry.path().is_file() {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The lexer fuzz corpus: `count` files `OUT/NNNNN.EXTENSION`, each a golden input (every `*.EXTENSION` under
/// `goldens`, drawn uniformly) with [`scatter`]'s edits from `pieces`, and `OUT/list` naming them; deterministic for
/// `seed`. Every file is valid UTF-8 (the edits are of code points).
pub fn scatter_corpus(goldens: &Path, extension: &str, out: &Path, count: usize, seed: &str, pieces: &[&str]) -> Result<()> {
    let texts: Vec<Vec<char>> =
        files_with_extension(goldens, extension)?.iter().map(|p| read_text(p).map(|t| t.chars().collect())).collect::<Result<_>>()?;
    anyhow::ensure!(!texts.is_empty(), "no *.{extension} under {}", goldens.display());
    let pieces: Vec<Vec<char>> = pieces.iter().map(|p| p.chars().collect()).collect();
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut rng = PyRandom::from_str_seed(seed);
    let mut list = String::new();
    for i in 0..count {
        let name = format!("{i:05}.{extension}");
        let text: String = scatter(rng.choice(&texts).clone(), &pieces, &mut rng).into_iter().collect();
        std::fs::write(out.join(&name), text).with_context(|| format!("writing {name} in {}", out.display()))?;
        list.push_str(&name);
        list.push('\n');
    }
    std::fs::write(out.join("list"), list).with_context(|| format!("writing {}", out.join("list").display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_apply_at_one_place() {
        let text = b"abcdef";
        assert_eq!(Edit::Insert { at: 2, piece: b"XY" }.apply(text), b"abXYcdef");
        assert_eq!(Edit::Replace { at: 2, end: 4, piece: b"X" }.apply(text), b"abXef");
        assert_eq!(Edit::Delete { at: 0, end: 1 }.apply(text), b"bcdef");
        assert_eq!(Edit::<u8>::Truncate { at: 3 }.apply(text), b"abc");
    }

    #[test]
    fn paths_sort_by_component() {
        let dir = std::env::temp_dir().join(format!("ide-oracle-fuzz-{}", std::process::id()));
        for f in ["a-c/x.java", "a/b.java", "a/b.txt"] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x\r\ny\r").unwrap();
        }
        // As strings "a-c/x.java" < "a/b.java"; as Python's (and Rust's) paths `a` < `a-c`.
        let files = files_with_extension(&dir, "java").unwrap();
        assert_eq!(files, [dir.join("a/b.java"), dir.join("a-c/x.java")]);
        assert_eq!(read_text(&files[0]).unwrap(), "x\ny\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
