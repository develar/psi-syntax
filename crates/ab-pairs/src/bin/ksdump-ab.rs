//! `ksdump-ab OLD NEW [--metric instructions|time] [--list LIST] [--root ROOT] [-- BENCH ARGS]`: two `ksdump` binaries
//! on one corpus, `--bench` of each in interleaved pairs (the first arm alternating) until the stopping rule gives its
//! verdict, under the host's reservation. BENCH ARGS go to both (`--threads 8`, `--walk`, `--lang java`, `--lex`).
//!
//! A run's numbers: `parse_s`, the seconds of the parse alone its `files … seconds …` line prints (the median of the
//! rounds under `--repeat`; with `--threads N` the round's wall), `mb_s` beside it, the process's `wall_ms` and `cpu_ms`,
//! and where the host counts them its `instructions` and `cycles`, the whole process's: reading the list's files
//! counts too, the same in both arms. `instructions` (the default where counted) are judged by `Rule::COUNT` and
//! guarded by the parse time and the cycles per instruction (`pairs::Guard`); a ratio that moved between the arms
//! escalates the verdict to `parse_s`. `time` judges `parse_s` by `Rule::MEDIAN` and waits for a quiet host first.
//! Before the pairs each arm runs once untimed, so the first pair reads the corpus from the page cache too.
use ab_pairs::pairs::{self, Arm, Guard, Host, HostArgs, HostPolicy, Plan, Progress, Ratio, Sample, Trial, Unit};
use ab_pairs::reserve;
use ab_pairs::stats::{Rule, Verdict, median};
use ab_pairs::sys;
use anyhow::{Context, Result, bail};
use clap::Parser;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

