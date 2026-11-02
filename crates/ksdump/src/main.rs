//! `ksdump`: dumps, benchmarks and lexes listed files with the psi-syntax parsers.
//!
//! Each file's language is `--lang kotlin|java|csharp` when given, else its extension: `.java` is Java, `.cs` C#,
//! anything else Kotlin.
//! `--lex`, `--out`, `--bench` and `--check-bodies` take Java files (`crates/java-syntax`); a `--bench` or
//! `--check-bodies` list (`--lex --bench` too) is of one language, else ksdump exits with status 2 before anything runs.
//!
//! - `ksdump --list LIST --root ROOT --out OUT [--trim] [--node]`: writes `OUT/<path>.psi.txt` per listed path, the
//!   `psi_to_string` dump, like the PSI oracle (`PsiOracle.java`) writes its own; compare with `scripts/compare.py`.
//!   A Java list line may carry `<TAB>LEVEL<TAB>ENTRY<TAB>TRIM` as for `JavaPsiOracle.java`; `--node` also writes
//!   `OUT/<path>.node.txt` (`node_tree_to_string`) for Java files.
//! - `ksdump --bench --list LIST --root ROOT [--threads N] [--walk] [--no-bodies] [--repeat K] [--file-stats OUT.tsv]`: reads
//!   every file, then parses them all and prints the files, megabytes, seconds and MB/s of the parse alone (the first
//!   line, one per round), then the tree's heap bytes per source MB. `--threads N` parses on N threads taking files
//!   from a shared counter, one `Parser` per thread, as an indexer does, and prints the wall MB/s and each thread's own parse MB/s; `--walk`
//!   also walks every tree in preorder reading each node's kind, range and depth, as an extractor does, and then
//!   scans its nodes in id order (`descendants`) reading kind and range, each timed apart; `--no-bodies` parses
//!   with `ParseOptions { expand_bodies: false }`, as a declaration-only pass does;
//!   `--repeat K` runs K rounds and prints their median; `--file-stats` writes one line per file of the last round.
//!   A Java list line may carry `<TAB>LEVEL` (default `LanguageLevel.HIGHEST`); every file parses as a file.
//!   Built with `--features alloc-stats`, a counting allocator adds the allocations per file (exact with one thread)
//!   and the bytes allocated per source MB; it slows the parse, so time without it. With `--features counters`, the
//!   builder's calls (`tt()`, lookahead, markers, rollbacks) and the tokens lexed, reused and skipped, per file token. Peak RSS: run under
//!   `/usr/bin/time -l` (the sources stay loaded: subtract their MB).
//! - `ksdump --lex --list LIST --root ROOT [--dump]`: one line per file as `scripts/LexOracle.java` (Kotlin) or
//!   `tools/java-oracle/JavaLexOracle.java` (Java; a list line may carry the level: `PATH<TAB>LEVEL`) prints it;
//!   with `--bench [--repeat K] [--docs]`, the MB/s of the file lexer alone instead (Kotlin's or Java's), and with
//!   `--docs` also of the doc-comment lexer over every doc comment, timed apart.
//! - `ksdump --check-bodies --list LIST --root ROOT`: parses each file in full and body-less and checks that the full
//!   tree's leaves are the text and `parse_body` of every unexpanded body is the full tree's subtree there (the same
//!   kinds, ranges, error messages and shape); prints the bodies, the differing ones (their paths and ranges) and
//!   exits nonzero on any. A Java list line is read as for `--out`.
//! - C# (`crates/csharp-syntax`): a list line is `PATH[<TAB>VERSION[<TAB>ENTRY[<TAB>DOC]]]` as `tools/csharp-oracle`
//!   reads it (`latest`/`preview`/`7.3`, `compilation-unit`/`expression`/..., `parse`/`none`); the text is the file's
//!   bytes as they are (no line-separator conversion) and must be UTF-8. `--out` writes `OUT/<path>.tree.txt` and
//!   `OUT/index.jsonl` as the oracle does (a dump past 2^29 UTF-16 code units is the failure `dump over 512 MB`);
//!   `--lex [--full OUT]` prints `PATH<TAB>TOKENS<TAB>HASH` (FNV-1a 64 of the lexer dump) per file and with `--full`
//!   writes `OUT/<path>.lex.txt`; `--bench` parses a list of C# files; `--check-bodies` (C# has no lazily parsed
//!   bodies) only checks that each tree's leaves are its file's text. `--nesting-limit` does not apply.
//! - `ksdump --check ORACLE --list LIST --root ROOT [--out OUT] [--node]`: makes the dumps `--out` would write, each in
//!   memory, and compares them byte for byte with the oracle's `ORACLE/<path>.<suffix>` without writing ours (what
//!   `diff -rq -x index.jsonl ORACLE OURS` checks of an `--out` directory: a dump that differs, a dump one side lacks, a
//!   directory of the oracle without dumps, all but `index.jsonl`); prints the differences sorted by path, then
//!   `N differing of M` (M the list's files), writes the differing dumps and the list `differing.txt` to OUT, and exits
//!   nonzero on any difference. A file the oracle has no dump for (its `index.jsonl` failures) must have none of ours.
//! - `ksdump --lex --check LINES --list LIST --root ROOT`: the `--lex` lines against the oracle's cached ones in LINES,
//!   line by line in list order (a Kotlin line's first two columns, as `scripts/LexOracle.java` prints them); prints
//!   each differing pair (`<` ours, `>` the oracle's), then `N differing lines of M` (M the oracle's lines), and exits
//!   nonzero on any.
//! - `--threads N`: `--out`, `--check`, `--lex` and `--check-bodies` parse on N threads (default: every core), one
//!   `Parser` per thread, files taken from a shared counter largest first; their output is in list order whatever N
//!   is. Peak RSS is a few files' trees and dumps (C#'s runtime corpus: 3.1 GB, its 9.9 MB `TestData.g.cs` alone
//!   most of it).
//! - `--nesting-limit N` (with `--check-bodies`, `--bench`, `--out` or `--check`): parses with the nesting limit N
//!   instead of `NESTING_LIMIT` (`Parser::set_nesting_limit`), so that ordinary files take the too-deep paths.
//!
//! The text is read as bytes with CRLF and CR converted to LF, as the oracles do; `--trim` also trims it (what the
//! compiler's parsing tests parse). A path ending in `.kt` (or without an extension) parses as a file, any other as a
//! script.

mod check;
mod pool;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use kotlin_syntax::counters::{self, AnyCounter};
use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::psi_builder::counters::Counter as BuilderCounter;
use kotlin_syntax::psi_builder::{self, Language, TreeLanguage};
use kotlin_syntax::tokens::TokenKind;
use kotlin_syntax::{FileKind, Kotlin, NodeId, ParseOptions, Parser};

/// Counts every allocation of the process (`--features alloc-stats` only).
#[cfg(feature = "alloc-stats")]
#[global_allocator]
static GLOBAL: stats_alloc::StatsAlloc<std::alloc::System> = stats_alloc::StatsAlloc::system();

fn usage() -> ! {
    eprintln!(
        "usage:\n  \
         ksdump --list LIST --root ROOT --out OUT [--trim] [--node]   write OUT/<path>.psi.txt (Java: .node.txt too) per listed file\n  \
         ksdump --bench --list LIST --root ROOT [--trim] [--threads N] [--walk] [--no-bodies] [--repeat K]\n  \
                [--file-stats OUT]                           parse throughput, tree bytes, walk cost\n  \
         ksdump --check-bodies --list LIST --root ROOT       parse_body of each body against the full parse\n  \
         ksdump --check ORACLE --list LIST --root ROOT [--out OUT] [--node]   our dumps against ORACLE's, in memory\n  \
         ksdump --lex --check LINES --list LIST --root ROOT  our lexer lines against the oracle's LINES\n  \
         (--nesting-limit N: parse with that nesting limit; --threads N: threads of --out, --check, --lex and\n  \
         --check-bodies, default every core)\n  \
         ksdump --lex --list LIST --root ROOT [--dump]       lexer hash lines as scripts/LexOracle.java prints them\n  \
         ksdump --lex --bench --list LIST --root ROOT [--repeat K] [--docs]   lexer throughput (--docs: doc comments too)\n  \
         ksdump --lex --list LIST --root ROOT [--full OUT]   C# files: lexer hash lines as tools/csharp-oracle lex prints them\n  \
         (--lang kotlin|java|csharp: the files' language; by default `.java` is Java, `.cs` C#, anything else Kotlin;\n  \
         C#: --version V and --doc parse|none|diagnose for the list lines without those columns, as tools/csharp-oracle)"
    );
    std::process::exit(2)
}

