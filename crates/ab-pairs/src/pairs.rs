//! The interleaved pairs every A/B of two binaries runs (`ab` on queries, `build-ab` and `lang-ab` on index builds,
//! `ab-run` through `ab`): a unit (a query, a build) runs once per arm per pair, the arm that goes first alternating, so
//! a drifting host falls on both; a unit leaves the rounds once every metric that decides it has its verdict
//! (`stats::decide`), it reaches `max_n`, or it spends its budget. What an arm is prepared with (homes, resets, cold
//! caches, seeds) and how a finished unit is reported stay with the command. Every reservation and wait for the host
//! goes through `Host`: when is the command's `HostPolicy`, how long and how hard its `HostArgs`.
//!
//! A unit judged by a count (`Plan::count`: instructions, which no neighbour's load moves) is watched by a `Guard`:
//! when the count decides, a time or the cycles per count that moved between the arms beyond the threshold (an I/O
//! wait, a lock, a parallelism change the count does not see) escalates it to the time's rule, its pairs going on;
//! a ratio too noisy to say either way leaves the unit flagged `guard_blind`, its count's verdict standing.
//! A `Filter` drops the pairs whose runs a neighbour's burst reached from a verdict a time gives; every run stays in
//! `Progress::runs`, so what a run answered never depends on its pair being kept.
use crate::reserve::{self, Reservation, wait_quiet, wait_unshared};
use crate::stats::{Rule, Verdict, decide, median};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Arm {
    pub name: String,
    pub bin: PathBuf,
    pub home: PathBuf,
}

/// One timed run of a unit: its numbers by metric name (`wall_ms`, `cpu_ms`; `wall_s`, `parse.kotlin_s`, …) and, for
/// a query, its answer with the timing removed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub metrics: BTreeMap<String, f64>,
    pub answer: Option<String>,
}

/// What a run of pairs asks of the machine; the tests replace it.
pub trait Trial {
    type Unit;
    /// Before each pair of a unit under `HostPolicy::PerPair`: its wait (`Host::before_pair`); the load then.
    fn before_pair(&mut self) -> Result<f64> {
        Ok(reserve::probe().load)
    }
    /// Before the next pair after one the `Filter` dropped, under `HostPolicy::PerStep`: its wait
    /// (`Host::after_burst`); the load then.
    fn after_burst(&mut self) -> Result<f64> {
        Ok(reserve::probe().load)
    }
    /// One timed run of `unit` by arm `index` of the run's arms.
    fn run(&mut self, index: usize, arm: &Arm, unit: &Self::Unit) -> Result<Sample>;
}

/// A unit of a run: what it is, the arms that run it (indexes into the run's arms, the first two compared) and the
/// metrics whose verdicts it waits for.
pub struct Unit<'a, U> {
    pub of: &'a U,
    pub arms: Vec<usize>,
    pub deciding: Vec<String>,
}

/// How the pairs stop: `rule` (the `count` metric's own rule for it) against `±max(rel × base median,
/// floor(metric))`, looked at after every pair from `min_n`, at `max_n` pairs run (dropped ones too) at the latest;
/// `budget` (a metric and its sum over a unit's timed runs, both arms) stops an undecided unit at its next pair from
/// `min_n`, flagged; `host` says whether each pair waits first (the command's policy, the one its `Host` follows);
/// `guard` watches the units the count decides, `filter` drops pairs from the verdicts of times.
pub struct Plan<'a> {
    pub host: HostPolicy,
    pub min_n: usize,
    pub max_n: usize,
    pub rule: Rule,
    pub count: Option<(&'a str, Rule)>,
    pub rel: f64,
    pub floor: &'a dyn Fn(&str) -> f64,
    pub budget: Option<(&'a str, f64)>,
    pub guard: Option<Guard<'a>>,
    pub filter: Option<Filter<'a>>,
}

impl Plan<'_> {
    /// The rule of `metric`: the count's for the count, `rule` for every other.
    pub fn rule_of(&self, metric: &str) -> Rule {
        match self.count {
            Some((count, rule)) if count == metric => rule,
            _ => self.rule,
        }
    }

    fn counted(&self, deciding: &[String]) -> bool {
        self.count.is_some_and(|(count, _)| deciding.iter().any(|m| m == count))
    }
}

/// A per-run ratio the guard compares between the arms: `of / per` (wall time per instruction, cycles per
/// instruction), named by what it watches (`wall`, `cpu`, `ipc`); `blind` says whether its failing to tell is kept
/// (`Progress::guard_blind`): a time's is, a proxy's that is too noisy to tell on most units (the cycles) is not.
#[derive(Debug, Clone, Copy)]
pub struct Ratio<'a> {
    pub name: &'a str,
    pub of: &'a str,
    pub per: &'a str,
    pub blind: bool,
}

/// The pairs below which the guard does not look: its interval is the sign test's 95 % one of the median (the
/// extremes at six), and on recorded A/A pairs (docs/measurements/speed.md, "the guard of a count's verdict and the
/// filter of crowded pairs") the range of three escalated 14 % of the looks on a loaded host where six escalated none.
/// A unit its count decides earlier runs on to six.
pub const GUARD_MIN_N: usize = 6;

