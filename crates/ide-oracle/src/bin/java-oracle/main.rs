//! `java-oracle`: the tooling of the Java PSI oracle (`tools/java-oracle/`): IntelliJ's own Java parser and lexer,
//! run on the pinned jars of `jars.sha256` with the IDE's JVM, the goldens vendored from intellij-community at the
//! parser's revision, and the corpora the gate compares (`tools/java-oracle/README.md` has every command).
//!
//! `cargo run --release -q -p ide-oracle --bin java-oracle -- <command>`.
mod mutate;
mod unmarked;
mod vendor;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ide_oracle::jvm::{Jvm, Streams};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "java-oracle", about = "The Java PSI oracle's tooling: pinned jars, oracle runs, goldens, corpora")]
struct Cli {
    /// The IntelliJ installation (its `Contents`) whose jars `jars.sha256` pins and whose JBR runs the oracles
    /// (`$JAVA` overrides the JVM).
    #[arg(long, global = true, default_value = ide_oracle::jvm::DEFAULT_IDE)]
    ide: PathBuf,
    /// Where the pinned jars are cached.
    #[arg(long, global = true)]
    lib: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Copies the jars of `jars.sha256` to the cache, checks them and prints the directory (`-cp "$LIB/*"`).
    Jars,
    /// Runs `JavaPsiOracle.java` (with `-Xss1g -Xmx8g`) with the given arguments.
    Psi {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Runs `JavaLexOracle.java` with the given arguments, its lines to `--lines` (else stdout).
    Lex {
        /// The file the lines go to; a failed write fails the run.
        #[arg(long)]
        lines: Option<PathBuf>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Writes `testdata/java/` from intellij-community at the revision (`git cat-file`, never the working tree).
    VendorGoldens {
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, default_value = vendor::REV)]
        rev: String,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Compares a `JavaPsiOracle.java --node` run over `manifest.tsv` with the goldens, byte for byte.
    CheckGoldens {
        /// The oracle's output (default `~/.cache/psi-syntax/oracle/java-goldens`).
        out: Option<PathBuf>,
        #[arg(long)]
        goldens: Option<PathBuf>,
        /// Runs the oracle over the manifest into OUT first.
        #[arg(long)]
        run: bool,
    },
    /// Prints the tracked `.java` files of a checkout without IDE test markup.
    Unmarked { checkout: PathBuf },
    /// One-place mutations of the PSI-clean goldens into OUT (`origins.json`, `list`, `original/`).
    Mutate {
        out: PathBuf,
        #[arg(long)]
        goldens: Option<PathBuf>,
        #[arg(long, default_value_t = 2)]
        per_file: usize,
        #[arg(long, default_value = "java-mut")]
        seed: String,
    },
    /// Lexer fuzz inputs: OUT/NNNNN.java (golden inputs with lexer-state pieces inserted, replaced or deleted) and
    /// OUT/list.
    LexFuzz {
        goldens: PathBuf,
        out: PathBuf,
        #[arg(default_value_t = 5000)]
        count: usize,
        #[arg(default_value = "java-lex")]
        seed: String,
    },
}

/// The lexer fuzz's pieces: what changes the Java and JavaDoc lexers' states (unicode escapes, backslashes, quotes,
/// text blocks, string templates, comment openers and closers, the JavaDoc tag and markdown characters, non-ASCII
/// letters and whitespace, a lone surrogate's escape).
const LEX_PIECES: [&str; 106] = [
    "\\", "\\\\", "\\u", "\\u0041", "\\uuu005c", "\\u005c", "\\u0022", "\\u0027", "\\u000a", "\\u001a", "\\u007b", "\\u007d",
    "\\u002f", "\\u002a", "\\ud800", "\\{", "\\}", "\"", "'", "\"\"\"", "\"\"\"\n", "\n", "\r\n", "\t", "\u{c}", " ", "\u{1a}",
    "{", "}", "/", "*", "/*", "*/", "/**", "/**/", "//", "///", "///\n", "#", "#!", "@", "@param ", "@see ", "@value ",
    "@code ", "@literal ", "@snippet ", "@return ", "{@", "{@code ", "{@link ", "{@snippet :\n", "<", ">", "<T>", "(", ")",
    "[", "]", ",", ".", "..", "...", "-", "--", "->", "::", "%d", "%", "`", "```", "~~~", "=", ":", "?", "!", "1", "0x", "0b",
    "1e", "1.", ".5", "0x1p", "1L", "_", "$", "\u{e9}", "\u{df}", "\u{1c5}", "\u{a0}", "\u{2028}", "\u{3000}", "\u{300}",
    "\u{1f600}", "\u{0}", "\u{1}", "\u{7f}", "assert", "enum", "non-sealed", "record", "\\[", "\\]", "\\`", "\\(", "\\)",
];

/// psi-syntax's root, this crate's `../..` (in accio and in the mirror alike).
fn psi_syntax() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(Path::parent).unwrap_or(manifest).to_path_buf()
}