/// The language a file is parsed as.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Lang {
    Kotlin,
    Java,
    CSharp,
}

impl Lang {
    /// `--lang`, else by extension: `.java` is Java, `.cs` C#, anything else Kotlin (`.kt`, `.kts`, none).
    fn of(path: &str, forced: Option<Lang>) -> Lang {
        forced.unwrap_or(if path.ends_with(".java") {
            Lang::Java
        } else if path.ends_with(".cs") {
            Lang::CSharp
        } else {
            Lang::Kotlin
        })
    }
}

#[derive(Default)]
struct Args {
    list: Option<PathBuf>,
    root: Option<PathBuf>,
    out: Option<PathBuf>,
    bench: bool,
    lex: bool,
    dump: bool,
    trim: bool,
    /// `--threads N`; `--bench` defaults to one thread, the other modes to every core.
    threads: Option<usize>,
    /// `--check ORACLE`: the oracle's dump directory, or with `--lex` its file of lexer lines.
    check: Option<PathBuf>,
    walk: bool,
    no_bodies: bool,
    check_bodies: bool,
    repeat: usize,
    file_stats: Option<PathBuf>,
    nesting_limit: Option<u32>,
    lang: Option<Lang>,
    node: bool,
    docs: bool,
    /// `--full OUT` (C# `--lex`): the lexer dumps' directory.
    full: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut args = Args { repeat: 1, ..Args::default() };
    let count =
        |v: Option<String>| v.and_then(|v| v.parse::<usize>().ok()).filter(|&n| n > 0).unwrap_or_else(|| usage());
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--list" => args.list = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            "--root" => args.root = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            "--out" => args.out = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            "--bench" => args.bench = true,
            "--lex" => args.lex = true,
            "--dump" => args.dump = true,
            "--trim" => args.trim = true,
            "--node" => args.node = true,
            "--docs" => args.docs = true,
            "--full" => args.full = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            "--threads" => args.threads = Some(count(it.next())),
            "--check" => args.check = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            "--walk" => args.walk = true,
            "--no-bodies" => args.no_bodies = true,
            "--check-bodies" => args.check_bodies = true,
            "--repeat" => args.repeat = count(it.next()),
            "--file-stats" => args.file_stats = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            "--nesting-limit" => args.nesting_limit = Some(count(it.next()) as u32),
            "--version" => {
                let version = it.next().and_then(|v| csharp_syntax::LanguageVersion::from_name(&v));
                let mut defaults = csharp_defaults();
                defaults.version = version.unwrap_or_else(|| usage());
                set_csharp_defaults(defaults);
            }
            "--doc" => {
                let doc = it.next().and_then(|d| csharp_syntax::DocumentationMode::from_name(&d));
                let mut defaults = csharp_defaults();
                defaults.doc_mode = doc.unwrap_or_else(|| usage());
                set_csharp_defaults(defaults);
            }
            "--lang" => {
                args.lang = Some(match it.next().as_deref() {
                    Some("kotlin") => Lang::Kotlin,
                    Some("java") => Lang::Java,
                    Some("csharp") => Lang::CSharp,
                    _ => usage(),
                })
            }
            _ => usage(),
        }
    }
    args
}

impl Args {
    /// The threads of `--out`, `--check`, `--lex` and `--check-bodies`.
    fn workers(&self) -> usize {
        self.threads.unwrap_or_else(pool::default_threads)
    }
}

fn read_list(list: &Path) -> Vec<String> {
    match std::fs::read_to_string(list) {
        Ok(s) => s.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect(),
        Err(e) => {
            eprintln!("cannot read {}: {e}", list.display());
            std::process::exit(1)
        }
    }
}

/// CRLF and CR to LF, as the oracles do.
fn normalize_line_separators(text: Vec<u8>) -> Vec<u8> {
    if !text.contains(&b'\r') {
        return text;
    }
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\r' {
            out.push(b'\n');
            if text.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
        } else {
            out.push(text[i]);
        }
        i += 1;
    }
    out
}

/// `String.strip()`: Unicode whitespace at both ends.
fn strip(text: Vec<u8>) -> Vec<u8> {
    match std::str::from_utf8(&text) {
        Ok(s) => s.trim().as_bytes().to_vec(),
        Err(_) => text,
    }
}

