//! `--check`: our dumps, lexer lines and value text lines against an oracle's cached ones, made and compared in memory on every core, so
//! that a run writes only what differs (the oracle's dumps are read once, ours never touch the disk).

use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;

use crate::{Dumps, Lang, Parsers, Schedule};

/// The oracle's index, which the gate's `diff -rq -x index.jsonl` left out (at any depth) and `--check` does too.
const INDEX: &str = "index.jsonl";

/// The bytes of an oracle dump read at a time.
const BUFFER: usize = 1 << 20;

/// `--check ORACLE` of dumps: `make(state, i)` makes the dumps of list line `i` on `threads` threads (`Schedule::run`),
/// each compared byte for byte with the oracle's file of that name; then every file under ORACLE but `index.jsonl` must be one of ours.
/// Prints the differences sorted by name and `N differing of FILES`; writes the differing dumps of ours and the
/// differences (`differing.txt`) to `out`. Returns N.
pub fn dumps<'a, S>(
    oracle: &Path,
    out: Option<&Path>,
    schedule: &Schedule,
    threads: usize,
    init: impl Fn() -> S + Sync,
    make: impl Fn(&mut S, usize) -> Dumps<'a> + Sync,
    stdout: &mut impl Write,
) -> usize {
    let mut ours = HashSet::new();
    let mut differences: Vec<(String, String)> = Vec::new();
    let check = |(state, buffer): &mut (S, Vec<u8>), i: usize| {
        let dumps = make(state, i);
        let base = normalized(dumps.path);
        let mut compared = Vec::with_capacity(dumps.files.len());
        for (suffix, text) in dumps.files {
            let name = format!("{base}.{suffix}");
            let difference = compare(&oracle.join(&name), text.as_bytes(), buffer);
            if difference.is_some()
                && let Some(out) = out
            {
                write(&out.join(&name), text.as_bytes());
            }
            compared.push((name, difference));
        }
        compared
    };
    let tree = std::thread::scope(|scope| {
        let walk = scope.spawn(|| oracle_tree(oracle));
        schedule.run(
            threads,
            || (init(), vec![0u8; BUFFER]),
            check,
            |_, compared| {
                for (name, difference) in compared {
                    if let Some(difference) = difference {
                        differences.push((name.clone(), difference));
                    }
                    ours.insert(name);
                }
            },
        );
        walk.join()
    });
    match tree {
        Ok(Ok(tree)) => {
            for name in tree.files.into_iter().filter(|name| !ours.contains(name)) {
                differences.push((name, "only the oracle has it".to_string()));
            }
            for dir in tree.empty_dirs {
                differences.push((format!("{dir}/"), "an oracle directory without a dump".to_string()));
            }
        }
        Ok(Err(e)) => differences.push((oracle.display().to_string(), format!("cannot read the oracle: {e}"))),
        Err(_) => differences.push((oracle.display().to_string(), "the oracle's walk panicked".to_string())),
    }
    differences.sort();
    let report: String = differences.iter().map(|(name, what)| format!("{name}: {what}\n")).collect();
    let _ = stdout.write_all(report.as_bytes());
    if let Some(out) = out {
        write(&out.join("differing.txt"), report.as_bytes());
    }
    let _ = writeln!(stdout, "{} differing of {}", differences.len(), schedule.order.len());
    differences.len()
}

