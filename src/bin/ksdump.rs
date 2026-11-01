//! `ksdump`: dumps, benchmarks and lexes listed Kotlin files.
//!
//! - `ksdump --list LIST --root ROOT --out OUT [--trim]`: writes `OUT/<path>.psi.txt` per listed path, the
//!   `psi_to_string` dump, like the PSI oracle (`PsiOracle.java`) writes its own; compare with `scripts/compare.py`.
//! - `ksdump --bench --list LIST --root ROOT`: reads every file, then parses them all on one thread and prints the
//!   files, megabytes, seconds and MB/s of the parse alone.
//! - `ksdump --lex --list LIST --root ROOT [--dump]`: one line per file as `scripts/LexOracle.java` prints it.
//!
//! The text is read as bytes with CRLF and CR converted to LF, as the oracles do; `--trim` also trims it (what the
//! compiler's parsing tests parse). A path ending in `.kt` (or without an extension) parses as a file, any other as a
//! script.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kotlin_syntax::dump::psi_to_string;
use kotlin_syntax::kdoc::lexer::KDocLexer;
use kotlin_syntax::lexer::KotlinLexer;
use kotlin_syntax::tokens::TokenKind;
use kotlin_syntax::{FileKind, Tree};

fn usage() -> ! {
    eprintln!(
        "usage:\n  \
         ksdump --list LIST --root ROOT --out OUT [--trim]   write OUT/<path>.psi.txt per listed file\n  \
         ksdump --bench --list LIST --root ROOT [--trim]     parse-only throughput, one thread\n  \
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
}

fn parse_args() -> Args {
    let mut args = Args::default();
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
        let sources: Vec<(FileKind, Vec<u8>)> = paths
            .iter()
            .filter_map(|p| read_source(root, p, args.trim).map(|t| (FileKind::from_file_name(file_name(p)), t)))
            .collect();
        let bytes: usize = sources.iter().map(|(_, t)| t.len()).sum();
        let start = Instant::now();
        let mut nodes = 0usize;
        for (kind, text) in &sources {
            nodes += Tree::parse(text, *kind).len();
        }
        let seconds = start.elapsed().as_secs_f64();
        let mb = bytes as f64 / 1e6;
        let _ = writeln!(
            stdout,
            "files {}  MB {:.2}  seconds {:.3}  MB/s {:.1}  nodes {}",
            sources.len(),
            mb,
            seconds,
            mb / seconds.max(1e-9),
            nodes
        );
        return;
    }

    let Some(out) = args.out.as_deref() else { usage() };
    let mut failed = 0;
    for path in &paths {
        let Some(text) = read_source(root, path, args.trim) else {
            eprintln!("cannot read {path}");
            failed += 1;
            continue;
        };
        let name = file_name(path);
        let tree = Tree::parse(&text, FileKind::from_file_name(name));
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