fn read_source(root: &Path, path: &str, trim: bool) -> Option<Vec<u8>> {
    let text = normalize_line_separators(std::fs::read(root.join(path)).ok()?);
    Some(if trim { strip(text) } else { text })
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

struct Fnv(u64);

impl Fnv {
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

fn escape(text: &[u8]) -> String {
    String::from_utf8_lossy(text).replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t")
}

/// The hash and token count `LexOracle.java` prints (KDoc tokens hashed inside `[` `]` after their doc comment), of the
/// tokens a parse lexes: `lexer::tokenize_into` and, in each doc comment, `kdoc::lexer::tokenize_into`.
fn lex_line(text: &[u8], dump: bool, out: &mut String) -> (u64, usize) {
    let mut hash = Fnv(0xcbf29ce484222325);
    let mut count = 0;
    let lexed = kotlin_syntax::lexer::tokenize(text);
    let (mut kdoc_types, mut kdoc_starts) = (Vec::new(), Vec::new());
    for (i, kind) in lexed.types.iter().enumerate() {
        let token_text = &text[lexed.starts[i] as usize..lexed.starts[i + 1] as usize];
        hash.feed(format!("{}:{};", kind.debug_name(), token_text.len()).as_bytes());
        if dump {
            out.push_str(&format!("  {} {}\n", kind.debug_name(), escape(token_text)));
        }
        if *kind == TokenKind::DOC_COMMENT {
            hash.feed(b"[");
            kotlin_syntax::kdoc::lexer::tokenize_into(token_text, &mut kdoc_types, &mut kdoc_starts);
            for (k, kind) in kdoc_types.iter().enumerate() {
                let k_text = &token_text[kdoc_starts[k] as usize..kdoc_starts[k + 1] as usize];
                hash.feed(format!("{}:{};", kind.debug_name(), k_text.len()).as_bytes());
                if dump {
                    out.push_str(&format!("    {} {}\n", kind.debug_name(), escape(k_text)));
                }
                count += 1;
            }
            hash.feed(b"]");
        }
        count += 1;
    }
    (hash.0, count)
}

/// The hash and token count `tools/java-oracle/JavaLexOracle.java` prints: `JavaLexer` at `level`, the
/// `JavaDocLexer` tokens of each doc comment (at `LanguageLevel.HIGHEST`, as `JavaDocCommentParser` lexes it) hashed
/// inside `[` `]` after it; of the tokens a parse lexes (`lexer::tokenize_into`, `doc_lexer::tokenize_into`).
fn java_lex_line(text: &[u8], level: java_syntax::LanguageLevel, dump: bool, out: &mut String) -> (u64, usize) {
    use java_syntax::TokenKind as JavaToken;
    let mut hash = Fnv(0xcbf29ce484222325);
    let mut count = 0;
    let lexed = java_syntax::lexer::tokenize(text, level);
    let (mut doc_types, mut doc_starts) = (Vec::new(), Vec::new());
    for (i, kind) in lexed.types.iter().enumerate() {
        let token_text = &text[lexed.starts[i] as usize..lexed.starts[i + 1] as usize];
        hash.feed(format!("{}:{};", kind.debug_name(), token_text.len()).as_bytes());
        if dump {
            out.push_str(&format!("  {} {}\n", kind.debug_name(), escape(token_text)));
        }
        if matches!(kind, JavaToken::DOC_COMMENT | JavaToken::DOC_MARKDOWN_COMMENT) {
            hash.feed(b"[");
            java_syntax::doc_lexer::tokenize_into(token_text, true, &mut doc_types, &mut doc_starts);
            for (k, kind) in doc_types.iter().enumerate() {
                let k_text = &token_text[doc_starts[k] as usize..doc_starts[k + 1] as usize];
                hash.feed(format!("{}:{};", kind.debug_name(), k_text.len()).as_bytes());
                if dump {
                    out.push_str(&format!("    {} {}\n", kind.debug_name(), escape(k_text)));
                }
                count += 1;
            }
            hash.feed(b"]");
        }
        count += 1;
    }
    (hash.0, count)
}

/// `--lex` of a Java file: a list line is `PATH[<TAB>LEVEL...]` as for the Java oracles; text that is not UTF-8 is
/// "unreadable", as the oracle, which decodes strictly, prints it.
fn java_lex(root: &Path, line: &str, dump: bool) -> String {
    let mut columns = line.split('\t');
    let path = columns.next().unwrap_or(line);
    let level =
        columns.next().and_then(java_syntax::LanguageLevel::from_name).unwrap_or(java_syntax::LanguageLevel::HIGHEST);
    let text = read_source(root, path, false).filter(|t| std::str::from_utf8(t).is_ok());
    let Some(text) = text else {
        return format!("{path}\tunreadable\t0\n");
    };
    let mut out = String::new();
    let (hash, count) = java_lex_line(&text, level, dump, &mut out);
    format!("{path}\t{hash:x}\t{count}\n{out}")
}

/// `--lex` of a Kotlin file: its line as `scripts/LexOracle.java` prints it (with the count, which `LexOracle.java`
/// leaves out) and with `--dump` the tokens after it.
fn kotlin_lex(root: &Path, path: &str, dump: bool) -> String {
    let Some(text) = read_source(root, path, false) else {
        return format!("{path}\tunreadable\t0\n");
    };
    let mut out = String::new();
    let (hash, count) = lex_line(&text, dump, &mut out);
    format!("{path}\t{hash:x}\t{count}\n{out}")
}

/// What `--lex` prints for a list line, newline-terminated: one line, with `--dump` the tokens after it.
fn lex_file(parsers: &mut Parsers, root: &Path, line: &str, args: &Args) -> String {
    let path = line.split('\t').next().unwrap_or(line);
    match Lang::of(path, args.lang) {
        Lang::Kotlin => kotlin_lex(root, path, args.dump),
        Lang::Java => java_lex(root, line, args.dump),
        Lang::CSharp => csharp_lex(parsers.csharp(), root, line, args.full.as_deref()) + "\n",
    }
}

/// A list's files as `--out`, `--check`, `--lex` and `--check-bodies` take them: largest first, so that the slowest
/// (C#'s 24 MB `hugeexpr1.cs`, some 20 s alone) start at once instead of finishing a run on one thread.
struct Schedule {
    order: Vec<usize>,
}

impl Schedule {
    /// The size of each list line's file (its first column; 0 when it cannot be read), stat'ed on `threads` threads.
    fn new(root: &Path, lines: &[String], threads: usize) -> Schedule {
        let mut sizes = Vec::with_capacity(lines.len());
        let size = |_: &mut (), i: usize| {
            let path = lines[i].split('\t').next().unwrap_or(&lines[i]);
            std::fs::metadata(root.join(path)).map_or(0, |m| m.len())
        };
        pool::ordered(&pool::list_order(lines.len()), threads, || (), size, |_, bytes| sizes.push(bytes));
        Schedule { order: pool::largest_first(&sizes) }
    }

    /// `work(state, i)` for the list's lines on `threads` threads (`pool::ordered`), largest first, each thread with its
    /// `init()` state; `each(i, result)` in list order.
    fn run<S, R: Send>(
        &self,
        threads: usize,
        init: impl Fn() -> S + Sync,
        work: impl Fn(&mut S, usize) -> R + Sync,
        each: impl FnMut(usize, R),
    ) {
        pool::ordered(&self.order, threads, init, work, each);
    }
}

/// A thread's parsers, one per language, each made when its first file comes.
struct Parsers {
    nesting_limit: Option<u32>,
    kotlin: Option<Parser>,
    java: Option<java_syntax::Parser>,
    csharp: Option<csharp_syntax::Parser>,
}

impl Parsers {
    fn new(nesting_limit: Option<u32>) -> Parsers {
        Parsers { nesting_limit, kotlin: None, java: None, csharp: None }
    }

    fn kotlin(&mut self) -> &mut Parser {
        let limit = self.nesting_limit;
        self.kotlin.get_or_insert_with(|| new_parser(limit))
    }

    fn java(&mut self) -> &mut java_syntax::Parser {
        let limit = self.nesting_limit;
        self.java.get_or_insert_with(|| new_parser(limit))
    }

    fn csharp(&mut self) -> &mut csharp_syntax::Parser {
        self.csharp.get_or_insert_with(csharp_syntax::Parser::new)
    }
}

/// The dumps of one list line as its oracle names them, `<path>.<suffix>` under its directory: Kotlin `psi.txt`, Java
/// `psi.txt` and with `--node` `node.txt`, C# `tree.txt`. A file the oracle reports as a failure has none.
struct Dumps<'a> {
    path: &'a str,
    files: Vec<(&'static str, String)>,
    /// A C# file's `index.jsonl` line.
    index: Option<String>,
}

/// What `--out` writes for a list line, made in memory.
fn dump_line<'a>(parsers: &mut Parsers, root: &Path, line: &'a str, args: &Args) -> Dumps<'a> {
    let path = line.split('\t').next().unwrap_or(line);
    let (files, index) = match Lang::of(path, args.lang) {
        Lang::Java => (java_dump(parsers.java(), root, line, args), None),
        Lang::CSharp => {
            let (dump, index) = csharp_dump(parsers.csharp(), root, line);
            (dump.map(|d| vec![("tree.txt", d)]).unwrap_or_default(), Some(index))
        }
        Lang::Kotlin => match read_source(root, path, args.trim) {
            Some(text) => {
                let name = file_name(path);
                let tree = parsers.kotlin().parse(&text, FileKind::from_file_name(name));
                (vec![("psi.txt", psi_to_string(&tree, name))], None)
            }
            None => {
                eprintln!("cannot read {path}");
                (Vec::new(), None)
            }
        },
    };
    Dumps { path, files, index }
}

/// `--out` of a list line's dumps: writes `OUT/<path>.<suffix>`; false when there is none or one cannot be written.
fn write_dumps(out: &Path, dumps: &Dumps) -> bool {
    let mut written = !dumps.files.is_empty();
    for (suffix, text) in &dumps.files {
        let target = out.join(format!("{}.{suffix}", dumps.path));
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&target, text) {
            eprintln!("cannot write {}: {e}", target.display());
            written = false;
        }
    }
    written
}

/// A Java list line, `PATH[<TAB>LEVEL[<TAB>ENTRY[<TAB>TRIM]]]` as `JavaPsiOracle.java` reads it: the path, how the
/// file parses (`LanguageLevel.HIGHEST` and a file by default) and whether its text is trimmed (`trim` by default).
fn java_line(line: &str, trim: bool) -> (&str, java_syntax::FileKind, bool) {
    let columns: Vec<&str> = line.split('\t').collect();
    let level = columns
        .get(1)
        .and_then(|l| java_syntax::LanguageLevel::from_name(l))
        .unwrap_or(java_syntax::LanguageLevel::HIGHEST);
    let entry = columns.get(2).and_then(|e| java_syntax::Entry::from_name(e)).unwrap_or(java_syntax::Entry::File);
    let trim = columns.get(3).map_or(trim, |t| *t == "1");
    (columns[0], java_syntax::FileKind { level, entry }, trim)
}