#[derive(Parser, Debug)]
#[command(name = "ksdump-ab", about = "Two ksdump binaries' --bench in interleaved pairs, to a verdict")]
struct Args {
    /// The base binary.
    old: PathBuf,
    /// The new binary.
    new: PathBuf,
    /// What decides: `instructions` where the host counts them, else `time`.
    #[arg(long, value_enum)]
    metric: Option<Metric>,
    /// The list of files to parse; default `~/.cache/psi-syntax/corpus/community-3caa0bf.list`.
    #[arg(long)]
    list: Option<PathBuf>,
    /// The directory the list's paths are under; default the bench cache's `intellij-community` clone.
    #[arg(long)]
    root: Option<PathBuf>,
    /// A move smaller than this share of the base median counts as the same.
    #[arg(long, default_value_t = 0.01)]
    threshold: f64,
    /// Pairs before a verdict (six for a 95 % interval and the guard's look).
    #[arg(long, default_value_t = 6)]
    min_n: usize,
    /// Pairs after which an undecided run is unclear.
    #[arg(long, default_value_t = 20)]
    max_n: usize,
    #[command(flatten)]
    host: HostArgs,
    /// Arguments of `ksdump --bench`, after `--`.
    #[arg(last = true)]
    bench: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Metric {
    Instructions,
    Time,
}

const PARSE: &str = "parse_s";
const MBS: &str = "mb_s";
const INSTRUCTIONS: &str = "instructions";
const CYCLES: &str = "cycles";

/// What escalates a count's verdict: the parse time per instruction (kept when it cannot tell) and the cycles per
/// instruction.
const GUARD: [Ratio<'static>; 2] = [
    Ratio { name: "parse", of: PARSE, per: INSTRUCTIONS, blind: true },
    Ratio { name: "ipc", of: CYCLES, per: INSTRUCTIONS, blind: false },
];

fn main() -> ExitCode {
    sys::catch_interrupt();
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ksdump-ab: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(a: Args) -> Result<()> {
    let counted = sys::counts_instructions();
    let metric = a.metric.unwrap_or(if counted { Metric::Instructions } else { Metric::Time });
    if metric == Metric::Instructions && !counted {
        bail!("this host does not count a child's instructions: --metric time");
    }
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let list = a.list.clone().unwrap_or_else(|| home().join(".cache/psi-syntax/corpus/community-3caa0bf.list"));
    let root = a.root.clone().unwrap_or_else(|| reserve::dir().join("intellij-community"));
    let mut bench = Bench { list, root, bench: a.bench.clone() };
    let arms = [arm("A", &a.old), arm("B", &a.new)];
    let host = Host { policy: HostPolicy::PerStep, args: a.host.clone(), checked: false };
    let (_held, load) = host.hold("ksdump-ab", metric == Metric::Time)?;
    eprintln!("ksdump-ab: {} pairs at most, threshold {} %, load {load:.1}", a.max_n, a.threshold * 100.0);
    for (i, arm) in arms.iter().enumerate() {
        bench.run(i, arm, &()).with_context(|| format!("the untimed run of {}", arm.bin.display()))?;
    }
    let outcome = compare(&mut bench, &arms, metric, &a)?;
    println!("{}", outcome.summary());
    Ok(())
}

fn arm(name: &str, bin: &std::path::Path) -> Arm {
    Arm { name: name.into(), bin: bin.to_path_buf(), home: PathBuf::new() }
}

/// One `ksdump --bench` over the list per run.
struct Bench {
    list: PathBuf,
    root: PathBuf,
    bench: Vec<String>,
}

impl Trial for Bench {
    type Unit = ();

    fn run(&mut self, _: usize, arm: &Arm, _: &()) -> Result<Sample> {
        sys::check_interrupt()?;
        let mut cmd = Command::new(&arm.bin);
        cmd.arg("--bench").arg("--list").arg(&self.list).arg("--root").arg(&self.root).args(&self.bench);
        let run = sys::run_timed(&mut cmd)?;
        let stdout = String::from_utf8_lossy(&run.stdout);
        let line = bench_line(&stdout).with_context(|| format!("no `files … seconds …` line in:\n{stdout}"))?;
        let mut metrics = std::collections::BTreeMap::from([
            (PARSE.to_string(), line.seconds),
            (MBS.to_string(), line.mb / line.seconds.max(1e-9)),
            ("wall_ms".to_string(), run.wall_ms()),
            ("cpu_ms".to_string(), run.cpu_ms()),
        ]);
        metrics.extend(run.instructions.map(|i| (INSTRUCTIONS.to_string(), i as f64)));
        metrics.extend(run.cycles.map(|c| (CYCLES.to_string(), c as f64)));
        Ok(Sample { metrics, answer: None })
    }
}

/// The megabytes and parse seconds of a `--bench` output: its `files … MB … seconds …` line, or the median seconds of
/// its `round K  files …` lines.
#[derive(Debug, PartialEq)]
struct BenchLine {
    mb: f64,
    seconds: f64,
}

fn bench_line(stdout: &str) -> Option<BenchLine> {
    let value = |words: &[&str], key: &str| -> Option<f64> {
        words.iter().position(|w| *w == key).and_then(|i| words.get(i + 1)?.parse().ok())
    };
    let mut mb = None;
    let mut seconds = Vec::new();
    for line in stdout.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let files = match words.as_slice() {
            ["files", ..] => &words[..],
            ["round", _, "files", ..] => &words[2..],
            _ => continue,
        };
        mb = Some(value(files, "MB")?);
        seconds.push(value(files, "seconds")?);
    }
    Some(BenchLine { mb: mb?, seconds: (!seconds.is_empty()).then(|| median(&seconds))? })
}

/// The pairs' end: the deciding metrics' verdicts, the pairs run, what the guard did, and each arm's runs.
struct Outcome {
    judged: Vec<(String, Option<Verdict>)>,
    progress: Progress,
}

fn compare(t: &mut impl Trial<Unit = ()>, arms: &[Arm; 2], metric: Metric, a: &Args) -> Result<Outcome> {
    let floor = |_: &str| 0.0;
    let (deciding, count, guard) = match metric {
        Metric::Instructions => {
            (INSTRUCTIONS, Some((INSTRUCTIONS, Rule::COUNT)), Some(Guard { ratios: &GUARD, escalate: &[PARSE] }))
        }
        Metric::Time => (PARSE, None, None),
    };
    let plan = Plan {
        host: HostPolicy::PerStep,
        min_n: a.min_n,
        max_n: a.max_n.max(a.min_n),
        rule: Rule::MEDIAN,
        count,
        rel: a.threshold,
        floor: &floor,
        budget: None,
        guard,
        filter: None,
    };
    let units = [Unit { of: &(), arms: vec![0, 1], deciding: vec![deciding.to_string()] }];
    let mut judged = Vec::new();
    let mut progress = pairs::run(t, arms, &units, &plan, |pair, p| {
        let last = |arm: usize| p.runs[arm].last().map(|s| s.metrics.clone()).unwrap_or_default();
        let (a, b) = (last(0), last(1));
        println!("pair {}  A {}  B {}", pair.n, numbers(&a), numbers(&b));
        judged = pair.judged.to_vec();
        Ok(())
    })?;
    Ok(Outcome { judged, progress: progress.remove(0) })
}

/// A run's MB/s, and its instructions and cycles in millions where counted.
fn numbers(m: &std::collections::BTreeMap<String, f64>) -> String {
    let mut s = format!("{:.1} MB/s", m.get(MBS).copied().unwrap_or(0.0));
    for (key, unit) in [(INSTRUCTIONS, "M instr"), (CYCLES, "M cycles")] {
        if let Some(v) = m.get(key) {
            s.push_str(&format!(" {:.0} {unit}", v / 1e6));
        }
    }
    s
}

impl Outcome {
    /// The verdict line, then each arm's medians and the ratio B/A of every metric both arms have.
    fn summary(&self) -> String {
        let p = &self.progress;
        let verdicts: Vec<String> = self
            .judged
            .iter()
            .map(|(m, v)| format!("{m} {}", v.map_or("unclear".to_string(), |v| format!("{v:?}").to_lowercase())))
            .collect();
        let mut s = format!("verdict: {} after {} pairs", verdicts.join(", "), p.n(&[0, 1]));
        if let Some(why) = &p.escalated {
            s.push_str(&format!("  ESCALATED by {why}"));
        }
        if !p.guard_blind.is_empty() {
            s.push_str(&format!("  GUARD BLIND {}", p.guard_blind.join(", ")));
        }
        for metric in [MBS, PARSE, INSTRUCTIONS, CYCLES, "wall_ms", "cpu_ms"] {
            let (Some(a), Some(b)) = (pairs::values(&p.samples[0], metric), pairs::values(&p.samples[1], metric))
            else {
                continue;
            };
            if a.is_empty() || b.is_empty() {
                continue;
            }
            let (ma, mb) = (median(&a), median(&b));
            let value = |v: f64| if v.abs() >= 1000.0 { format!("{v:.0}") } else { format!("{v:.4}") };
            s.push_str(&format!("\nmedian {metric}  A {}  B {}  B/A {:.4}", value(ma), value(mb), mb / ma));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bench_line_is_the_parse_or_the_median_of_its_rounds() {
        let one = "files 3  MB 1.50  seconds 0.500  MB/s 3.0  nodes 10\n  heap bytes per source MB 7\n";
        assert_eq!(bench_line(one), Some(BenchLine { mb: 1.5, seconds: 0.5 }));
        let rounds = "round 1  files 3  MB 1.50  seconds 0.900  MB/s 1.7  nodes 10\nround 2  files 3  MB 1.50  seconds \
                      0.500  MB/s 3.0  nodes 10\nround 3  files 3  MB 1.50  seconds 0.600  MB/s 2.5  nodes 10\nmedian \
                      MB/s 2.5 of 3 rounds\n";
        assert_eq!(bench_line(rounds), Some(BenchLine { mb: 1.5, seconds: 0.6 }));
        let lex = "files 3  MB 1.50  seconds 0.100  MB/s 15.0  tokens 99\ndocs 2  MB 0.10  seconds 0.010  MB/s 10.0\n";
        assert_eq!(bench_line(lex), Some(BenchLine { mb: 1.5, seconds: 0.1 }));
        assert_eq!(bench_line("dumped 3 files\n"), None);
    }

    /// Two arms that print the same parse time are the same at `min_n`, judged by time; a slower parse is slower.
    #[test]
    fn equal_binaries_are_the_same_and_a_slower_parse_is_slower() {
        let dir = tempfile::tempdir().unwrap();
        let script = |name: &str, seconds: &str| {
            let path = dir.path().join(name);
            let line = format!("files 3  MB 1.50  seconds {seconds}  MB/s 1.0  nodes 10");
            std::fs::write(&path, format!("#!/bin/sh\necho '{line}'\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        };
        let (base, same, slow) = (script("base", "0.500"), script("same", "0.500"), script("slow", "0.600"));
        let args = |old: &PathBuf, new: &PathBuf| {
            Args::parse_from(["ksdump-ab", old.to_str().unwrap(), new.to_str().unwrap(), "--metric", "time"])
        };
        let mut bench = Bench { list: "list".into(), root: "root".into(), bench: Vec::new() };
        let outcome =
            compare(&mut bench, &[arm("A", &base), arm("B", &same)], Metric::Time, &args(&base, &same)).unwrap();
        assert_eq!(outcome.judged, [(PARSE.to_string(), Some(Verdict::Same))]);
        assert_eq!(outcome.progress.n(&[0, 1]), 6);
        assert!(outcome.summary().starts_with("verdict: parse_s same after 6 pairs"), "{}", outcome.summary());
        let outcome =
            compare(&mut bench, &[arm("A", &base), arm("B", &slow)], Metric::Time, &args(&base, &slow)).unwrap();
        assert_eq!(outcome.judged, [(PARSE.to_string(), Some(Verdict::Slower))]);
        assert!(outcome.summary().contains("median parse_s  A 0.5000  B 0.6000  B/A 1.2000"), "{}", outcome.summary());
    }
}