/// What watches a verdict of the count: once the count decides a unit (from `GUARD_MIN_N` pairs), each ratio's paired
/// differences of logs, new − base, judged by `Rule::MEDIAN` without its tail hold against `ln(1 + max(rel,
/// floor(of) / base median of of))`; the first that moved (faster or slower) escalates the unit: `escalate` decides
/// it from then on, by `Plan::rule`, its pairs so far counted, and the unit is flagged with the ratio's name. When
/// none moved, the ratios neither the same nor moved (a wall too noisy to tell at six pairs) are kept as
/// `Progress::guard_blind`: the count's verdict stands, its confirmation missing. A ratio a run lacks is not looked
/// at. One look per unit, at the count's verdict: the guard spends no level on later looks.
pub struct Guard<'a> {
    pub ratios: &'a [Ratio<'a>],
    pub escalate: &'a [&'a str],
}

/// What the guard's look saw.
#[derive(Debug, Clone, PartialEq)]
enum Look {
    /// The first ratio (in `Guard::ratios`' order) whose arms moved apart beyond the threshold.
    Moved(String),
    /// No ratio moved, and these were neither the same nor moved: too noisy to confirm the count.
    Blind(Vec<String>),
    /// Every ratio looked at is the same between the arms.
    Clear,
}

impl Guard<'_> {
    fn look(&self, plan: &Plan, base: &[Sample], new: &[Sample]) -> Look {
        let logs = |s: &[Sample], r: &Ratio| -> Option<Vec<f64>> {
            s.iter()
                .map(|x| {
                    let (of, per) = (*x.metrics.get(r.of)?, *x.metrics.get(r.per)?);
                    (of > 0.0 && per > 0.0).then(|| (of / per).ln())
                })
                .collect()
        };
        let mut blind = Vec::new();
        for r in self.ratios {
            let (Some(b), Some(n), Some(of)) = (logs(base, r), logs(new, r), values(base, r.of)) else { continue };
            let rel = plan.rel.max((plan.floor)(r.of) / median(&of));
            match decide(&Rule::MEDIAN.without_tail_hold(), &b, &n, 0.0, rel.ln_1p(), 1, usize::MAX).map(|d| d.verdict)
            {
                Some(Verdict::Slower | Verdict::Faster) => return Look::Moved(r.name.to_string()),
                Some(Verdict::Same) => {}
                _ if r.blind => blind.push(r.name.to_string()),
                _ => {}
            }
        }
        if blind.is_empty() { Look::Clear } else { Look::Blind(blind) }
    }
}

/// Which pairs a neighbour's burst reached: given every run of a unit, per arm of the unit (`Unit::arms`' order, run
/// `i` of each the unit's pair `i`), `dirty` marks each run; a pair one of whose runs is marked is dropped from a unit
/// whose deciding metrics include one of `times` (a count's verdict keeps it: no load moves a count), counted in
/// `Progress::dropped`. The marks are taken again after every pair, so `dirty` may judge a run against the unit's
/// others (a burst against the unit's own steady load); a unit the guard escalates to a time drops its pairs then.
pub struct Filter<'a> {
    pub times: &'a [&'a str],
    pub dirty: &'a dyn Fn(&[&[Sample]]) -> Vec<Vec<bool>>,
}

impl Filter<'_> {
    fn applies(&self, deciding: &[String]) -> bool {
        deciding.iter().any(|m| self.times.contains(&m.as_str()))
    }
}

/// A unit's timed runs so far, per arm of the run (an arm that does not run it has none).
#[derive(Debug, Clone)]
pub struct Progress {
    /// Every timed run, the dropped pairs' too, in order, with its answer.
    pub runs: Vec<Vec<Sample>>,
    /// The numbers of the runs its verdicts rest on: `runs` without the dropped pairs, no answers.
    pub samples: Vec<Vec<Sample>>,
    /// The budget's metric summed over the unit's timed runs.
    pub spent: f64,
    /// It stopped at its budget, not by its verdicts or at `max_n`.
    pub budget_hit: bool,
    pub done: bool,
    /// The load before each of its pairs under `HostPolicy::PerPair` (`Trial::before_pair`).
    pub loads: Vec<f64>,
    /// The guard's ratio that moved (`Guard`): the unit was then decided by `Guard::escalate`.
    pub escalated: Option<String>,
    /// The guard's ratios that could not tell at its look (neither the same nor moved): the count's verdict stands
    /// unconfirmed by them. Non-gating.
    pub guard_blind: Vec<String>,
    /// The pairs the filter dropped (`Filter`): their positions in `runs`, their runs absent from `samples`.
    pub dropped: usize,
    pub dropped_at: Vec<usize>,
}

impl Progress {
    /// The pairs it ran: the fewest runs of one of `arms`.
    pub fn n(&self, arms: &[usize]) -> usize {
        arms.iter().map(|&a| self.samples[a].len()).min().unwrap_or(0)
    }

    /// The pairs it ran, the dropped ones too.
    pub fn ran(&self, arms: &[usize]) -> usize {
        arms.iter().map(|&a| self.runs[a].len()).min().unwrap_or(0)
    }

