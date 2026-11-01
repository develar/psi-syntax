//! `ksdump`: dumps, benchmarks and lexes listed Kotlin files.
//!
//! - `ksdump --list LIST --root ROOT --out OUT [--trim]`: writes `OUT/<path>.psi.txt` per listed path, the
//!   `psi_to_string` dump, like the PSI oracle (`PsiOracle.java`) writes its own; compare with `scripts/compare.py`.
//! - `ksdump --bench --list LIST --root ROOT [--threads N] [--walk] [--no-bodies] [--repeat K] [--file-stats OUT.tsv]`: reads
//!   every file, then parses them all and prints the files, megabytes, seconds and MB/s of the parse alone (the first
//!   line, one per round), then the tree's heap bytes per source MB. `--threads N` parses on N threads taking files
//!   from a shared counter, one `Parser` per thread, as an indexer does, and prints the wall MB/s and each thread's own parse MB/s; `--walk`
//!   also walks every tree in preorder reading each node's kind, range and depth, as an extractor does, and then
//!   scans its nodes in id order (`descendants`) reading kind and range, each timed apart; `--no-bodies` parses
//!   with `ParseOptions { expand_bodies: false }`, as a declaration-only pass does;
//!   `--repeat K` runs K rounds and prints their median; `--file-stats` writes one line per file of the last round.
//!   Built with `--features alloc-stats`, a counting allocator adds the allocations per file (exact with one thread)
//!   and the bytes allocated per source MB; it slows the parse, so time without it. With `--features counters`, the
//!   builder's calls (`tt()`, lookahead, markers, rollbacks) and the tokens lexed, reused and skipped, per file token. Peak RSS: run under
//!   `/usr/bin/time -l` (the sources stay loaded: subtract their MB).
//! - `ksdump --lex --list LIST --root ROOT [--dump]`: one line per file as `scripts/LexOracle.java` prints it.
//! - `ksdump --check-bodies --list LIST --root ROOT`: parses each file in full and body-less and checks that
//!   `parse_body` of every unexpanded body dumps as the full tree's subtree there; prints the bodies, the differing
//!   ones (their paths and ranges) and exits nonzero on any.
//!
//! The text is read as bytes with CRLF and CR converted to LF, as the oracles do; `--trim` also trims it (what the
//! compiler's parsing tests parse). A path ending in `.kt` (or without an extension) parses as a file, any other as a
//! script.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use kotlin_syntax::counters::{self, Counter};
use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::dump::subtree_to_string;
use kotlin_syntax::kdoc::lexer::KDocLexer;
use kotlin_syntax::lexer::KotlinLexer;
use kotlin_syntax::tokens::TokenKind;
use kotlin_syntax::{ElementType, FileKind, NodeId, ParseOptions, Parser, Tree};

/// Counts every allocation of the process (`--features alloc-stats` only).
#[cfg(feature = "alloc-stats")]
#[global_allocator]
static GLOBAL: stats_alloc::StatsAlloc<std::alloc::System> = stats_alloc::StatsAlloc::system();