/// `String.trim()` of a Java test's text, by bytes.
fn trim_bytes(text: Vec<u8>) -> Vec<u8> {
    let start = text.iter().position(|&b| b > b' ').unwrap_or(text.len());
    let end = text.iter().rposition(|&b| b > b' ').map_or(start, |e| e + 1);
    text[start..end].to_vec()
}

/// `--out` of a Java file, as `tools/java-oracle/JavaPsiOracle.java` writes it: a list line is
/// `PATH[<TAB>LEVEL[<TAB>ENTRY[<TAB>TRIM]]]` (`testdata/java/manifest.tsv` is a list), the text is trimmed (chars up to
/// ' ', `String.trim()`) when TRIM is 1 or, without it, with `--trim`; the dumps of `<path>.psi.txt` and with `--node`
/// `<path>.node.txt`. Text that is not UTF-8 is not dumped (the oracle reports it as a failure).
fn java_dump(parser: &mut java_syntax::Parser, root: &Path, line: &str, args: &Args) -> Vec<(&'static str, String)> {
    let (path, kind, trim) = java_line(line, args.trim);
    let Some(mut text) = read_source(root, path, false).filter(|t| std::str::from_utf8(t).is_ok()) else {
        eprintln!("cannot read {path} as UTF-8");
        return Vec::new();
    };
    if trim {
        text = trim_bytes(text);
    }
    let tree = parser.parse(&text, kind);
    let mut dumps = vec![("psi.txt", java_syntax::dump::psi_to_string(&tree, file_name(path)))];
    if args.node {
        dumps.push(("node.txt", java_syntax::dump::node_tree_to_string(&tree)));
    }
    dumps
}

/// The C# file kind of a list line's empty or absent columns: `--version` and `--doc` (once each), else `latest`,
/// `compilation-unit`, `parse`.
static CSHARP_DEFAULTS: std::sync::Mutex<Option<csharp_syntax::FileKind>> = std::sync::Mutex::new(None);

fn csharp_defaults() -> csharp_syntax::FileKind {
    CSHARP_DEFAULTS.lock().ok().and_then(|d| *d).unwrap_or_default()
}

fn set_csharp_defaults(kind: csharp_syntax::FileKind) {
    if let Ok(mut d) = CSHARP_DEFAULTS.lock() {
        *d = Some(kind);
    }
}

/// A C# list line, `PATH[<TAB>VERSION[<TAB>ENTRY[<TAB>DOC]]]` as `tools/csharp-oracle` reads it (an empty or absent
/// column takes `csharp_defaults`); `None` for a column that names nothing.
fn csharp_line(line: &str) -> Option<(&str, csharp_syntax::FileKind)> {
    let columns: Vec<&str> = line.split('\t').collect();
    let mut kind = csharp_defaults();
    if let Some(v) = columns.get(1).filter(|v| !v.is_empty()) {
        kind.version = csharp_syntax::LanguageVersion::from_name(v)?;
    }
    if let Some(e) = columns.get(2).filter(|e| !e.is_empty()) {
        kind.entry = csharp_syntax::Entry::from_name(e)?;
    }
    if let Some(d) = columns.get(3).filter(|d| !d.is_empty()) {
        kind.doc_mode = csharp_syntax::DocumentationMode::from_name(d)?;
    }
    Some((columns[0], kind))
}

/// A C# file's bytes as they are, when they are UTF-8 (`Encoding.UTF8.GetString` with a throwing decoder).
fn csharp_source(root: &Path, path: &str) -> Result<Vec<u8>, &'static str> {
    let text = std::fs::read(root.join(path)).map_err(|_| "cannot read")?;
    if std::str::from_utf8(&text).is_err() {
        return Err("invalid UTF-8");
    }
    Ok(text)
}

/// `--out` of a C# file: the dump of `<path>.tree.txt` (none on a failure) and its `index.jsonl` line.
fn csharp_dump(parser: &mut csharp_syntax::Parser, root: &Path, line: &str) -> (Option<String>, String) {
    use csharp_syntax::dump;
    let Some((path, kind)) = csharp_line(line) else {
        return (None, dump::failure_line(line.split('\t').next().unwrap_or(line), "bad list line"));
    };
    let text = match csharp_source(root, path) {
        Ok(text) => text,
        Err(failure) => return (None, dump::failure_line(path, failure)),
    };
    let tree = parser.parse(&text, kind);
    if tree.has_no_result() {
        return (None, dump::failure_line(path, &format!("no {}", kind.entry.name())));
    }
    let Some(text) = dump::tree_to_string_within(&tree, dump::MAX_CHARS) else {
        return (None, dump::failure_line(path, "dump over 512 MB"));
    };
    (Some(text), dump::index_line(path, &tree))
}

/// `--check-bodies` of C# files (C# has no lazily parsed bodies): each file parses, without a panic, into a tree whose
/// leaves are its text; prints the differing ones and a last line `differing N  files M`, returns N. A file that is
/// not UTF-8 or that its entry cannot parse is not checked.
fn csharp_check_lossless(root: &Path, lines: &[String], threads: usize, stdout: &mut impl Write) -> usize {
    let schedule = Schedule::new(root, lines, threads);
    let mut differing = 0usize;
    let check = |parser: &mut csharp_syntax::Parser, i: usize| {
        let (path, kind) = csharp_line(&lines[i])?;
        let text = csharp_source(root, path).ok()?;
        let tree = parser.parse(&text, kind);
        if tree.has_no_result() {
            return None;
        }
        let covered: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).iter().copied()).collect();
        (covered != text).then(|| path.to_owned())
    };
    schedule.run(threads, csharp_syntax::Parser::new, check, |_, failed| {
        if let Some(path) = failed {
            let _ = writeln!(stdout, "{path}: the leaves are not the text");
            differing += 1;
        }
    });
    let _ = writeln!(stdout, "differing {differing}  files {}", lines.len());
    differing
}

/// `--lex` of a C# file: `PATH<TAB>TOKENS<TAB>HASH` as `tools/csharp-oracle lex` prints it; with `--full OUT` the
/// dump goes to `OUT/<path>.lex.txt`.
fn csharp_lex(parser: &mut csharp_syntax::Parser, root: &Path, line: &str, full: Option<&Path>) -> String {
    let path = line.split('\t').next().unwrap_or(line);
    let Some((_, kind)) = csharp_line(line) else { return format!("{path}\tfailure\tbad list line") };
    let text = match csharp_source(root, path) {
        Ok(text) => text,
        Err(failure) => return format!("{path}\tfailure\t{failure}"),
    };
    let tree = parser.lex(&text, kind);
    let Some(dump) = csharp_syntax::dump::lex_to_string_within(&tree, csharp_syntax::dump::MAX_CHARS) else {
        return format!("{path}\tfailure\tdump over 512 MB");
    };
    let mut hash = Fnv(0xcbf29ce484222325);
    hash.feed(dump.as_bytes());
    if let Some(full) = full {
        let target = full.join(format!("{path}.lex.txt"));
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&target, &dump);
    }
    format!("{path}\t{}\t{:016x}", csharp_syntax::dump::lexed_tokens(&tree), hash.0)
}