fn tools() -> PathBuf {
    psi_syntax().join("tools/java-oracle")
}

fn goldens_dir(given: Option<PathBuf>) -> PathBuf {
    given.unwrap_or_else(|| psi_syntax().join("testdata/java"))
}

/// The pinned jars, fetched and checked.
fn jars(cli: &Cli) -> Result<PathBuf> {
    let lib = cli.lib.clone().unwrap_or_else(|| ide_oracle::cache_home().join("psi-syntax/java-oracle/lib"));
    ide_oracle::jars::fetch(&tools().join("jars.sha256"), &cli.ide, &lib)
}

fn jvm(cli: &Cli) -> Result<Jvm> {
    Ok(Jvm::new(ide_oracle::jvm::java(&cli.ide), &jars(cli)?))
}

fn psi_oracle(cli: &Cli) -> Result<Jvm> {
    Ok(jvm(cli)?.option("-Xss1g").option("-Xmx8g"))
}

fn os_args(args: &[OsString]) -> Vec<&OsStr> {
    args.iter().map(OsString::as_os_str).collect()
}

/// `NAME.java.psi.txt` against `NAME.txt` and `NAME.java.node.txt` against `NAME_node.txt` for every manifest line;
/// prints the counts and each differing file with its first differing line.
fn check_goldens(out: &Path, goldens: &Path) -> Result<bool> {
    let manifest = std::fs::read_to_string(goldens.join("manifest.tsv")).context("reading manifest.tsv")?;
    let mut pass = true;
    for (kind, golden_suffix, dump_suffix) in [("psi", ".txt", ".psi.txt"), ("node", "_node.txt", ".node.txt")] {
        let (mut equal, mut differ) = (0, Vec::new());
        for row in manifest.lines() {
            let path = row.split('\t').next().unwrap_or(row);
            let stem = path.strip_suffix(".java").with_context(|| format!("{path}: not a .java file"))?;
            let want = std::fs::read(goldens.join(format!("{stem}{golden_suffix}"))).with_context(|| format!("reading {stem}{golden_suffix}"))?;
            match std::fs::read(out.join(format!("{path}{dump_suffix}"))) {
                Err(_) => differ.push(format!("missing: {path}")),
                Ok(got) => match ide_oracle::compare::first_differing_line(&want, &got) {
                    None => equal += 1,
                    Some(line) => differ.push(format!("differs: {path} (line {line})")),
                },
            }
        }
        println!("{kind}: {equal} equal, {} differ", differ.len());
        for d in &differ {
            println!("  {d}");
        }
        pass &= differ.is_empty();
    }
    Ok(pass)
}

fn run(cli: Cli) -> Result<bool> {
    match &cli.cmd {
        Cmd::Jars => println!("{}", jars(&cli)?.display()),
        Cmd::Psi { args } => psi_oracle(&cli)?.run(&tools().join("JavaPsiOracle.java"), &os_args(args), Streams::default())?,
        Cmd::Lex { lines, args } => {
            jvm(&cli)?.run(&tools().join("JavaLexOracle.java"), &os_args(args), Streams { stdout: lines.as_deref(), stderr: None })?
        }
        Cmd::VendorGoldens { repo, rev, out } => {
            let repo = repo.clone().unwrap_or_else(|| ide_oracle::cache_home().join("psi-syntax/intellij-community"));
            vendor::vendor(&repo, rev, &goldens_dir(out.clone()))?
        }
        Cmd::CheckGoldens { out, goldens, run } => {
            let goldens = goldens_dir(goldens.clone());
            let out = out.clone().unwrap_or_else(|| ide_oracle::cache_home().join("psi-syntax/oracle/java-goldens"));
            if *run {
                let err = PathBuf::from(format!("{}.err", out.display()));
                psi_oracle(&cli)?.run_list(&tools().join("JavaPsiOracle.java"), &goldens.join("manifest.tsv"), &goldens, &out, &["--node"], &err)?;
            }
            return check_goldens(&out, &goldens);
        }
        Cmd::Unmarked { checkout } => unmarked::unmarked(checkout)?,
        Cmd::Mutate { out, goldens, per_file, seed } => mutate::mutate(&goldens_dir(goldens.clone()), out, *per_file, seed)?,
        Cmd::LexFuzz { goldens, out, count, seed } => ide_oracle::fuzz::scatter_corpus(goldens, "java", out, *count, seed, &LEX_PIECES)?,
    }
    Ok(true)
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(e) => {
            eprintln!("java-oracle: {e:#}");
            std::process::ExitCode::from(2)
        }
    }
}