fn usage() -> ! {
    eprintln!(
        "usage:\n  \
         ksdump --list LIST --root ROOT --out OUT [--trim]   write OUT/<path>.psi.txt per listed file\n  \
         ksdump --bench --list LIST --root ROOT [--trim] [--threads N] [--walk] [--no-bodies] [--repeat K]\n  \
                [--file-stats OUT]                           parse throughput, tree bytes, walk cost\n  \
         ksdump --check-bodies --list LIST --root ROOT       parse_body of each body against the full parse\n  \
         ksdump --lex --list LIST --root ROOT [--dump]       lexer hash lines as scripts/LexOracle.java prints them"
    );
    std::process::exit(2)
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
    threads: usize,
    walk: bool,
    no_bodies: bool,
    check_bodies: bool,
    repeat: usize,
    file_stats: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut args = Args { threads: 1, repeat: 1, ..Args::default() };
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
            "--threads" => args.threads = count(it.next()),
            "--walk" => args.walk = true,
            "--no-bodies" => args.no_bodies = true,
            "--check-bodies" => args.check_bodies = true,
            "--repeat" => args.repeat = count(it.next()),
            "--file-stats" => args.file_stats = Some(it.next().map(PathBuf::from).unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }
    args
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

/// The hash and token count `LexOracle.java` prints (KDoc tokens hashed inside `[` `]` after their doc comment).
fn lex_line(text: &[u8], dump: bool, out: &mut String) -> (u64, usize) {
    let mut hash = Fnv(0xcbf29ce484222325);
    let mut count = 0;
    let mut lexer = KotlinLexer::new(text);
    while let Some(t) = lexer.advance() {
        let token_text = &text[t.start..t.end];
        hash.feed(format!("{}:{};", t.kind.debug_name(), token_text.len()).as_bytes());
        if dump {
            out.push_str(&format!("  {} {}\n", t.kind.debug_name(), escape(token_text)));
        }
        if t.kind == TokenKind::DOC_COMMENT {
            hash.feed(b"[");
            let mut kdoc = KDocLexer::new(token_text);
            while let Some(k) = kdoc.advance() {
                let k_text = &token_text[k.start..k.end];
                hash.feed(format!("{}:{};", k.kind.debug_name(), k_text.len()).as_bytes());
                if dump {
                    out.push_str(&format!("    {} {}\n", k.kind.debug_name(), escape(k_text)));
                }
                count += 1;
            }
            hash.feed(b"]");
        }
        count += 1;
    }
    (hash.0, count)
}

fn main() {
    let args = parse_args();
    let (Some(list), Some(root)) = (args.list.as_deref(), args.root.as_deref()) else { usage() };
    let paths = read_list(list);
    let stdout = std::io::stdout();
    let mut stdout = std::io::BufWriter::new(stdout.lock());

    if args.lex {
        for path in &paths {
            let Some(text) = read_source(root, path, false) else {
                let _ = writeln!(stdout, "{path}\tunreadable\t0");
                continue;
            };
            let mut dump = String::new();
            let (hash, count) = lex_line(&text, args.dump, &mut dump);
            let _ = writeln!(stdout, "{path}\t{hash:x}\t{count}");
            let _ = stdout.write_all(dump.as_bytes());
        }
        return;
    }

    if args.bench {
        bench(&args, root, &paths, &mut stdout);
        return;
    }

    if args.check_bodies {
        let differing = check_bodies(root, &paths, &mut stdout);
        let _ = stdout.flush();
        std::process::exit(if differing == 0 { 0 } else { 1 });
    }

    let Some(out) = args.out.as_deref() else { usage() };
    let mut failed = 0;
    let mut parser = Parser::new();
    for path in &paths {
        let Some(text) = read_source(root, path, args.trim) else {
            eprintln!("cannot read {path}");
            failed += 1;
            continue;
        };
        let name = file_name(path);
        let tree = parser.parse(&text, FileKind::from_file_name(name));
        let dump = psi_to_string(&tree, name);
        let target = out.join(format!("{path}.psi.txt"));
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&target, dump) {
            eprintln!("cannot write {}: {e}", target.display());
            failed += 1;
        }
    }
    let _ = writeln!(stdout, "dumped {} of {} files to {}", paths.len() - failed, paths.len(), out.display());
}