fn main() {
    let args = parse_args();
    let (Some(list), Some(root)) = (args.list.as_deref(), args.root.as_deref()) else { usage() };
    let paths = read_list(list);
    if args.lex && !args.bench {
        let stdout = std::io::stdout();
        let mut stdout = std::io::BufWriter::new(stdout.lock());
        let schedule = Schedule::new(root, &paths, args.workers());
        let lex = |parsers: &mut Parsers, i: usize| lex_file(parsers, root, &paths[i], &args);
        if let Some(lines) = args.check.as_deref() {
            if args.dump {
                usage();
            }
            let differing = check::lex(lines, &paths, &schedule, args.lang, args.workers(), lex, &mut stdout);
            let _ = stdout.flush();
            std::process::exit(if differing == 0 { 0 } else { 1 });
        }
        schedule.run(
            args.workers(),
            || Parsers::new(None),
            lex,
            |_, text| {
                let _ = stdout.write_all(text.as_bytes());
            },
        );
        return;
    }
    // A Java or C# list line may carry columns after its path (`PATH<TAB>LEVEL...`): the path decides the language.
    let lang_of = |line: &String| Lang::of(line.split('\t').next().unwrap_or(line), args.lang);
    let java_files = paths.iter().filter(|l| lang_of(l) == Lang::Java).count();
    let csharp_files = paths.iter().filter(|l| lang_of(l) == Lang::CSharp).count();
    let languages =
        (java_files != 0) as usize + (csharp_files != 0) as usize + (java_files + csharp_files != paths.len()) as usize;
    if (args.bench || args.check_bodies) && languages > 1 {
        eprintln!("the list mixes languages ({java_files} Java and {csharp_files} C# files of {})", paths.len());
        std::process::exit(2);
    }
    let stdout = std::io::stdout();
    let mut stdout = std::io::BufWriter::new(stdout.lock());
    if args.check_bodies && csharp_files != 0 {
        let differing = csharp_check_lossless(root, &paths, args.workers(), &mut stdout);
        let _ = stdout.flush();
        std::process::exit(if differing == 0 { 0 } else { 1 });
    }

    if args.lex && args.bench {
        if csharp_files != 0 {
            csharp_lex_bench(&args, root, &paths, &mut stdout);
            return;
        }
        lex_bench(&args, root, &paths, java_files != 0, &mut stdout);
        return;
    }

    if args.bench {
        if csharp_files != 0 {
            bench::<csharp_syntax::CSharp>(&args, root, &paths, &mut stdout);
        } else if java_files == 0 {
            bench::<Kotlin>(&args, root, &paths, &mut stdout);
        } else {
            bench::<java_syntax::Java>(&args, root, &paths, &mut stdout);
        }
        return;
    }

    if args.check_bodies {
        let differing = if java_files == 0 {
            check_bodies::<Kotlin>(root, &paths, args.nesting_limit, args.workers(), &mut stdout)
        } else {
            check_bodies::<java_syntax::Java>(root, &paths, args.nesting_limit, args.workers(), &mut stdout)
        };
        let _ = stdout.flush();
        std::process::exit(if differing == 0 { 0 } else { 1 });
    }

    let schedule = Schedule::new(root, &paths, args.workers());
    let init = || Parsers::new(args.nesting_limit);
    if let Some(oracle) = args.check.as_deref() {
        let dumps = |parsers: &mut Parsers, i: usize| dump_line(parsers, root, &paths[i], &args);
        let differing = check::dumps(oracle, args.out.as_deref(), &schedule, args.workers(), init, dumps, &mut stdout);
        let _ = stdout.flush();
        std::process::exit(if differing == 0 { 0 } else { 1 });
    }

    let Some(out) = args.out.as_deref() else { usage() };
    let mut failed = 0;
    let mut csharp_index = String::new();
    let write = |parsers: &mut Parsers, i: usize| {
        let dumps = dump_line(parsers, root, &paths[i], &args);
        (write_dumps(out, &dumps), dumps.index)
    };
    schedule.run(args.workers(), init, write, |_, (written, index)| {
        failed += !written as usize;
        if let Some(index) = index {
            csharp_index.push_str(&index);
            csharp_index.push('\n');
        }
    });
    if csharp_files != 0 {
        let _ = std::fs::create_dir_all(out);
        if let Err(e) = std::fs::write(out.join("index.jsonl"), csharp_index) {
            eprintln!("cannot write {}: {e}", out.join("index.jsonl").display());
        }
    }
    let _ = writeln!(stdout, "dumped {} of {} files to {}", paths.len() - failed, paths.len(), out.display());
}

/// A source file of the bench, read before timing.
struct Source<K> {
    path: String,
    kind: K,
    text: Vec<u8>,
}

/// What `--bench` asks of a language: its parser and how a list line parses.
trait BenchLanguage: TreeLanguage {
    /// The reusable parser (one per thread) and the tree it makes.
    type Parser;
    type Tree;
    /// A parser, with `--nesting-limit` when given and the language has one.
    fn new_parser(nesting_limit: Option<u32>) -> Self::Parser;
    fn parse(parser: &mut Self::Parser, text: &[u8], kind: Self::FileKind, options: ParseOptions) -> Self::Tree;
    /// The tree's nodes.
    fn psi(tree: &Self::Tree) -> &psi_builder::Tree<Self>;
    /// What a consumer pays to keep the tree.
    fn heap_bytes(tree: &Self::Tree) -> usize;
    /// The path of a list line and how its file is parsed.
    fn file_kind(line: &str) -> (&str, Self::FileKind);
    /// This thread's counters in `kotlin_syntax::counters::ALL` order, empty when the language has none.
    fn counters() -> Vec<u64>;
    /// A file's text as the oracles read it (Kotlin and Java: line separators converted to LF).
    fn read_text(root: &Path, path: &str, trim: bool) -> Option<Vec<u8>> {
        read_source(root, path, trim)
    }
}

/// What `--check-bodies` asks of a language beyond `psi_builder::Language`.
trait BodiesLanguage: Language {
    /// `--check-bodies`: the path of a list line, the text it parses and how.
    fn check_source(root: &Path, line: &str) -> Option<(String, Vec<u8>, Self::FileKind)>;
}

/// The `BenchLanguage` items of a language parsed by `psi_builder::Parser`.
macro_rules! psi_bench_language {
    () => {
        type Parser = psi_builder::Parser<Self>;
        type Tree = psi_builder::Tree<Self>;
        fn new_parser(nesting_limit: Option<u32>) -> Self::Parser {
            new_parser::<Self>(nesting_limit)
        }
        fn parse(parser: &mut Self::Parser, text: &[u8], kind: Self::FileKind, options: ParseOptions) -> Self::Tree {
            parser.parse_with_options(text, kind, options)
        }
        fn psi(tree: &Self::Tree) -> &psi_builder::Tree<Self> {
            tree
        }
        fn heap_bytes(tree: &Self::Tree) -> usize {
            tree.heap_bytes()
        }
    };
}

impl BenchLanguage for Kotlin {
    psi_bench_language!();
    fn file_kind(line: &str) -> (&str, FileKind) {
        (line, FileKind::from_file_name(file_name(line)))
    }
    fn counters() -> Vec<u64> {
        kotlin_syntax::counters::snapshot()
    }
}

impl BodiesLanguage for Kotlin {
    fn check_source(root: &Path, line: &str) -> Option<(String, Vec<u8>, FileKind)> {
        let text = read_source(root, line, false)?;
        Some((line.to_string(), text, FileKind::from_file_name(file_name(line))))
    }
}

impl BenchLanguage for csharp_syntax::CSharp {
    type Parser = csharp_syntax::Parser;
    type Tree = csharp_syntax::Tree;
    fn new_parser(_: Option<u32>) -> Self::Parser {
        csharp_syntax::Parser::new()
    }
    /// Every C# file parses in full: `options` (body-less parsing) does not apply.
    fn parse(parser: &mut Self::Parser, text: &[u8], kind: Self::FileKind, _: ParseOptions) -> Self::Tree {
        parser.parse(text, kind)
    }
    fn psi(tree: &Self::Tree) -> &psi_builder::Tree<Self> {
        tree.as_psi()
    }
    fn heap_bytes(tree: &Self::Tree) -> usize {
        tree.heap_bytes()
    }
    /// `PATH[<TAB>VERSION[<TAB>ENTRY[<TAB>DOC]]]`, defaults for a column that names nothing.
    fn file_kind(line: &str) -> (&str, csharp_syntax::FileKind) {
        csharp_line(line).unwrap_or((line.split('\t').next().unwrap_or(line), csharp_defaults()))
    }
    fn counters() -> Vec<u64> {
        Vec::new()
    }
    /// The bytes as they are, as `tools/csharp-oracle` reads them.
    fn read_text(root: &Path, path: &str, _: bool) -> Option<Vec<u8>> {
        std::fs::read(root.join(path)).ok()
    }
}