/// `--lex --check LINES`: our line of each list line (`lex(parsers, i)`, one newline-terminated line) against line `i`
/// of LINES, a Kotlin line cut to its first two columns as `scripts/LexOracle.java` prints them (the gate's
/// `cut -f1,2`). Prints each differing pair (`<` ours, `>` the oracle's) and `N differing lines of M`; returns N.
pub fn lex(
    lines: &Path,
    paths: &[String],
    schedule: &Schedule,
    lang: Option<Lang>,
    threads: usize,
    lex: impl Fn(&mut Parsers, usize) -> String + Sync,
    stdout: &mut impl Write,
) -> usize {
    let text = read_lines(lines);
    let expected: Vec<&str> = text.split_terminator('\n').collect();
    // A last line without its newline is a line `diff` tells apart from ours, which always ends with one.
    let unterminated = !text.is_empty() && !text.ends_with('\n');
    let mut differing = 0usize;
    let mut report = |ours: Option<&str>, oracle: Option<&str>| {
        differing += 1;
        if let Some(line) = ours {
            let _ = writeln!(stdout, "< {line}");
        }
        if let Some(line) = oracle {
            let _ = writeln!(stdout, "> {line}");
        }
    };
    schedule.run(
        threads,
        || Parsers::new(None),
        lex,
        |i, line| {
            let line = line.strip_suffix('\n').unwrap_or(&line);
            let path = paths[i].split('\t').next().unwrap_or(&paths[i]);
            let line = if Lang::of(path, lang) == Lang::Kotlin { first_two_columns(line) } else { line };
            let oracle = expected.get(i).copied();
            if oracle != Some(line) || (unterminated && i + 1 == expected.len()) {
                report(Some(line), oracle);
            }
        },
    );
    for &oracle in expected.iter().skip(paths.len()) {
        report(None, Some(oracle));
    }
    let _ = writeln!(stdout, "{differing} differing lines of {}", expected.len());
    differing
}

/// `--values --check LINES` and `--directives --check LINES` (C#): the lines `per_file(state, i)` makes of list line
/// `i` (`dump::value_lines`, `dump::directive_lines`: none or more a file) against the cached lines of
/// `tools/csharp-oracle/oracle.sh values --kept` or `directives` of the same file (their first column, the list line's
/// path), every line in order. Prints each differing file's lines (`< ours`, `> oracle`), the oracle's lines of a file
/// not listed, and `N differing lines of M`. Returns N.
pub fn file_lines(
    lines: &Path,
    paths: &[String],
    schedule: &Schedule,
    threads: usize,
    per_file: impl Fn(&mut Parsers, usize) -> String + Sync,
    stdout: &mut impl Write,
) -> usize {
    let text = read_lines(lines);
    let total = text.split_terminator('\n').count();
    let mut expected: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut order = Vec::new();
    for line in text.split_terminator('\n') {
        let path = line.split('\t').next().unwrap_or(line);
        expected
            .entry(path)
            .or_insert_with(|| {
                order.push(path);
                Vec::new()
            })
            .push(line);
    }
    let mut differing = 0usize;
    schedule.run(
        threads,
        || Parsers::new(None),
        per_file,
        |i, ours| {
            let path = paths[i].split('\t').next().unwrap_or(&paths[i]);
            let oracle = expected.remove(path).unwrap_or_default();
            let ours: Vec<&str> = ours.split_terminator('\n').collect();
            if ours != oracle {
                differing += ours.len().max(oracle.len());
                for line in ours {
                    let _ = writeln!(stdout, "< {line}");
                }
                for line in oracle {
                    let _ = writeln!(stdout, "> {line}");
                }
            }
        },
    );
    for path in order {
        for line in expected.remove(path).unwrap_or_default() {
            differing += 1;
            let _ = writeln!(stdout, "> {line}");
        }
    }
    let _ = writeln!(stdout, "{differing} differing lines of {total}");
    differing
}

/// The text of LINES; one that cannot be read ends the process.
fn read_lines(lines: &Path) -> String {
    std::fs::read_to_string(lines).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {e}", lines.display());
        std::process::exit(1)
    })
}

/// `cut -f1,2`: the line up to its second tab.
fn first_two_columns(line: &str) -> &str {
    line.match_indices('\t').nth(1).map_or(line, |(at, _)| &line[..at])
}

/// A list path as the oracle names its dump under its directory: the components joined by `/`, without empty or `.`
/// ones (as `Path.resolve` and a directory walk see it).
fn normalized(path: &str) -> String {
    path.split('/').filter(|c| !c.is_empty() && *c != ".").collect::<Vec<_>>().join("/")
}