    /// `samples` from `runs`: the pairs `filter` marks dropped (when given), the rest kept without their answers.
    fn keep(&mut self, arms: &[usize], filter: Option<&Filter>) {
        let n = self.ran(arms);
        self.dropped_at = match filter {
            Some(f) => {
                let marks = (f.dirty)(&arms.iter().map(|&a| &self.runs[a][..n]).collect::<Vec<_>>());
                (0..n).filter(|&i| marks.iter().any(|m| m.get(i).copied().unwrap_or(false))).collect()
            }
            None => Vec::new(),
        };
        self.dropped = self.dropped_at.len();
        for &a in arms {
            let runs = &self.runs[a];
            self.samples[a] = runs
                .iter()
                .enumerate()
                .filter(|(i, _)| self.dropped_at.binary_search(i).is_err())
                .map(|(_, s)| Sample { metrics: s.metrics.clone(), answer: None })
                .collect();
        }
    }
}

/// One pair of a unit, as `run` reports it: the unit's position, its pairs so far, each deciding metric's verdict
/// (`None` while it needs more, or for a metric a run lacks) and whether the unit is done.
pub struct Pair<'a> {
    pub unit: usize,
    pub n: usize,
    pub judged: &'a [(String, Option<Verdict>)],
    pub done: bool,
}

/// Every value of `metric` in `samples`, or `None` when a run lacks it.
pub fn values(samples: &[Sample], metric: &str) -> Option<Vec<f64>> {
    samples.iter().map(|s| s.metrics.get(metric).copied()).collect()
}