impl BenchLanguage for java_syntax::Java {
    psi_bench_language!();
    /// `PATH[<TAB>LEVEL]`: a file at LEVEL, else at `LanguageLevel.HIGHEST`.
    fn file_kind(line: &str) -> (&str, java_syntax::FileKind) {
        let mut columns = line.split('\t');
        let path = columns.next().unwrap_or(line);
        let level = columns
            .next()
            .and_then(java_syntax::LanguageLevel::from_name)
            .unwrap_or(java_syntax::LanguageLevel::HIGHEST);
        (path, java_syntax::FileKind::file(level))
    }
    fn counters() -> Vec<u64> {
        Vec::new()
    }
}

impl BodiesLanguage for java_syntax::Java {
    /// As `--out` reads it (`java_source`), the text as bytes even when it is not UTF-8.
    fn check_source(root: &Path, line: &str) -> Option<(String, Vec<u8>, java_syntax::FileKind)> {
        let (path, kind, trim) = java_line(line, false);
        let text = read_source(root, path, false)?;
        Some((path.to_string(), if trim { trim_bytes(text) } else { text }, kind))
    }
}

/// What one file of a bench round measured.
#[derive(Default, Clone, Copy)]
struct FileResult {
    parse_ns: u64,
    walk_ns: u64,
    linear_walk_ns: u64,
    nodes: usize,
    tree_bytes: usize,
    /// Allocations and reallocations during `Parser::parse` (`alloc-stats`; exact with one thread).
    allocs: u64,
    /// Bytes allocated during `Parser::parse`, and the bytes still live when it returns (the tree).
    alloc_bytes: u64,
    live_bytes: i64,
}

/// What one thread of a bench round measured.
#[derive(Default)]
struct ThreadResult {
    bytes: usize,
    files: usize,
    parse_seconds: f64,
    walk_seconds: f64,
    linear_walk_seconds: f64,
    checksum: u64,
    results: Vec<(usize, FileResult)>,
    /// This thread's `kotlin_syntax::counters` (`--features counters`).
    counters: Vec<u64>,
}

#[cfg(feature = "alloc-stats")]
fn alloc_snapshot() -> (u64, u64, i64) {
    let s = GLOBAL.stats();
    // A growing realloc counts its growth in `bytes_allocated`, a shrinking one in `bytes_deallocated`.
    let calls = (s.allocations + s.reallocations) as u64;
    let allocated = s.bytes_allocated as u64;
    let live = s.bytes_allocated as i64 - s.bytes_deallocated as i64;
    (calls, allocated, live)
}

#[cfg(not(feature = "alloc-stats"))]
fn alloc_snapshot() -> (u64, u64, i64) {
    (0, 0, 0)
}

/// The walk an extractor does: every node in preorder, its kind, its range and its depth; returns a checksum so it is
/// not optimized away.
fn walk<L: TreeLanguage>(tree: &psi_builder::Tree<L>) -> u64 {
    let mut acc = 0u64;
    for (id, depth) in tree.preorder(tree.root()) {
        let range = tree.range(id);
        acc = acc.wrapping_mul(0x100000001b3)
            ^ tree.kind(id).index() as u64
            ^ ((range.start as u64) << 9)
            ^ ((range.end as u64) << 37)
            ^ depth as u64;
    }
    acc
}

/// What a pass that needs no depth does: every node in id order (document order), its kind and its range.
fn linear_walk<L: TreeLanguage>(tree: &psi_builder::Tree<L>) -> u64 {
    let mut acc = 0u64;
    for id in tree.descendants(tree.root()) {
        let range = tree.range(id);
        acc = acc.wrapping_mul(0x100000001b3)
            ^ tree.kind(id).index() as u64
            ^ ((range.start as u64) << 9)
            ^ ((range.end as u64) << 37);
    }
    acc
}

/// A parser, with `--nesting-limit` when given.
fn new_parser<L: Language>(nesting_limit: Option<u32>) -> psi_builder::Parser<L> {
    let mut parser = psi_builder::Parser::new();
    if let Some(limit) = nesting_limit {
        parser.set_nesting_limit(limit);
    }
    parser
}

/// The subtree of `a` at `a_root` and that of `b` at `b_root` have the same nodes: kinds, ranges, error messages and
/// shape (what their dumps show, and the ranges).
fn same_subtree<L: Language>(
    a: &psi_builder::Tree<L>,
    a_root: NodeId,
    b: &psi_builder::Tree<L>,
    b_root: NodeId,
) -> bool {
    let size = a.subtree_end(a_root).0 - a_root.0;
    if b.subtree_end(b_root).0 - b_root.0 != size {
        return false;
    }
    (0..size).all(|k| {
        let (x, y) = (NodeId(a_root.0 + k), NodeId(b_root.0 + k));
        a.kind(x) == b.kind(y)
            && a.range(x) == b.range(y)
            && a.error_message(x) == b.error_message(y)
            && a.subtree_end(x).0 - x.0 == b.subtree_end(y).0 - y.0
            && (k == 0 || a.parent(x).map(|p| x.0 - p.0) == b.parent(y).map(|p| y.0 - p.0))
    })
}

/// What `--check-bodies` found in one file: its bodies, the differing ones and their report lines.
#[derive(Default)]
struct BodiesChecked {
    bodies: usize,
    differing: usize,
    report: String,
}

/// `--check-bodies`: returns the number of differing bodies.
fn check_bodies<L: BodiesLanguage>(
    root: &Path,
    lines: &[String],
    nesting_limit: Option<u32>,
    threads: usize,
    stdout: &mut impl Write,
) -> usize {
    let schedule = Schedule::new(root, lines, threads);
    let (mut bodies, mut differing) = (0usize, 0usize);
    let parsers = || (new_parser::<L>(nesting_limit), new_parser::<L>(nesting_limit));
    let check = |parsers: &mut (psi_builder::Parser<L>, psi_builder::Parser<L>), i: usize| {
        check_file_bodies::<L>(&mut parsers.0, &mut parsers.1, root, &lines[i])
    };
    schedule.run(threads, parsers, check, |_, checked| {
        bodies += checked.bodies;
        differing += checked.differing;
        let _ = stdout.write_all(checked.report.as_bytes());
    });
    let _ = writeln!(stdout, "bodies {bodies}  differing {differing}  files {}", lines.len());
    differing
}

/// `--check-bodies` of one list line: the full tree's leaves are the text, and `parse_body` of every body of the
/// body-less parse is the full tree's subtree there.
fn check_file_bodies<L: BodiesLanguage>(
    parser: &mut psi_builder::Parser<L>,
    body_parser: &mut psi_builder::Parser<L>,
    root: &Path,
    line: &str,
) -> BodiesChecked {
    use std::fmt::Write as _;
    let mut checked = BodiesChecked::default();
    let Some((path, text, kind)) = L::check_source(root, line) else { return checked };
    let full = parser.parse(&text, kind);
    let covered: Vec<u8> = full.tokens().flat_map(|id| full.text(id).iter().copied()).collect();
    if covered != text {
        let _ = writeln!(checked.report, "{path}: the leaves are not the text");
        checked.differing += 1;
    }
    let lazy = parser.parse_with_options(&text, kind, ParseOptions { expand_bodies: false });
    let (mut l, mut f) = (0u32, 0u32);
    while (l as usize) < lazy.len() && (f as usize) < full.len() {
        let (ln, fnode) = (NodeId(l), NodeId(f));
        if lazy.kind(ln) != full.kind(fnode) || lazy.range(ln) != full.range(fnode) {
            let _ = writeln!(checked.report, "{path}: node {l} {:?} differs from the full tree's {f}", lazy.range(ln));
            checked.differing += 1;
            break;
        }
        if lazy.is_unexpanded(ln) {
            checked.bodies += 1;
            let same = body_parser
                .parse_body(&lazy, ln, ParseOptions::default())
                .is_some_and(|body| same_subtree(&body, body.root(), &full, fnode));
            if !same {
                let _ = writeln!(checked.report, "{path}: body {:?} differs", lazy.range(ln));
                checked.differing += 1;
            }
            f = full.subtree_end(fnode).0;
        } else {
            f += 1;
        }
        l += 1;
    }
    checked
}