/// A source file of the bench, read before timing.
struct Source {
    path: String,
    kind: FileKind,
    text: Vec<u8>,
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

fn element_index(kind: ElementType) -> u64 {
    match kind {
        ElementType::Token(t) => t as u64,
        ElementType::Node(n) => 256 + n as u64,
    }
}

/// The walk an extractor does: every node in preorder, its kind, its range and its depth; returns a checksum so it is
/// not optimized away.
fn walk(tree: &Tree) -> u64 {
    let mut acc = 0u64;
    for (id, depth) in tree.preorder(tree.root()) {
        let range = tree.range(id);
        acc = acc.wrapping_mul(0x100000001b3)
            ^ element_index(tree.kind(id))
            ^ ((range.start as u64) << 9)
            ^ ((range.end as u64) << 37)
            ^ depth as u64;
    }
    acc
}

/// What a pass that needs no depth does: every node in id order (document order), its kind and its range.
fn linear_walk(tree: &Tree) -> u64 {
    let mut acc = 0u64;
    for id in tree.descendants(tree.root()) {
        let range = tree.range(id);
        acc = acc.wrapping_mul(0x100000001b3)
            ^ element_index(tree.kind(id))
            ^ ((range.start as u64) << 9)
            ^ ((range.end as u64) << 37);
    }
    acc
}

/// `--check-bodies`: returns the number of differing bodies.
fn check_bodies(root: &Path, paths: &[String], stdout: &mut impl Write) -> usize {
    let mut parser = Parser::new();
    let mut body_parser = Parser::new();
    let (mut bodies, mut differing) = (0usize, 0usize);
    for path in paths {
        let Some(text) = read_source(root, path, false) else { continue };
        let kind = FileKind::from_file_name(file_name(path));
        let full = parser.parse(&text, kind);
        let lazy = parser.parse_with_options(&text, kind, ParseOptions { expand_bodies: false });
        let (mut l, mut f) = (0u32, 0u32);
        while (l as usize) < lazy.len() && (f as usize) < full.len() {
            let (ln, fnode) = (NodeId(l), NodeId(f));
            if lazy.kind(ln) != full.kind(fnode) || lazy.range(ln) != full.range(fnode) {
                let _ = writeln!(stdout, "{path}: node {l} {:?} differs from the full tree's {f}", lazy.range(ln));
                differing += 1;
                break;
            }
            if lazy.is_unexpanded(ln) {
                bodies += 1;
                let same = body_parser
                    .parse_body(&lazy, ln, ParseOptions::default())
                    .is_some_and(|body| subtree_to_string(&body, body.root()) == subtree_to_string(&full, fnode));
                if !same {
                    let _ = writeln!(stdout, "{path}: body {:?} differs", lazy.range(ln));
                    differing += 1;
                }
                f = full.subtree_end(fnode).0;
            } else {
                f += 1;
            }
            l += 1;
        }
    }
    let _ = writeln!(stdout, "bodies {bodies}  differing {differing}  files {}", paths.len());
    differing
}

fn bench_thread(sources: &[Source], next: &AtomicUsize, do_walk: bool, options: ParseOptions) -> ThreadResult {
    let mut r = ThreadResult::default();
    // One parser per thread, as an indexer keeps one per worker: its buffers are reused from file to file.
    let mut parser = Parser::new();
    loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some(source) = sources.get(i) else { break };
        let (calls0, allocated0, live0) = alloc_snapshot();
        let start = Instant::now();
        let tree: Tree = parser.parse_with_options(&source.text, source.kind, options);
        let parsed = Instant::now();
        let (calls1, allocated1, live1) = alloc_snapshot();
        let mut f = FileResult {
            parse_ns: (parsed - start).as_nanos() as u64,
            nodes: tree.len(),
            tree_bytes: tree.heap_bytes(),
            allocs: calls1 - calls0,
            alloc_bytes: allocated1 - allocated0,
            live_bytes: live1 - live0,
            ..FileResult::default()
        };
        if do_walk {
            let walk_start = Instant::now();
            r.checksum ^= walk(&tree);
            let linear_start = Instant::now();
            f.walk_ns = (linear_start - walk_start).as_nanos() as u64;
            r.checksum ^= linear_walk(&tree).rotate_left(1);
            f.linear_walk_ns = linear_start.elapsed().as_nanos() as u64;
        }
        drop(tree);
        r.bytes += source.text.len();
        r.files += 1;
        r.parse_seconds += f.parse_ns as f64 / 1e9;
        r.walk_seconds += f.walk_ns as f64 / 1e9;
        r.linear_walk_seconds += f.linear_walk_ns as f64 / 1e9;
        r.results.push((i, f));
    }
    r.counters = kotlin_syntax::counters::snapshot();
    r
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    if values.is_empty() { 0.0 } else { values[values.len() / 2] }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() { 0 } else { sorted[((sorted.len() - 1) as f64 * p).round() as usize] }
}

fn bench(args: &Args, root: &Path, paths: &[String], stdout: &mut impl Write) {
    let sources: Vec<Source> = paths
        .iter()
        .filter_map(|p| {
            read_source(root, p, args.trim).map(|text| Source {
                path: p.clone(),
                kind: FileKind::from_file_name(file_name(p)),
                text,
            })
        })
        .collect();
    let bytes: usize = sources.iter().map(|s| s.text.len()).sum();
    let mb = bytes as f64 / 1e6;
    let threads = args.threads;
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
            let handles: Vec<_> =
                (0..threads).map(|_| scope.spawn(|| bench_thread(&sources, &next, args.walk, options))).collect();
            handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
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
        if counters::ENABLED {
            let mut total = vec![0u64; Counter::ALL.len()];
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
    if counters::ENABLED {
        let file_tokens = last_counters[Counter::FileTokens as usize].max(1) as f64;
        let _ = writeln!(stdout, "counters (last round; per file token):");
        for (k, v) in Counter::ALL.iter().zip(&last_counters) {
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
