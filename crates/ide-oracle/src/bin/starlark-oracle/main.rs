//! `starlark-oracle`: runs the Bazel plugin's Starlark lexer on pinned IntelliJ jars.
//!
//! `cargo run --release -p ide-oracle --bin starlark-oracle -- --plugin-jar LANGUAGES.jar lex -- --list LIST --root ROOT`.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use ide_oracle::jvm::{Jvm, Streams};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "starlark-oracle", about = "The Bazel plugin's pinned Starlark lexer oracle")]
struct Cli {
    /// IntelliJ's `Contents`, whose jars the Java oracle pins and whose JBR runs it (`$JAVA` overrides the JVM).
    #[arg(long, global = true, default_value = ide_oracle::jvm::DEFAULT_IDE)]
    ide: PathBuf,
    /// Where the pinned IntelliJ jars are cached.
    #[arg(long, global = true)]
    lib: Option<PathBuf>,
    /// The Bazel plugin's `//plugins/bazel/intellij.bazel.languages:languages` jar.
    #[arg(long, global = true)]
    plugin_jar: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Runs `StarlarkLexOracle.java`; its hash/count lines go to `--lines` or stdout.
    Lex {
        /// The hash/count lines' output file; a failed write fails the run.
        #[arg(long)]
        lines: Option<PathBuf>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Writes IntelliJ's `DebugUtil.psiToString` dumps for a Starlark file list.
    Psi {
        #[arg(long)]
        list: PathBuf,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        trim: bool,
    },
    /// Generates deterministic lexer mutations of the Starlark files under `goldens`.
    LexFuzz {
        goldens: PathBuf,
        out: PathBuf,
        #[arg(default_value_t = 5000)]
        count: usize,
        #[arg(default_value = "starlark-lex")]
        seed: String,
    },
}

const LEX_FUZZ_PIECES: &[&str] = &[
    "\\",
    "\\\n",
    "\\\r\n",
    "'",
    "\"",
    "'''",
    "\"\"\"",
    "\n",
    "\r\n",
    "\t",
    " ",
    "#",
    "# comment\n",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ":",
    ",",
    ".",
    "=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "&",
    "|",
    "^",
    "<",
    ">",
    "//",
    "**",
    "<<",
    ">>",
    "0",
    "1",
    "0x",
    "0o",
    ".5",
    "1e",
    "_",
    "a",
    "load",
    "name",
    "True",
    "None",
    "\u{a0}",
    "\u{2028}",
    "\u{3000}",
];

fn tools() -> PathBuf {
    ide_oracle::psi_syntax().join("tools/starlark-oracle")
}

fn jars(cli: &Cli, plugin_jar: &Path) -> Result<PathBuf> {
    let expected = ide_oracle::jars::pins(&tools().join("languages.sha256"))?;
    if expected.len() != 1 {
        bail!("{} must pin exactly one Bazel plugin jar", tools().join("languages.sha256").display());
    }
    let pin = &expected[0];
    let actual = ide_oracle::jars::digest_of(plugin_jar, &pin.digest)?;
    if actual != pin.digest {
        bail!(
            "{}: SHA-256 {actual}, expected {} ({})",
            plugin_jar.display(),
            pin.digest,
            tools().join("languages.sha256").display()
        );
    }

    let lib = cli.lib.clone().unwrap_or_else(|| ide_oracle::cache_home().join("psi-syntax/starlark-oracle/lib"));
    let platform_pins = ide_oracle::psi_syntax().join("tools/java-oracle/jars.sha256");
    ide_oracle::jars::fetch(&platform_pins, &cli.ide, &lib)
}

fn os_args(args: &[OsString]) -> Vec<&OsStr> {
    args.iter().map(OsString::as_os_str).collect()
}

fn run(cli: Cli) -> Result<()> {
    match &cli.cmd {
        Cmd::Lex { lines, args } => {
            let plugin_jar = cli.plugin_jar.as_deref().context("--plugin-jar is required")?;
            let lib = jars(&cli, plugin_jar)?;
            let jvm = Jvm::new(ide_oracle::jvm::java(&cli.ide), &lib).with_classpath(plugin_jar)?;
            jvm.run(
                &tools().join("StarlarkLexOracle.java"),
                &os_args(args),
                Streams { stdout: lines.as_deref(), stderr: None },
            )
            .with_context(|| format!("running Starlark lexer from {}", plugin_jar.display()))?;
        }
        Cmd::Psi { list, root, out, trim } => {
            let plugin_jar = cli.plugin_jar.as_deref().context("--plugin-jar is required")?;
            let lib = jars(&cli, plugin_jar)?;
            let jvm = Jvm::new(ide_oracle::jvm::java(&cli.ide), &lib).with_classpath(plugin_jar)?;
            let mut args = vec![
                "--list",
                list.to_str().context("list path is not UTF-8")?,
                "--root",
                root.to_str().context("root path is not UTF-8")?,
                "--out",
                out.to_str().context("output path is not UTF-8")?,
            ]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
            if *trim {
                args.push(OsString::from("--trim"));
            }
            jvm.run(&tools().join("StarlarkPsiOracle.java"), &os_args(&args), Streams { stdout: None, stderr: None })
                .with_context(|| format!("running Starlark PSI oracle from {}", plugin_jar.display()))?;
        }
        Cmd::LexFuzz { goldens, out, count, seed } => {
            ide_oracle::fuzz::scatter_corpus(goldens, "bzl", out, *count, seed, LEX_FUZZ_PIECES)?;
        }
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("starlark-oracle: {e:#}");
            std::process::ExitCode::from(2)
        }
    }
}