fn bench_thread<L: BenchLanguage>(
    sources: &[Source<L::FileKind>],
    next: &AtomicUsize,
    do_walk: bool,
    options: ParseOptions,
    nesting_limit: Option<u32>,
) -> ThreadResult {
    let mut r = ThreadResult::default();
    // One parser per thread, as an indexer keeps one per worker: its buffers are reused from file to file.
    let mut parser = L::new_parser(nesting_limit);
    loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some(source) = sources.get(i) else { break };
        let (calls0, allocated0, live0) = alloc_snapshot();
        let start = Instant::now();
        let parsed_tree = L::parse(&mut parser, &source.text, source.kind, options);
        let parsed = Instant::now();
        let (calls1, allocated1, live1) = alloc_snapshot();
        let tree = L::psi(&parsed_tree);
        let mut f = FileResult {
            parse_ns: (parsed - start).as_nanos() as u64,
            nodes: tree.len(),
            tree_bytes: L::heap_bytes(&parsed_tree),
            allocs: calls1 - calls0,
            alloc_bytes: allocated1 - allocated0,
            live_bytes: live1 - live0,
            ..FileResult::default()
        };
        if do_walk {
            let walk_start = Instant::now();
            r.checksum ^= walk(tree);
            let linear_start = Instant::now();
            f.walk_ns = (linear_start - walk_start).as_nanos() as u64;
            r.checksum ^= linear_walk(tree).rotate_left(1);
            f.linear_walk_ns = linear_start.elapsed().as_nanos() as u64;
        }
        drop(parsed_tree);
        r.bytes += source.text.len();
        r.files += 1;
        r.parse_seconds += f.parse_ns as f64 / 1e9;
        r.walk_seconds += f.walk_ns as f64 / 1e9;
        r.linear_walk_seconds += f.linear_walk_ns as f64 / 1e9;
        r.results.push((i, f));
    }
    r.counters = L::counters();
    r
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    if values.is_empty() { 0.0 } else { values[values.len() / 2] }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() { 0 } else { sorted[((sorted.len() - 1) as f64 * p).round() as usize] }
}

/// `--bench --lex`: the file lexer alone (Kotlin's `lexer::tokenize_into`, Java's at the line's level; one buffer
/// reused), one thread, per round; with `--docs` also the doc-comment lexer (`KDocLexer`, `JavaDocLexer`) over every
/// doc comment the file lexer found, as a parse lexes its doc chameleons, timed apart.
fn lex_bench(args: &Args, root: &Path, paths: &[String], java: bool, stdout: &mut impl Write) {
    use java_syntax::TokenKind as JavaToken;
    let sources: Vec<(Vec<u8>, java_syntax::LanguageLevel)> = paths
        .iter()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let path = columns.next().unwrap_or(line);
            let level = columns
                .next()
                .and_then(java_syntax::LanguageLevel::from_name)
                .unwrap_or(java_syntax::LanguageLevel::HIGHEST);
            read_source(root, path, args.trim).map(|text| (text, level))
        })
        .collect();
    let mb = sources.iter().map(|s| s.0.len()).sum::<usize>() as f64 / 1e6;
    let mut kotlin = kotlin_syntax::lexer::Lexed::default();
    let mut java_lexed = java_syntax::lexer::Lexed::default();
    // The doc comments of every file (file index, range), found once before the rounds.
    let mut docs: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
    if args.docs {
        for (i, (text, level)) in sources.iter().enumerate() {
            if java {
                java_syntax::lexer::tokenize_into(text, *level, &mut java_lexed);
                for (t, kind) in java_lexed.types.iter().enumerate() {
                    if matches!(kind, JavaToken::DOC_COMMENT | JavaToken::DOC_MARKDOWN_COMMENT) {
                        docs.push((i, java_lexed.starts[t] as usize..java_lexed.starts[t + 1] as usize));
                    }
                }
            } else {
                kotlin_syntax::lexer::tokenize_into(text, &mut kotlin);
                for (t, kind) in kotlin.types.iter().enumerate() {
                    if *kind == TokenKind::DOC_COMMENT {
                        docs.push((i, kotlin.starts[t] as usize..kotlin.starts[t + 1] as usize));
                    }
                }
            }
        }
    }
    let doc_mb = docs.iter().map(|d| d.1.len()).sum::<usize>() as f64 / 1e6;
    let (mut kdoc_types, mut java_doc_types, mut doc_starts) = (Vec::new(), Vec::new(), Vec::new());
    let mut round_mbs = Vec::new();
    for round in 1..=args.repeat {
        let (mut tokens, mut seconds) = (0usize, 0.0);
        for (text, level) in &sources {
            let start = Instant::now();
            if java {
                java_syntax::lexer::tokenize_into(text, *level, &mut java_lexed);
            } else {
                kotlin_syntax::lexer::tokenize_into(text, &mut kotlin);
            }
            seconds += start.elapsed().as_secs_f64();
            tokens += if java { java_lexed.len() } else { kotlin.len() };
        }
        let mbs = mb / seconds.max(1e-9);
        round_mbs.push(mbs);
        let prefix = if args.repeat > 1 { format!("round {round}  ") } else { String::new() };
        let _ = writeln!(
            stdout,
            "{prefix}files {}  MB {mb:.2}  seconds {seconds:.3}  MB/s {mbs:.1}  tokens {tokens}",
            sources.len()
        );
        if args.docs {
            let (mut doc_tokens, start) = (0usize, Instant::now());
            for (i, range) in &docs {
                let text = &sources[*i].0[range.clone()];
                if java {
                    java_syntax::doc_lexer::tokenize_into(text, true, &mut java_doc_types, &mut doc_starts);
                    doc_tokens += java_doc_types.len();
                } else {
                    kotlin_syntax::kdoc::lexer::tokenize_into(text, &mut kdoc_types, &mut doc_starts);
                    doc_tokens += kdoc_types.len();
                }
            }
            let doc_seconds = start.elapsed().as_secs_f64();
            let _ = writeln!(
                stdout,
                "{prefix}docs {}  MB {doc_mb:.2}  seconds {doc_seconds:.3}  MB/s {:.1}  tokens {doc_tokens}",
                docs.len(),
                doc_mb / doc_seconds.max(1e-9)
            );
        }
    }
    if args.repeat > 1 {
        let _ = writeln!(stdout, "median MB/s {:.1} of {} rounds", median(&mut round_mbs), args.repeat);
    }
}

/// `--lex --bench` of C# files: the lexer alone (`Parser::lex_count`, every token of `SyntaxFactory.ParseTokens` with
/// its trivia, directives and diagnostics, no tree), over the valid UTF-8 files of the list.
fn csharp_lex_bench(args: &Args, root: &Path, paths: &[String], stdout: &mut impl Write) {
    let sources: Vec<(Vec<u8>, csharp_syntax::FileKind)> = paths
        .iter()
        .filter_map(|line| {
            let (path, kind) = csharp_line(line)?;
            csharp_source(root, path).ok().map(|text| (text, kind))
        })
        .collect();
    let mb = sources.iter().map(|s| s.0.len()).sum::<usize>() as f64 / 1e6;
    let mut parser = csharp_syntax::Parser::new();
    let mut round_mbs = Vec::new();
    for round in 1..=args.repeat {
        let (mut tokens, mut seconds) = (0usize, 0.0);
        for (text, kind) in &sources {
            let start = Instant::now();
            tokens += parser.lex_count(text, *kind);
            seconds += start.elapsed().as_secs_f64();
        }
        let mbs = mb / seconds.max(1e-9);
        round_mbs.push(mbs);
        let prefix = if args.repeat > 1 { format!("round {round}  ") } else { String::new() };
        let _ = writeln!(
            stdout,
            "{prefix}files {}  MB {mb:.2}  seconds {seconds:.3}  MB/s {mbs:.1}  tokens {tokens}",
            sources.len()
        );
    }
    if args.repeat > 1 {
        let _ = writeln!(stdout, "median MB/s {:.1} of {} rounds", median(&mut round_mbs), args.repeat);
    }
}