/// How our dump compares with the oracle's file `path`: `None` when byte-equal, else the difference.
fn compare(path: &Path, text: &[u8], buffer: &mut [u8]) -> Option<String> {
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Some("only ours has it".to_string()),
        Err(e) => return Some(format!("cannot read the oracle's: {e}")),
    };
    match same_bytes(&mut file, text, buffer) {
        Ok(true) => None,
        Ok(false) if path.to_string_lossy().ends_with(".getters.txt") => match std::fs::read(path) {
            Ok(oracle) => getters_difference(&oracle, text),
            Err(e) => Some(format!("cannot read the oracle's: {e}")),
        },
        Ok(false) => Some("differs".to_string()),
        Err(e) => Some(format!("cannot read the oracle's: {e}")),
    }
}

/// `--getters --check`: the oracle's getter lines against ours, line by line, an oracle answer `!` (IntelliJ threw)
/// matching any of ours for the same `<id> <getter>`. `None` when every line matches, else the first difference.
fn getters_difference(oracle: &[u8], ours: &[u8]) -> Option<String> {
    let oracle_lines = oracle.split(|&b| b == b'\n').filter(|l| !l.is_empty());
    let mut our_lines = ours.split(|&b| b == b'\n').filter(|l| !l.is_empty());
    for expected in oracle_lines {
        let Some(line) = our_lines.next() else {
            return Some(format!("differs: only the oracle has {}", String::from_utf8_lossy(expected)));
        };
        if line == expected {
            continue;
        }
        let excused =
            expected.strip_suffix(b" !").is_some_and(|key| line.starts_with(key) && line.get(key.len()) == Some(&b' '));
        if !excused {
            return Some(format!(
                "differs: ours {} where the oracle has {}",
                String::from_utf8_lossy(line),
                String::from_utf8_lossy(expected)
            ));
        }
    }
    our_lines.next().map(|line| format!("differs: only ours has {}", String::from_utf8_lossy(line)))
}

/// The file holds exactly `text`, read a buffer at a time.
fn same_bytes(file: &mut std::fs::File, text: &[u8], buffer: &mut [u8]) -> std::io::Result<bool> {
    if file.metadata()?.len() != text.len() as u64 {
        return Ok(false);
    }
    let mut at = 0;
    loop {
        let n = match file.read(buffer) {
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Ok(at == text.len());
        }
        if text.get(at..at + n) != Some(&buffer[..n]) {
            return Ok(false);
        }
        at += n;
    }
}

fn write(target: &Path, bytes: &[u8]) {
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(target, bytes) {
        eprintln!("cannot write {}: {e}", target.display());
    }
}

/// The oracle's directory as `diff -rq -x index.jsonl` sees it: its files but `index.jsonl` (relative, `/`-joined) and
/// its directories without one of them under them (a directory our side never makes, so `diff` reports it).
#[derive(Default)]
struct OracleTree {
    files: Vec<String>,
    empty_dirs: Vec<String>,
}

fn oracle_tree(dir: &Path) -> std::io::Result<OracleTree> {
    let mut tree = OracleTree::default();
    walk(dir, "", &mut tree)?;
    Ok(tree)
}

/// Adds the files under `dir` (named `rel` in the tree) to `tree`; returns how many there are.
fn walk(dir: &Path, rel: &str, tree: &mut OracleTree) -> std::io::Result<usize> {
    let mut files = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == INDEX {
            continue;
        }
        let child = if rel.is_empty() { name.into_owned() } else { format!("{rel}/{name}") };
        if entry.file_type()?.is_dir() {
            let under = walk(&entry.path(), &child, tree)?;
            if under == 0 {
                tree.empty_dirs.push(child);
            }
            files += under;
        } else {
            tree.files.push(child);
            files += 1;
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kotlin_lexer_line_is_cut_to_two_columns() {
        assert_eq!(first_two_columns("a.kt\tf00\t12"), "a.kt\tf00");
        assert_eq!(first_two_columns("a.kt\tunreadable"), "a.kt\tunreadable");
        assert_eq!(first_two_columns("a.kt"), "a.kt");
    }

    #[test]
    fn list_paths_are_named_as_under_the_oracle() {
        assert_eq!(normalized("./a//b/./c.kt"), "a/b/c.kt");
        assert_eq!(normalized("a/b.kt"), "a/b.kt");
    }
}