/// Each deciding metric's verdict of a unit's pairs so far (`None` while it needs more, or for a metric a run lacks).
fn judge(plan: &Plan, p: &Progress, arms: &[usize], deciding: &[String]) -> Vec<(String, Option<Verdict>)> {
    match arms {
        [b, nw, ..] => deciding
            .iter()
            .map(|m| {
                let v = values(&p.samples[*b], m).zip(values(&p.samples[*nw], m)).and_then(|(bv, nv)| {
                    decide(&plan.rule_of(m), &bv, &nv, plan.rel, (plan.floor)(m), plan.min_n, plan.max_n)
                });
                (m.clone(), v.map(|d| d.verdict))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Rounds of one pair per undecided unit, in the units' order, until each is done; `report` after every pair.
pub fn run<T: Trial>(
    t: &mut T,
    arms: &[Arm],
    units: &[Unit<T::Unit>],
    plan: &Plan,
    mut report: impl FnMut(&Pair, &Progress) -> Result<()>,
) -> Result<Vec<Progress>> {
    let mut progress: Vec<Progress> = units
        .iter()
        .map(|_| Progress {
            runs: vec![Vec::new(); arms.len()],
            samples: vec![Vec::new(); arms.len()],
            spent: 0.0,
            budget_hit: false,
            done: false,
            loads: Vec::new(),
            escalated: None,
            guard_blind: Vec::new(),
            dropped: 0,
            dropped_at: Vec::new(),
        })
        .collect();
    // The pair just run was dropped: the host is not what the step waited for, whichever unit runs next.
    let mut burst = false;
    for round in 0.. {
        let pending: Vec<usize> = (0..units.len()).filter(|&i| !progress[i].done).collect();
        if pending.is_empty() {
            break;
        }
        for ui in pending {
            let u = &units[ui];
            if plan.host.waits_per_pair() {
                let load = t.before_pair()?;
                progress[ui].loads.push(load);
            } else if std::mem::take(&mut burst) && plan.host == HostPolicy::PerStep {
                t.after_burst()?;
            }
            let mut order = u.arms.clone();
            if round % 2 == 1 {
                order.reverse();
            }
            for &a in &order {
                let s = t.run(a, &arms[a], u.of)?;
                let p = &mut progress[ui];
                if let Some((metric, _)) = plan.budget {
                    p.spent += s.metrics.get(metric).copied().unwrap_or(0.0);
                }
                p.runs[a].push(s);
            }
            let p = &mut progress[ui];
            let paired = u.arms.len() >= 2;
            let guard = plan.guard.as_ref().filter(|_| paired && plan.counted(&u.deciding));
            let deciding = |p: &Progress| -> Vec<String> {
                match (&p.escalated, guard) {
                    (Some(_), Some(g)) => g.escalate.iter().map(|m| m.to_string()).collect(),
                    _ => u.deciding.clone(),
                }
            };
            let filter = |p: &mut Progress| {
                let f = plan.filter.as_ref().filter(|f| paired && f.applies(&deciding(p)));
                p.keep(&u.arms, f);
            };
            filter(p);
            burst = p.dropped_at.last().is_some_and(|&i| i + 1 == p.ran(&u.arms));
            let mut judged = judge(plan, p, &u.arms, &deciding(p));
            let decided = |p: &Progress, judged: &[(String, Option<Verdict>)]| {
                p.n(&u.arms) >= plan.min_n && judged.iter().all(|(_, v)| v.is_some())
            };
            let mut is_decided = decided(p, &judged);
            if let Some(g) = guard.filter(|_| is_decided && p.escalated.is_none()) {
                if p.n(&u.arms) < GUARD_MIN_N {
                    // The count decided before the guard can look: on to its pairs.
                    is_decided = false;
                } else {
                    match g.look(plan, &p.samples[u.arms[0]], &p.samples[u.arms[1]]) {
                        Look::Moved(why) => {
                            p.escalated = Some(why);
                            filter(p);
                            judged = judge(plan, p, &u.arms, &deciding(p));
                            is_decided = decided(p, &judged);
                        }
                        Look::Blind(names) => p.guard_blind = names,
                        Look::Clear => {}
                    }
                }
            }
            let n = p.n(&u.arms);
            let ran = p.ran(&u.arms);
            p.done = if paired {
                let spent = plan.budget.is_some_and(|(_, limit)| p.spent >= limit);
                p.budget_hit = !is_decided && spent && n >= plan.min_n && ran < plan.max_n;
                is_decided || p.budget_hit || ran >= plan.max_n
            } else {
                n >= plan.min_n
            };
            report(&Pair { unit: ui, n, judged: &judged, done: p.done }, p)?;
        }
    }
    Ok(progress)
}

/// The flags a stopping rule of query pairs takes (`ab`, and `ab-run` for its `ab` steps).
#[derive(clap::Args, Debug, Clone, PartialEq)]
pub struct PairArgs {
    /// A move smaller than this share of the base median counts as the same.
    #[arg(long, default_value_t = 0.10)]
    pub threshold: f64,
    /// … or smaller than this many milliseconds.
    #[arg(long, default_value_t = 5.0)]
    pub floor_ms: f64,
    /// Pairs before a query may stop (six is the least for a 95 % interval).
    #[arg(long, default_value_t = 6)]
    pub min_n: usize,
    /// Pairs after which an undecided query is reported unclear.
    #[arg(long, default_value_t = 30)]
    pub max_n: usize,
    /// Seconds of timed runs (both arms) after which an undecided query stops at its next pair from `--min-n`, its
    /// verdict `unclear` (`budget` in its line); 0: none, `--max-n` alone ends it.
    #[arg(long, default_value_t = 120.0)]
    pub budget_s: f64,
    /// A pair one of whose runs saw other processes take more than this share of the host's cores beyond the
    /// query's steady level (the median over its runs) and the accounting's tick per core leaves a verdict of the
    /// wall, counted as dropped (a verdict of instructions keeps it); 0: every pair stays. The 10 ms tick per core
    /// leaves short runs unfiltered: at 0.5, none under 20 ms on a quiet host, none at all beside half the cores busy.
    #[arg(long, default_value_t = 0.5)]
    pub foreign_cpu: f64,
}

impl PairArgs {
    /// The flags that give a child (`ab-run`'s `ab` steps) these values.
    pub fn to_args(&self) -> Vec<String> {
        [
            ("--threshold", self.threshold.to_string()),
            ("--floor-ms", self.floor_ms.to_string()),
            ("--min-n", self.min_n.to_string()),
            ("--max-n", self.max_n.to_string()),
            ("--budget-s", self.budget_s.to_string()),
            ("--foreign-cpu", self.foreign_cpu.to_string()),
        ]
        .into_iter()
        .flat_map(|(k, v)| [k.to_string(), v])
        .collect()
    }
}

/// When a command waits for the host beyond its reservation; `Host` carries it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPolicy {
    /// One wait for a quiet host before a step's timed runs (`Host::before_step`; `ab`, each of `ab-run`'s steps),
    /// none between the pairs but after one the `Filter` dropped: the next pair waits until no other process is busy
    /// (`Host::after_burst`), so a load that comes during the step costs its pairs, not the quiet host the step
    /// waited for.
    PerStep,
    /// That wait, then before every pair one until no other process is busy (`Host::before_pair`; `build-ab`,
    /// `lang-ab`): a full build's thread-time sums follow another session's load where its CPU does not.
    PerPair,
    /// No reservation and no wait at all (`--quick` of `ab` and `ab-run`): triage judged by instructions on a host
    /// as it is, never a published number; the load is still recorded.
    None,
}

impl HostPolicy {
    /// Whether `run` calls `Trial::before_pair` before each pair.
    fn waits_per_pair(self) -> bool {
        self == HostPolicy::PerPair
    }
}

/// How long and how hard every A/B run of two binaries waits for the host.
#[derive(clap::Args, Debug, Clone, PartialEq)]
pub struct HostArgs {
    /// Seconds to wait for the host's reservation.
    #[arg(long, default_value_t = 7200)]
    pub wait_reservation: u64,
    /// Seconds to wait for a quiet host.
    #[arg(long, default_value_t = 1800)]
    pub wait_quiet: u64,
    /// Measure on a loaded host.
    #[arg(long)]
    pub force: bool,
}

/// Every reservation and wait of a command: its policy, its flags, and whether its caller holds the reservation and
/// waited for a quiet host already (`checked`, `ab-run`'s children).
#[derive(Debug, Clone)]
pub struct Host {
    pub policy: HostPolicy,
    pub args: HostArgs,
    pub checked: bool,
}

impl Host {
    /// The host's reservation, held while the guard lives (a child's joins its caller's, `reserve::HELD`); under
    /// `HostPolicy::None` none, taken at once.
    pub fn reserve(&self, what: &str) -> Result<Reservation> {
        if self.policy == HostPolicy::None {
            return Ok(Reservation::none());
        }
        Reservation::acquire(what, Duration::from_secs(self.args.wait_reservation))
    }

    /// Before a step's timed runs: a quiet host when the step is judged by its wall (`wall`; one judged by CPU time or
    /// instructions starts on a loaded host), the caller did not wait already and the policy waits at all; the load
    /// then.
    pub fn before_step(&self, wall: bool) -> Result<f64> {
        if wall && !self.checked && self.policy != HostPolicy::None {
            wait_quiet(Duration::from_secs(self.args.wait_quiet), self.args.force)
        } else {
            Ok(reserve::probe().load)
        }
    }

    /// The reservation, then `before_step`.
    pub fn hold(&self, what: &str, wall: bool) -> Result<(Reservation, f64)> {
        let held = self.reserve(what)?;
        Ok((held, self.before_step(wall)?))
    }

    /// Before a pair: under `PerPair` until no other process is busy, whatever the load average (and whatever the
    /// caller waited for); the load then.
    pub fn before_pair(&self) -> Result<f64> {
        match self.policy {
            HostPolicy::PerPair => wait_unshared(Duration::from_secs(self.args.wait_quiet), self.args.force),
            HostPolicy::PerStep | HostPolicy::None => Ok(reserve::probe().load),
        }
    }

    /// Before the pair after a dropped one: under `PerStep` until no other process is busy, whatever the caller waited
    /// for before the step (`checked`) and whatever the load average, which the step's own runs still hold; the load
    /// then.
    pub fn after_burst(&self) -> Result<f64> {
        match self.policy {
            HostPolicy::PerStep => wait_unshared(Duration::from_secs(self.args.wait_quiet), self.args.force),
            HostPolicy::PerPair | HostPolicy::None => Ok(reserve::probe().load),
        }
    }
}

/// The values a flattened argument group takes when no flag is given: its defaults, from its own declaration.
pub fn defaults<T: clap::Args + clap::FromArgMatches>() -> T {
    let cmd = T::augment_args(clap::Command::new("defaults"));
    T::from_arg_matches(&cmd.get_matches_from(["defaults"])).expect("every flag of the group has a default")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Units answer by a function of (arm, unit, call count) under the metric `ms`; every call is logged.
    struct Fake {
        log: Vec<String>,
        calls: usize,
        time: Box<dyn Fn(&str, &str, usize) -> Option<f64>>,
    }

    impl Trial for Fake {
        type Unit = String;
        fn before_pair(&mut self) -> Result<f64> {
            self.log.push("pair".into());
            Ok(self.log.len() as f64)
        }
        fn after_burst(&mut self) -> Result<f64> {
            self.log.push("burst".into());
            Ok(self.log.len() as f64)
        }
        fn run(&mut self, index: usize, arm: &Arm, unit: &String) -> Result<Sample> {
            self.calls += 1;
            self.log.push(format!("{unit} {index}{}", arm.name));
            let metrics = (self.time)(&arm.name, unit, self.calls).map(|v| ("ms".to_string(), v)).into_iter().collect();
            Ok(Sample { metrics, answer: None })
        }
    }

    fn fake(time: impl Fn(&str, &str, usize) -> Option<f64> + 'static) -> Fake {
        Fake { log: Vec::new(), calls: 0, time: Box::new(time) }
    }

    fn arms() -> [Arm; 2] {
        [
            Arm { name: "b".into(), bin: "b".into(), home: "h/b".into() },
            Arm { name: "n".into(), bin: "n".into(), home: "h/n".into() },
        ]
    }

    fn jitter(i: usize) -> f64 {
        ((i * 7919) % 13) as f64 / 13.0 - 0.5
    }

    fn floor(_: &str) -> f64 {
        5.0
    }

    fn plan(budget: Option<(&str, f64)>) -> Plan<'_> {
        Plan {
            host: HostPolicy::PerPair,
            min_n: 6,
            max_n: 12,
            rule: Rule::MEDIAN,
            count: None,
            rel: 0.1,
            floor: &floor,
            budget,
            guard: None,
            filter: None,
        }
    }

    fn unit(of: &String, arms: Vec<usize>) -> Unit<'_, String> {
        Unit { of, arms, deciding: vec!["ms".into()] }
    }

    /// Each pair waits first, the first arm alternates by round, a decided unit leaves the rounds and `report` hears
    /// every pair with the verdicts so far.
    #[test]
    fn pairs_alternate_and_a_decided_unit_leaves_the_rounds() {
        let mut f = fake(|arm, u, i| {
            Some(
                if u == "moved" && arm == "n" { 50.0 } else { 100.0 }
                    + if u == "noisy" { 40.0 * jitter(i * 3 + 1) } else { jitter(i) },
            )
        });
        let (moved, noisy) = ("moved".to_string(), "noisy".to_string());
        let units = [unit(&moved, vec![0, 1]), unit(&noisy, vec![0, 1])];
        let mut heard = Vec::new();
        let p = run(&mut f, &arms(), &units, &plan(None), |pair, _| {
            heard.push((pair.unit, pair.n, pair.judged[0].1, pair.done));
            Ok(())
        })
        .unwrap();
        assert_eq!(&f.log[..6], &["pair", "moved 0b", "moved 1n", "pair", "noisy 0b", "noisy 1n"]);
        assert_eq!(&f.log[6..12], &["pair", "moved 1n", "moved 0b", "pair", "noisy 1n", "noisy 0b"]);
        assert_eq!((p[0].n(&[0, 1]), p[1].n(&[0, 1])), (6, 12));
        assert_eq!(heard.iter().rfind(|h| h.0 == 0), Some(&(0, 6, Some(Verdict::Faster), true)));
        assert_eq!(heard.iter().rfind(|h| h.0 == 1), Some(&(1, 12, Some(Verdict::Unclear), true)));
        assert!(heard.iter().filter(|h| h.1 < 6).all(|h| h.2.is_none() && !h.3), "no verdict before min_n");
        assert_eq!(f.log.len(), 6 * 3 + 12 * 3);
        // The load before each pair is the unit's: `moved`'s first two pairs waited at log lengths 1 and 7.
        assert_eq!((p[0].loads.len(), p[1].loads.len(), &p[0].loads[..2]), (6, 12, &[1.0, 7.0][..]));
        // Under `PerStep` no pair waits.
        let mut f = fake(|_, _, i| Some(100.0 + jitter(i)));
        let p = run(
            &mut f,
            &arms(),
            &[unit(&moved, vec![0, 1])],
            &Plan { host: HostPolicy::PerStep, ..plan(None) },
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!((f.log.len(), f.log.iter().any(|l| l == "pair"), p[0].loads.len()), (12, false, 0));
    }

    /// Under `PerStep` the pair after one the filter dropped waits for the host (`Trial::after_burst`), whichever unit
    /// it is; a pair after a clean one does not, and under `PerPair` the wait before every pair is the only one.
    #[test]
    fn the_pair_after_a_dropped_one_waits_under_per_step() {
        let dirty = |runs: &[&[Sample]]| -> Vec<Vec<bool>> {
            runs.iter().map(|r| r.iter().map(|s| s.metrics["ms"] > 500.0).collect()).collect()
        };
        let filter = || Some(Filter { times: &["ms"], dirty: &dirty });
        // Calls 9 and 10, the third pair of `a` (two units, four runs a round), ran beside a burst.
        let time = |_: &str, _: &str, i: usize| Some(if i == 9 || i == 10 { 1000.0 } else { 100.0 + jitter(i) });
        let (a, b) = ("a".to_string(), "b".to_string());
        let units = [unit(&a, vec![0, 1]), unit(&b, vec![0, 1])];
        let mut f = fake(time);
        let p = run(
            &mut f,
            &arms(),
            &units,
            &Plan { host: HostPolicy::PerStep, filter: filter(), ..plan(None) },
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!((&p[0].dropped_at[..], p[1].dropped), (&[2][..], 0));
        let waits: Vec<usize> = f.log.iter().enumerate().filter(|(_, l)| *l == "burst").map(|(i, _)| i).collect();
        assert_eq!(waits, [10], "once, before `b`'s third pair: {:?}", f.log);
        let mut f = fake(time);
        run(&mut f, &arms(), &units, &Plan { filter: filter(), ..plan(None) }, |_, _| Ok(())).unwrap();
        assert!(!f.log.iter().any(|l| l == "burst"), "{:?}", f.log);
    }

    /// A unit of one arm runs `min_n` times; a metric one run lacks never decides, so its unit runs to `max_n`.
    #[test]
    fn one_arm_runs_min_n_and_a_missing_metric_runs_to_max_n() {
        // The second pair of `gap` (calls 5 and 6, new first) has no value of the new arm.
        let mut f = fake(|arm, u, i| if u == "gap" && arm == "n" && i == 5 { None } else { Some(100.0 + jitter(i)) });
        let (single, gap) = ("single".to_string(), "gap".to_string());
        let p = run(&mut f, &arms(), &[unit(&single, vec![1]), unit(&gap, vec![0, 1])], &plan(None), |_, _| Ok(()))
            .unwrap();
        assert_eq!((p[0].samples[0].len(), p[0].samples[1].len()), (0, 6));
        assert_eq!((p[1].n(&[0, 1]), p[1].budget_hit), (12, false));
    }

    /// An undecided unit stops at its budget from `min_n`, flagged; one decided by then is not flagged.
    #[test]
    fn a_budget_stops_an_undecided_unit_from_min_n() {
        let mut f = fake(|arm, u, i| {
            Some(if u == "same" {
                1000.0 + jitter(i)
            } else {
                1000.0 + if arm == "n" { 100.0 } else { 0.0 } + 300.0 * jitter(i * 3 + 1)
            })
        });
        let (same, noisy) = ("same".to_string(), "noisy".to_string());
        let p = run(
            &mut f,
            &arms(),
            &[unit(&same, vec![0, 1]), unit(&noisy, vec![0, 1])],
            &plan(Some(("ms", 15_000.0))),
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!((p[0].n(&[0, 1]), p[0].budget_hit), (6, false));
        // Eight pairs of about 2.1 s pass 15 s.
        assert_eq!((p[1].n(&[0, 1]), p[1].budget_hit), (8, true));
        assert!(p[1].spent >= 15_000.0);
    }

    /// Units whose runs report several metrics, a function of (arm, unit, call count).
    struct Metrics(usize, Box<dyn Fn(&str, &str, usize) -> Vec<(&'static str, f64)>>);

    impl Trial for Metrics {
        type Unit = String;
        fn run(&mut self, _: usize, arm: &Arm, unit: &String) -> Result<Sample> {
            self.0 += 1;
            Ok(Sample {
                metrics: (self.1)(&arm.name, unit, self.0).into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                answer: None,
            })
        }
    }

    const RATIOS: [Ratio; 1] = [Ratio { name: "time", of: "ms", per: "count", blind: true }];

    fn counted(min_n: usize) -> Plan<'static> {
        Plan {
            host: HostPolicy::PerStep,
            min_n,
            max_n: 12,
            rule: Rule::MEDIAN,
            count: Some(("count", Rule::COUNT)),
            rel: 0.1,
            floor: &floor,
            budget: None,
            guard: Some(Guard { ratios: &RATIOS, escalate: &["ms"] }),
            filter: None,
        }
    }

    /// A count decides from three pairs, but the guard looks at six: a count that agrees and a time that follows it
    /// stop there unescalated, moved or not; a time that moved apart from an unmoved count escalates the unit to the
    /// time's rule, which decides it on the pairs so far. Without a guard the count stops at three.
    #[test]
    fn the_guard_escalates_a_count_whose_time_moved_apart() {
        let units = ["same", "both", "apart"].map(String::from);
        let mut t = Metrics(
            0,
            Box::new(|arm, u, i| {
                let new = arm == "n";
                let count = 1e9 * (1.0 + 0.0001 * jitter(i)) * if u == "both" && new { 1.2 } else { 1.0 };
                let ms = 100.0
                    * (1.0 + 0.01 * jitter(i * 3 + 1))
                    * if new && u != "same" { if u == "both" { 1.2 } else { 1.5 } } else { 1.0 };
                vec![("count", count), ("ms", ms)]
            }),
        );
        let us: Vec<Unit<String>> =
            units.iter().map(|u| Unit { of: u, arms: vec![0, 1], deciding: vec!["count".into()] }).collect();
        let mut last = vec![None; 3];
        let p = run(&mut t, &arms(), &us, &counted(3), |pair, _| {
            last[pair.unit] = Some(pair.judged.to_vec());
            Ok(())
        })
        .unwrap();
        let got: Vec<(usize, Option<&str>)> = p.iter().map(|p| (p.n(&[0, 1]), p.escalated.as_deref())).collect();
        assert_eq!(got, [(6, None), (6, None), (6, Some("time"))]);
        assert_eq!(last[0].as_deref(), Some(&[("count".to_string(), Some(Verdict::Same))][..]));
        assert_eq!(last[1].as_deref(), Some(&[("count".to_string(), Some(Verdict::Slower))][..]));
        assert_eq!(
            last[2].as_deref(),
            Some(&[("ms".to_string(), Some(Verdict::Slower))][..]),
            "judged by the time it escalated to"
        );
        assert!(p.iter().all(|p| p.guard_blind.is_empty()), "a quiet time confirms the count");
        let mut t = Metrics(0, Box::new(|_, _, i| vec![("count", 1e9 * (1.0 + 0.0001 * jitter(i))), ("ms", 100.0)]));
        let p = run(&mut t, &arms(), &us[..1], &Plan { guard: None, ..counted(3) }, |_, _| Ok(())).unwrap();
        assert_eq!(p[0].n(&[0, 1]), 3);
    }

    /// A time too noisy to tell at the guard's look (its sign test neither the same nor moved) leaves the count's
    /// verdict standing, unescalated, the unit flagged blind by that ratio; a time that is clearly the same does not.
    #[test]
    fn a_guard_that_cannot_tell_flags_the_unit_blind() {
        let noisy = "noisy".to_string();
        // The time swings by half either way, both arms alike; the count agrees.
        let mut t = Metrics(0, Box::new(|_, _, i| vec![("count", 1e9), ("ms", 100.0 * (1.0 + jitter(i * 3 + 1)))]));
        let mut last = None;
        let p = run(
            &mut t,
            &arms(),
            &[Unit { of: &noisy, arms: vec![0, 1], deciding: vec!["count".into()] }],
            &counted(3),
            |pair, _| {
                last = Some(pair.judged.to_vec());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            (p[0].n(&[0, 1]), p[0].escalated.as_deref(), &p[0].guard_blind[..]),
            (6, None, &["time".to_string()][..])
        );
        assert_eq!(last.as_deref(), Some(&[("count".to_string(), Some(Verdict::Same))][..]));
    }

    /// A pair one of whose runs is dirty leaves a time's verdict, counted as dropped, the pairs run counting toward
    /// `max_n`; a count's verdict keeps it until the guard escalates the unit to the time, which drops it then.
    #[test]
    fn a_dirty_pair_leaves_a_times_verdict_and_a_count_keeps_it() {
        let dirty = |runs: &[&[Sample]]| -> Vec<Vec<bool>> {
            runs.iter()
                .map(|r| r.iter().map(|s| s.metrics.get("foreign").is_some_and(|&f| f > 0.0)).collect())
                .collect()
        };
        let filtered = |min_n| Plan { filter: Some(Filter { times: &["ms"], dirty: &dirty }), ..counted(min_n) };
        // Every fifth call's run is dirty, its time ten times as long: pairs 3, 5 and 8 (calls 5, 10, 15) are dropped.
        let mut t = Metrics(
            0,
            Box::new(|_, _, i| {
                vec![
                    ("count", 1e9),
                    ("ms", if i % 5 == 0 { 1000.0 } else { 100.0 + jitter(i) }),
                    ("foreign", if i % 5 == 0 { 1.0 } else { 0.0 }),
                ]
            }),
        );
        let timed = "timed".to_string();
        let run_timed = |t: &mut Metrics| {
            let unit = Unit { of: &timed, arms: vec![0, 1], deciding: vec!["ms".into()] };
            run(t, &arms(), &[unit], &filtered(6), |_, _| Ok(())).unwrap()
        };
        let p = run_timed(&mut t);
        assert!(p[0].dropped > 0 && p[0].samples.iter().flatten().all(|s| s.metrics["ms"] < 200.0), "{:?}", p[0]);
        assert_eq!(
            (p[0].n(&[0, 1]), p[0].dropped, &p[0].dropped_at[..]),
            (6, 3, &[2, 4, 7][..]),
            "same on the six clean pairs of nine"
        );
        assert_eq!((p[0].runs[0].len(), p[0].runs[1].len()), (9, 9), "every run stays in runs");
        // Every pair dirty: nothing to judge, ended at max_n pairs run.
        let mut t = Metrics(0, Box::new(|_, _, _| vec![("count", 1e9), ("ms", 100.0), ("foreign", 1.0)]));
        let p = run_timed(&mut t);
        assert_eq!((p[0].n(&[0, 1]), p[0].dropped, p[0].done, p[0].ran(&[0, 1])), (0, 12, true, 12));
        // Judged by its count, every pair stays; escalated, the dirty ones go.
        let unit = |deciding: &str| Unit { of: &timed, arms: vec![0, 1], deciding: vec![deciding.into()] };
        let mut t = Metrics(
            0,
            Box::new(|_, _, i| vec![("count", 1e9), ("ms", 100.0), ("foreign", if i % 3 == 0 { 1.0 } else { 0.0 })]),
        );
        let p = run(&mut t, &arms(), &[unit("count")], &filtered(6), |_, _| Ok(())).unwrap();
        assert_eq!((p[0].n(&[0, 1]), p[0].dropped, p[0].escalated.as_deref()), (6, 0, None));
        let mut t = Metrics(
            0,
            Box::new(|arm, _, i| {
                vec![
                    ("count", 1e9),
                    ("ms", if arm == "n" { 150.0 } else { 100.0 } + jitter(i)),
                    ("foreign", if i % 3 == 0 { 1.0 } else { 0.0 }),
                ]
            }),
        );
        let p = run(&mut t, &arms(), &[unit("count")], &Plan { max_n: 30, ..filtered(6) }, |_, _| Ok(())).unwrap();
        assert_eq!(p[0].escalated.as_deref(), Some("time"));
        assert!(
            p[0].dropped >= 4 && p[0].n(&[0, 1]) >= 6,
            "the dirty pairs of the first six went at the escalation: {:?}",
            (p[0].dropped, p[0].n(&[0, 1]))
        );
    }

    /// The groups' defaults are their declarations', and a child given `to_args` parses the same values back.
    #[test]
    fn the_flag_groups_give_their_defaults_and_round_trip() {
        let host: HostArgs = defaults();
        assert_eq!(host, HostArgs { wait_reservation: 7200, wait_quiet: 1800, force: false });
        let pair = PairArgs { threshold: 0.05, floor_ms: 2.5, min_n: 3, max_n: 9, budget_s: 0.0, foreign_cpu: 0.5 };
        let cmd = <PairArgs as clap::Args>::augment_args(clap::Command::new("x"));
        let back = <PairArgs as clap::FromArgMatches>::from_arg_matches(
            &cmd.get_matches_from(std::iter::once("x".to_string()).chain(pair.to_args())),
        )
        .unwrap();
        assert_eq!(back, pair);
        assert_eq!(
            defaults::<PairArgs>(),
            PairArgs { threshold: 0.10, floor_ms: 5.0, min_n: 6, max_n: 30, budget_s: 120.0, foreign_cpu: 0.5 }
        );
    }

    /// The waits a policy skips return at once whatever the host does: a step judged by CPU, a step whose caller
    /// waited (`checked`), a pair under `PerStep`.
    #[test]
    fn a_host_skips_the_waits_its_policy_does_not_ask_for() {
        let host = |policy, checked| Host {
            policy,
            args: HostArgs { wait_reservation: 0, wait_quiet: 3600, force: false },
            checked,
        };
        let started = std::time::Instant::now();
        host(HostPolicy::PerStep, false).before_step(false).unwrap();
        host(HostPolicy::PerPair, true).before_step(true).unwrap();
        host(HostPolicy::PerStep, false).before_pair().unwrap();
        host(HostPolicy::PerPair, false).after_burst().unwrap();
        // `None` waits for nothing, a step judged by its wall included, and reserves nothing: the reservation it
        // returns holds no lock, so the host's own is free while it lives.
        host(HostPolicy::None, false).before_step(true).unwrap();
        host(HostPolicy::None, false).before_pair().unwrap();
        host(HostPolicy::None, false).after_burst().unwrap();
        let dir = std::env::temp_dir().join(format!("accio-bench-none-{}", std::process::id()));
        let held = host(HostPolicy::None, false).reserve("quick").unwrap();
        drop(Reservation::acquire_in(&dir, "another", Duration::ZERO).expect("the lock is free beside a quick run"));
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(started.elapsed() < Duration::from_secs(4), "no wait (a wait sleeps 5 s at a time)");
    }
}