fn bench<L: BenchLanguage>(args: &Args, root: &Path, paths: &[String], stdout: &mut impl Write) {
    let sources: Vec<Source<L::FileKind>> = paths
        .iter()
        .filter_map(|line| {
            let (path, kind) = L::file_kind(line);
            L::read_text(root, path, args.trim).map(|text| Source { path: path.to_owned(), kind, text })
        })
        .collect();
    let bytes: usize = sources.iter().map(|s| s.text.len()).sum();
    let mb = bytes as f64 / 1e6;
    let threads = args.threads.unwrap_or(1);
    let options = ParseOptions { expand_bodies: !args.no_bodies };
    if cfg!(feature = "alloc-stats") {
        let _ = writeln!(stdout, "alloc-stats: timings include the counting allocator");
    }
    let mut round_mbs = Vec::new();
    let mut last: Vec<FileResult> = Vec::new();
    let mut last_counters: Vec<u64> = Vec::new();
    for round in 1..=args.repeat {
        let next = AtomicUsize::new(0);
        let start = Instant::now();
        let per_thread: Vec<ThreadResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|_| {
                    std::thread::Builder::new().spawn_scoped(scope, || {
                        bench_thread::<L>(&sources, &next, args.walk, options, args.nesting_limit)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.ok().and_then(|h| h.join().ok()).unwrap_or_default()).collect()
        });
        let wall = start.elapsed().as_secs_f64();
        let mut results = vec![FileResult::default(); sources.len()];
        for t in &per_thread {
            for &(i, f) in &t.results {
                results[i] = f;
            }
        }
        let nodes: usize = results.iter().map(|f| f.nodes).sum();
        let parse_seconds: f64 = per_thread.iter().map(|t| t.parse_seconds).sum();
        // One thread: the summed parse time, as before; more: the wall time of the round.
        let seconds = if threads == 1 { parse_seconds } else { wall };
        let mbs = mb / seconds.max(1e-9);
        round_mbs.push(mbs);
        let prefix = if args.repeat > 1 { format!("round {round}  ") } else { String::new() };
        let _ = writeln!(
            stdout,
            "{prefix}files {}  MB {:.2}  seconds {:.3}  MB/s {:.1}  nodes {}",
            sources.len(),
            mb,
            seconds,
            mbs,
            nodes
        );
        if threads > 1 {
            let mut each: Vec<f64> =
                per_thread.iter().map(|t| t.bytes as f64 / 1e6 / t.parse_seconds.max(1e-9)).collect();
            let min = each.iter().copied().fold(f64::INFINITY, f64::min);
            let max = each.iter().copied().fold(0.0, f64::max);
            let _ = writeln!(
                stdout,
                "  threads {threads}  wall {wall:.3}  parse-busy {parse_seconds:.3}  per-thread parse MB/s min {min:.1} \
                 median {:.1} max {max:.1}  sum {:.1}",
                median(&mut each),
                each.iter().sum::<f64>()
            );
        }
        if args.walk {
            let walk_seconds: f64 = per_thread.iter().map(|t| t.walk_seconds).sum();
            let checksum = per_thread.iter().fold(0, |a, t| a ^ t.checksum);
            let _ = writeln!(
                stdout,
                "  walk seconds {walk_seconds:.3} (summed over threads)  MB/s {:.1}  ns/node {:.2}  of parse {:.1}%  \
                 checksum {checksum:x}",
                mb / walk_seconds.max(1e-9),
                walk_seconds * 1e9 / nodes.max(1) as f64,
                100.0 * walk_seconds / parse_seconds.max(1e-9)
            );
            let linear_seconds: f64 = per_thread.iter().map(|t| t.linear_walk_seconds).sum();
            let _ = writeln!(
                stdout,
                "  linear walk seconds {linear_seconds:.3} (summed over threads)  MB/s {:.1}  ns/node {:.2}  of parse \
                 {:.1}%",
                mb / linear_seconds.max(1e-9),
                linear_seconds * 1e9 / nodes.max(1) as f64,
                100.0 * linear_seconds / parse_seconds.max(1e-9)
            );
        }
        if counters::ENABLED && !per_thread.iter().all(|t| t.counters.is_empty()) {
            let mut total = vec![0u64; counters::ALL.len()];
            for t in &per_thread {
                for (sum, c) in total.iter_mut().zip(&t.counters) {
                    *sum += c;
                }
            }
            last_counters = total;
        }
        last = results;
    }
    if args.repeat > 1 {
        let _ = writeln!(stdout, "median MB/s {:.1} of {} rounds", median(&mut round_mbs), args.repeat);
    }

    let nodes: usize = last.iter().map(|f| f.nodes).sum();
    let tree_bytes: usize = last.iter().map(|f| f.tree_bytes).sum();
    let max_tree = last.iter().map(|f| f.tree_bytes).max().unwrap_or(0);
    let _ = writeln!(
        stdout,
        "tree bytes {tree_bytes}  per source MB {:.2} MB  bytes/node {:.1}  nodes per source KB {:.1}  largest tree {:.1} MB",
        tree_bytes as f64 / bytes.max(1) as f64,
        tree_bytes as f64 / nodes.max(1) as f64,
        nodes as f64 * 1e3 / bytes.max(1) as f64,
        max_tree as f64 / 1e6
    );
    if cfg!(feature = "alloc-stats") {
        let mut allocs: Vec<u64> = last.iter().map(|f| f.allocs).collect();
        allocs.sort_unstable();
        let total: u64 = allocs.iter().sum();
        let allocated: u64 = last.iter().map(|f| f.alloc_bytes).sum();
        let live: i64 = last.iter().map(|f| f.live_bytes).sum();
        let exact = if threads == 1 { "" } else { " (other threads' allocations mixed in: use --threads 1)" };
        let _ = writeln!(
            stdout,
            "allocations {total}  per file median {} p90 {} p99 {} max {}  per source KB {:.2}  allocated bytes per \
             source byte {:.2}  live after parse per source byte {:.2}{exact}",
            percentile(&allocs, 0.5),
            percentile(&allocs, 0.9),
            percentile(&allocs, 0.99),
            allocs.last().copied().unwrap_or(0),
            total as f64 * 1e3 / bytes.max(1) as f64,
            allocated as f64 / bytes.max(1) as f64,
            live as f64 / bytes.max(1) as f64
        );
    }
    if counters::ENABLED && !last_counters.is_empty() {
        let file_tokens_at = counters::ALL.iter().position(|&c| c == AnyCounter::Builder(BuilderCounter::FileTokens));
        let file_tokens = file_tokens_at.map_or(0, |i| last_counters[i]).max(1) as f64;
        let _ = writeln!(stdout, "counters (last round; per file token):");
        for (k, v) in counters::ALL.iter().zip(&last_counters) {
            let _ = writeln!(stdout, "  {:<20} {:>12}  {:>8.3}", format!("{k:?}"), v, *v as f64 / file_tokens);
        }
    }
    if let Some(out) = &args.file_stats {
        let mut text =
            String::from("path\tbytes\tparse_us\twalk_us\tlinear_walk_us\tnodes\ttree_bytes\tallocs\talloc_bytes\n");
        for (s, f) in sources.iter().zip(&last) {
            text.push_str(&format!(
                "{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{}\n",
                s.path,
                s.text.len(),
                f.parse_ns as f64 / 1e3,
                f.walk_ns as f64 / 1e3,
                f.linear_walk_ns as f64 / 1e3,
                f.nodes,
                f.tree_bytes,
                f.allocs,
                f.alloc_bytes
            ));
        }
        if let Err(e) = std::fs::write(out, text) {
            eprintln!("cannot write {}: {e}", out.display());
        }
    }
}
