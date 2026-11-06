//! When two arms differ: interleaved paired differences, their 95 % interval robust to the heavy tails of timings (the
//! 25 %-trimmed mean's, `trimmed_interval`), against a threshold of `max(rel × base median, floor)`.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The interval lies inside ±threshold: no move worth a number.
    Same,
    /// The interval lies entirely below −threshold.
    Faster,
    /// The interval lies entirely above +threshold.
    Slower,
    /// `max_n` pairs and the interval still straddles a threshold.
    Unclear,
}

pub fn median(v: &[f64]) -> f64 {
    quantile(v, 0.5)
}

/// Linear interpolation between order statistics (numpy's default).
pub fn quantile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let pos = q * (s.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    s[lo] + (s[hi] - s[lo]) * (pos - lo as f64)
}

pub fn iqr(v: &[f64]) -> f64 {
    quantile(v, 0.75) - quantile(v, 0.25)
}

/// The correctly rounded sum, CPython's `math.fsum` (Shewchuk's partials, the last step rounding half to even): a
/// mean a quality suite saves is the same whatever the order, the platform or the Python of the run that filed it
/// (Python 3.12 made `sum()` of floats compensated, 3.11 adds left to right).
pub fn fsum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut partials: Vec<f64> = Vec::new();
    for mut x in values {
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        partials.push(x);
    }
    let Some(&last) = partials.last() else { return 0.0 };
    let (mut hi, mut lo, mut n) = (last, 0.0, partials.len() - 1);
    while n > 0 {
        let x = hi;
        n -= 1;
        let y = partials[n];
        hi = x + y;
        lo = y - (hi - x);
        if lo != 0.0 {
            break;
        }
    }
    // Half-even rounding of the last partial: `hi + 2·lo` is exact only when `lo` was half an ulp.
    if n > 0 && ((lo < 0.0 && partials[n - 1] < 0.0) || (lo > 0.0 && partials[n - 1] > 0.0)) {
        let y = lo * 2.0;
        let x = hi + y;
        if y == x - hi {
            hi = x;
        }
    }
    hi
}

/// `fsum(values) / count`, 0 for none.
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() { 0.0 } else { fsum(values.iter().copied()) / values.len() as f64 }
}

/// splitmix64's next value from `state`: a seeded stream that is the same on every platform and toolchain (the SCIP
/// extracts' shuffle, `ab-sim`'s runs).
pub fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The statistic of the paired differences an interval is of.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Statistic {
    /// The median: the sign test's distribution-free order statistics.
    Median,
    /// The mean with this share of each end trimmed (Yuen, 1974).
    Trimmed(f64),
    /// The range of the differences, their least to their greatest, while it is narrower than this share of the
    /// threshold and spans at least `SPREAD_MIN_N` pairs: a metric with next to no noise decides from the first such
    /// look, the pairs agreeing to a fraction of the threshold; a wider range, or fewer pairs, falls back to the
    /// median's sign-test interval (none below six pairs).
    Spread(f64),
}

/// The fewest pairs whose range `Statistic::Spread` takes as an interval: one or two agreeing pairs are no evidence,
/// whatever `--min-n` (or `settle`'s look at one pair) allows.
pub const SPREAD_MIN_N: usize = 3;

/// How a run of interleaved pairs is judged: the interval a `same` needs (`alpha_same`, two-sided) and the one a move
/// needs (`alpha_move`), both of `statistic`, looked at after every pair from `min_n`. Two levels, because the two
/// errors cost differently: a false `same` at a real move hides a regression, a false move trains people to wave
/// verdicts away; and because a look after every pair spends its level once per look (`ab-sim` measures both errors
/// of each rule over the looks of a whole run).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rule {
    pub statistic: Statistic,
    pub alpha_same: f64,
    pub alpha_move: f64,
    /// A `same` waits while the pairs past the threshold lean to the new arm and are not yet a flag (`tail_open`):
    /// a stall in a share of the new arm's runs, which the statistic leaves out, gets the pairs `tail_slower` needs.
    pub tail_hold: bool,
}

impl Rule {
    /// The default: the sign test's 95 % interval of the median, for both verdicts, its `same` held for the tail. In
    /// `ab-sim` the hold leaves both errors where the median's alone has them (false `same` at 1.2 thresholds 3.6 →
    /// 3.4 %, AR(1)) for 0.25 more pairs at no move, and lifts the share of a fifth-of-runs stall flagged 28 → 37 %.
    pub const MEDIAN: Rule = Rule { statistic: Statistic::Median, alpha_same: 0.05, alpha_move: 0.05, tail_hold: true };
    /// The opt-in rule: the 20 %-trimmed mean at 99 % for both verdicts. In `ab-sim` (docs/measurements/speed.md, "ab's
    /// stopping rule, both errors over whole runs") it has fewer false `same`s than the median's at 1.2 and 1.5
    /// thresholds under every model, and fewer false moves except at no move on the recorded noise (0.06 → 0.16 %),
    /// but it takes more pairs: no trimmed rule was both no worse in both errors and faster. At 95 % the trimmed mean
    /// was faster (11.9 pairs against 15.3 at no move) and doubled the false `same` (3.6 → 7.2 % at 1.2 thresholds).
    pub const TRIMMED: Rule =
        Rule { statistic: Statistic::Trimmed(0.20), alpha_same: 0.01, alpha_move: 0.01, tail_hold: true };
    /// The rule of a count (instructions retired): from three pairs (`SPREAD_MIN_N`) their range inside a quarter of
    /// the threshold decides, else the sign test as `MEDIAN` without its tail hold. On recorded A/A instructions
    /// (docs/measurements/speed.md, "instructions as `ab`'s metric: their noise and `Rule::COUNT`") `ab-sim` gives it
    /// the median's errors for 5.6 pairs at no move against 6.8; on a time's noise the range shortcut costs false
    /// `same`s (3.58 → 3.79 % at 1.2 thresholds), so it is for counts only.
    pub const COUNT: Rule =
        Rule { statistic: Statistic::Spread(0.25), alpha_same: 0.05, alpha_move: 0.05, tail_hold: false };

    /// The rule without its tail hold: a run stops at its verdict whatever the tail (build-ab, whose rebuilds cost a
    /// minute each, and `ab-sim`'s comparison).
    pub const fn without_tail_hold(self) -> Rule {
        Rule { tail_hold: false, ..self }
    }

    /// The interval at two-sided level `alpha` and its point estimate; `None` below six values (where even the
    /// median's extremes are no 95 % interval, unless a `Spread` of at least `SPREAD_MIN_N` values is narrow enough
    /// against `threshold`) or where the level is out of the statistic's reach at this n.
    pub fn interval(&self, d: &[f64], alpha: f64, threshold: f64) -> Option<(f64, f64, f64)> {
        let median_iv = || median_interval(d, alpha).map(|(lo, hi)| (lo, hi, median(d)));
        match self.statistic {
            Statistic::Median => median_iv(),
            Statistic::Trimmed(trim) => trimmed_interval(d, trim, alpha),
            Statistic::Spread(share) => {
                let (lo, hi) = d.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &x| (lo.min(x), hi.max(x)));
                if d.len() >= SPREAD_MIN_N && hi - lo < share * threshold {
                    Some((lo, hi, median(d)))
                } else {
                    median_iv()
                }
            }
        }
    }

    /// `median` or `trimmed 25 %`, and the two levels where they differ from 95 %.
    pub fn label(&self) -> String {
        let stat = match self.statistic {
            Statistic::Median => "median".to_string(),
            Statistic::Trimmed(t) => format!("trimmed {:.0} %", t * 100.0),
            Statistic::Spread(s) => format!("range under {:.0} % of the threshold, else median", s * 100.0),
        };
        let levels = if self.alpha_same == 0.05 && self.alpha_move == 0.05 {
            stat
        } else {
            format!(
                "{stat}, same at {:.1} %, a move at {:.1} %",
                (1.0 - self.alpha_same) * 100.0,
                (1.0 - self.alpha_move) * 100.0
            )
        };
        if self.tail_hold { format!("{levels}, holding for the tail") } else { levels }
    }
}

/// The rules `--rule` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleName {
    /// The sign test's 95 % interval of the median, for both verdicts (`Rule::MEDIAN`).
    #[default]
    Median,
    /// The 20 %-trimmed mean's 99 % interval (`Rule::TRIMMED`): fewer false `same`s, more robust to bursts, more pairs.
    Trimmed,
    /// Counts only, such as instructions (`Rule::COUNT`): from three pairs, and from `--min-n` when that is more
    /// (`ab` takes six unless told otherwise), differences agreeing within a quarter of the threshold decide, others
    /// take the median's sign test. A time's noise gets false `same`s from it, so a run judged by a time refuses it.
    Count,
}

impl RuleName {
    pub fn rule(self) -> Rule {
        match self {
            RuleName::Median => Rule::MEDIAN,
            RuleName::Trimmed => Rule::TRIMMED,
            RuleName::Count => Rule::COUNT,
        }
    }

    /// The rule of a run judged by a time (wall, CPU, build seconds): `count` is refused, its range shortcut being
    /// for counts only (docs/measurements/speed.md, "instructions as `ab`'s metric").
    pub fn for_time(self, metric: &str) -> anyhow::Result<Rule> {
        if self == RuleName::Count {
            anyhow::bail!(
                "--rule count is for a count such as instructions; this run is judged by {metric}, a time, whose noise gets false `same`s from its range shortcut: use median or trimmed"
            );
        }
        Ok(self.rule())
    }

    /// The rule a step judged by times takes when a run of several steps is given this one (`ab-run`'s `build-ab`
    /// step, whose metrics are seconds): `count` there is `median`.
    pub fn of_times(self) -> RuleName {
        if self == RuleName::Count { RuleName::Median } else { self }
    }

    /// The value of `--rule` that names it to a child.
    pub fn arg(self) -> String {
        clap::ValueEnum::to_possible_value(&self).expect("no rule is skipped").get_name().to_string()
    }
}

/// The two-sided `1 - alpha` interval of the median of `d`: `[d(k), d(n-k+1)]` with the largest `k` whose binomial
/// tail `2·P(X ≤ k-1)`, X ~ Bin(n, ½), stays within `alpha`. `None` below the n where even the extremes are no
/// interval of this level (six at 95 %).
pub fn median_interval(d: &[f64], alpha: f64) -> Option<(f64, f64)> {
    let n = d.len();
    let mut s = d.to_vec();
    s.sort_by(f64::total_cmp);
    // P(X <= i) accumulated term by term: C(n, i) / 2^n.
    let mut term = 0.5f64.powi(n as i32);
    let mut tail = 0.0;
    let mut k = 0;
    for i in 0..n {
        tail += term;
        if 2.0 * tail > alpha {
            break;
        }
        k = i + 1;
        term *= (n - i) as f64 / (i + 1) as f64;
    }
    (k >= 1).then(|| (s[k - 1], s[n - k]))
}

/// ln Γ(x) for x > 0 (Lanczos, g = 7, nine terms; relative error under 1e-14).
fn ln_gamma(x: f64) -> f64 {
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let t = x + 7.5;
    let mut a = C[0];
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// The regularized incomplete beta I_x(a, b) by its continued fraction (modified Lentz).
fn incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - incomplete_beta(1.0 - x, b, a);
    }
    let front = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp() / a;
    let tiny = 1e-300;
    let (mut c, mut d) = (1.0, 1.0 - (a + b) * x / (a + 1.0));
    d = if d.abs() < tiny { 1.0 / tiny } else { 1.0 / d };
    let mut f = d;
    for m in 1..300 {
        let m = m as f64;
        for num in [
            m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m)),
            -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0)),
        ] {
            d = 1.0 + num * d;
            d = if d.abs() < tiny { 1.0 / tiny } else { 1.0 / d };
            c = 1.0 + num / c;
            if c.abs() < tiny {
                c = tiny;
            }
            f *= c * d;
        }
        if (c * d - 1.0).abs() < 1e-15 {
            break;
        }
    }
    front * f
}

/// Student's t quantile at `p` (> ½) for `df` degrees of freedom: the CDF `1 − ½·I_{df/(df+t²)}(df/2, ½)` inverted by
/// bisection, memoized per (p, df).
pub fn t_quantile(p: f64, df: usize) -> f64 {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local!(static MEMO: RefCell<HashMap<(u64, usize), f64>> = RefCell::new(HashMap::new()));
    MEMO.with(|m| {
        *m.borrow_mut().entry((p.to_bits(), df)).or_insert_with(|| {
            let v = df.max(1) as f64;
            let cdf = |t: f64| 1.0 - 0.5 * incomplete_beta(v / (v + t * t), v / 2.0, 0.5);
            let (mut lo, mut hi) = (0.0, 1e4);
            for _ in 0..200 {
                let mid = 0.5 * (lo + hi);
                if cdf(mid) < p { lo = mid } else { hi = mid }
            }
            0.5 * (lo + hi)
        })
    })
}

/// The two-sided `1 − alpha` interval of the `trim`-trimmed mean of `d` (Yuen, 1974), and the trimmed mean:
/// `g = ⌊trim·n⌋` values off each end, the mean of the `h = n − 2g` left, its standard error from the variance of the
/// sample winsorized at the same points, `√((n−1)·s²_w / (h·(h−1)))`, Student's t with `h − 1` degrees of freedom.
/// `None` below six values. The effective trim steps with n (at 25 %: 1/6 at six, 1/7 at seven, 1/4 at eight), so the
/// interval can widen by a pair for no reason in the data.
pub fn trimmed_interval(d: &[f64], trim: f64, alpha: f64) -> Option<(f64, f64, f64)> {
    let n = d.len();
    if n < 6 {
        return None;
    }
    let mut s = d.to_vec();
    s.sort_by(f64::total_cmp);
    let g = (trim * n as f64).floor() as usize;
    let h = n - 2 * g;
    let kept = &s[g..n - g];
    let mean = fsum(kept.iter().copied()) / h as f64;
    let (lo, hi) = (s[g], s[n - g - 1]);
    let win: Vec<f64> = s.iter().map(|&x| x.clamp(lo, hi)).collect();
    let wmean = fsum(win.iter().copied()) / n as f64;
    let var = fsum(win.iter().map(|x| (x - wmean) * (x - wmean))) / (n - 1) as f64;
    let se = ((n - 1) as f64 * var / (h * (h - 1)) as f64).sqrt();
    let t = t_quantile(1.0 - alpha / 2.0, h - 1);
    Some((mean - t * se, mean + t * se, mean))
}

/// A run's judgement: the verdict, the interval it rests on (the `same` level's for a `same`, the move level's
/// otherwise) and the statistic's point estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decision {
    pub verdict: Verdict,
    pub interval: (f64, f64),
    pub estimate: f64,
}

/// The judgement of a query after `base.len()` interleaved pairs, `None` while it needs more: `rule`'s intervals of the
/// paired differences against `±max(rel × base median, floor)`, looked at after every pair from `min_n`. Inside the
/// band at the `same` level it is the same, beyond one side at the move level faster or slower, neither at `max_n`
/// unclear. A function of the values alone: the same sequence decides the same at the same pair.
pub fn decide(
    rule: &Rule,
    base: &[f64],
    new: &[f64],
    rel: f64,
    floor: f64,
    min_n: usize,
    max_n: usize,
) -> Option<Decision> {
    let n = base.len().min(new.len());
    if n < min_n {
        return None;
    }
    let d: Vec<f64> = new.iter().zip(base).map(|(n, b)| n - b).collect();
    let threshold = (rel * median(base)).max(floor);
    let same = rule.interval(&d, rule.alpha_same, threshold);
    let moved = if rule.alpha_move == rule.alpha_same { same } else { rule.interval(&d, rule.alpha_move, threshold) };
    let decision = |verdict, iv: Option<(f64, f64, f64)>| {
        let (lo, hi, est) = iv.unwrap_or((f64::NAN, f64::NAN, median(&d)));
        Some(Decision { verdict, interval: (lo, hi), estimate: est })
    };
    match (same, moved) {
        (Some((lo, hi, _)), _)
            if lo >= -threshold && hi <= threshold && !(rule.tail_hold && n < max_n && tail_open(&d, threshold)) =>
        {
            decision(Verdict::Same, same)
        }
        (_, Some((lo, _, _))) if lo > threshold => decision(Verdict::Slower, moved),
        (_, Some((_, hi, _))) if hi < -threshold => decision(Verdict::Faster, moved),
        _ if n >= max_n => decision(Verdict::Unclear, moved),
        _ => None,
    }
}

/// The verdict at the pairs there are, for a value that does not decide when a run stops: `decide` with no more
/// pairs to come, so an interval still straddling a threshold is `Unclear`. `None` without a pair.
pub fn settle(rule: &Rule, base: &[f64], new: &[f64], rel: f64, floor: f64) -> Option<Decision> {
    let n = base.len().min(new.len());
    if n == 0 {
        return None;
    }
    decide(rule, base, new, rel, floor, 1, n)
}

/// The pairs whose difference new − base passes the threshold, `(up, down)`: the new arm slower by more than it, and
/// faster by more than it. A neighbour's burst lands on either arm of a pair; a stall of the new build in a share of
/// its runs lands up only, and the median or a trimmed mean of the differences leaves it out.
pub fn exceedances(d: &[f64], threshold: f64) -> (usize, usize) {
    (d.iter().filter(|&&x| x > threshold).count(), d.iter().filter(|&&x| x < -threshold).count())
}

/// The one-sided p of `up` or more of the `up + down` pairs past the threshold lying up, when either side is as
/// likely (the arms exchangeable): the binomial tail at ½. Three of three is 0.125, five of five 0.031.
pub fn tail_p(up: usize, down: usize) -> f64 {
    let m = up + down;
    let ln_choose =
        |n: usize, k: usize| ln_gamma(n as f64 + 1.0) - ln_gamma(k as f64 + 1.0) - ln_gamma((n - k) as f64 + 1.0);
    let p: f64 = (up..=m).map(|x| (ln_choose(m, x) - m as f64 * std::f64::consts::LN_2).exp()).sum();
    p.min(1.0)
}

/// The level below which the pairs past the threshold flag `TAIL SLOWER`.
pub const TAIL_ALPHA: f64 = 0.05;

/// The level below which a `same` waits for more pairs (`Rule::tail_hold`): two of two pairs up (0.25) and leaner.
pub const TAIL_HOLD: f64 = 0.25;

/// The pairs past the threshold lean to the new arm and are not a flag yet: a `same` waits (`decide`).
pub fn tail_open(d: &[f64], threshold: f64) -> bool {
    let (up, down) = exceedances(d, threshold);
    let p = tail_p(up, down);
    (TAIL_ALPHA..=TAIL_HOLD).contains(&p)
}

/// The flag `ab` puts beside a verdict that is not `slower`: more of the pairs past the threshold lie up than chance
/// gives (`tail_p` under `TAIL_ALPHA`, five pairs at least). Non-gating.
pub fn tail_slower(base: &[f64], new: &[f64], threshold: f64) -> bool {
    let d: Vec<f64> = new.iter().zip(base).map(|(n, b)| n - b).collect();
    let (up, down) = exceedances(&d, threshold);
    tail_p(up, down) < TAIL_ALPHA
}

/// Two arms' values as a report states them: the median paired difference new − base, the new median against the base
/// median in percent, and the point gate of one comparison of medians (new > base × (1 + rel) and more than `floor`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    pub delta: f64,
    pub delta_pct: Option<f64>,
    pub breach: bool,
}

pub fn compare(base: &[f64], new: &[f64], rel: f64, floor: f64) -> Comparison {
    let d: Vec<f64> = new.iter().zip(base).map(|(n, b)| n - b).collect();
    let (mb, mn) = (median(base), median(new));
    Comparison {
        delta: median(&d),
        delta_pct: (mb > 0.0).then(|| (mn - mb) / mb * 100.0),
        breach: mn > mb * (1.0 + rel) && mn - mb > floor,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const T: Rule = Rule::TRIMMED;
    const M: Rule = Rule::MEDIAN;

    #[test]
    fn a_settled_value_is_unclear_where_a_deciding_one_would_go_on() {
        let base = [100.0, 101.0, 99.0, 100.5, 99.5, 100.2];
        let noisy = [100.0, 130.0, 80.0, 125.0, 90.0, 110.0];
        for rule in [M, T] {
            assert_eq!(decide(&rule, &base, &noisy, 0.05, 2.0, 6, 20), None);
            assert_eq!(settle(&rule, &base, &noisy, 0.05, 2.0).map(|d| d.verdict), Some(Verdict::Unclear));
            assert_eq!(settle(&rule, &base, &base, 0.05, 2.0).map(|d| d.verdict), Some(Verdict::Same));
            assert_eq!(settle(&rule, &[], &[], 0.05, 2.0), None);
        }
        let c = compare(&[100.0, 100.0, 100.0], &[120.0, 121.0, 119.0], 0.1, 5.0);
        assert_eq!((c.delta, c.delta_pct, c.breach), (20.0, Some(20.0), true));
    }

    /// A deterministic jitter in [-1, 1).
    fn noise(i: usize, seed: u64) -> f64 {
        let mut x = (i as u64 + 1).wrapping_mul(6364136223846793005).wrapping_add(seed);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51afd7ed558ccd);
        x ^= x >> 33;
        (x % 2000) as f64 / 1000.0 - 1.0
    }

    /// Feed pairs one at a time, as the runner does, until a verdict.
    fn run(rule: &Rule, base: impl Fn(usize) -> f64, new: impl Fn(usize) -> f64) -> (Verdict, usize) {
        let (mut b, mut n) = (Vec::new(), Vec::new());
        for i in 0.. {
            b.push(base(i));
            n.push(new(i));
            if let Some(d) = decide(rule, &b, &n, 0.10, 5.0, 6, 30) {
                return (d.verdict, b.len());
            }
        }
        unreachable!()
    }

    #[test]
    fn the_intervals_need_six_pairs_and_the_trimmed_one_ignores_the_tails() {
        assert_eq!(median_interval(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.05), None);
        assert_eq!(median_interval(&[6.0, 1.0, 5.0, 2.0, 4.0, 3.0], 0.05), Some((1.0, 6.0)));
        let d: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(median_interval(&d, 0.05), Some((6.0, 15.0)));
        assert_eq!(trimmed_interval(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.25, 0.05), None);
        // n 6: one value off each end, the mean 3.5 of 2..5, the winsorized 2 2 3 4 5 5 (variance 1.9), t(3).
        let (lo, hi, est) = trimmed_interval(&[6.0, 1.0, 5.0, 2.0, 4.0, 3.0], 0.25, 0.05).unwrap();
        let half = 3.182_446 * (5.0f64 * 1.9 / 12.0).sqrt();
        assert!((lo - (3.5 - half)).abs() < 1e-5 && (hi - (3.5 + half)).abs() < 1e-5 && est == 3.5, "{lo} {hi}");
        // An outlier of a thousand moves it no more than a value at the trim point.
        let calm = trimmed_interval(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 0.25, 0.05).unwrap();
        assert_eq!(trimmed_interval(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 1000.0], 0.25, 0.05), Some(calm));
        assert_eq!(trimmed_interval(&[4.0; 6], 0.25, 0.05), Some((4.0, 4.0, 4.0)));
    }

    /// Student's t from the incomplete beta meets the tables.
    #[test]
    fn t_quantiles_are_the_tables() {
        for (p, df, t) in [
            (0.975, 1, 12.706),
            (0.975, 3, 3.182),
            (0.975, 30, 2.042),
            (0.995, 5, 4.032),
            (0.995, 10, 3.169),
            (0.9975, 4, 5.598),
        ] {
            assert!((t_quantile(p, df) - t).abs() < 1e-3, "t({p}, {df}) = {} against {t}", t_quantile(p, df));
        }
    }

    #[test]
    fn a_clear_result_stops_early_and_noise_runs_to_max_n() {
        for rule in [M, T] {
            // 100 ms ± 2 against itself: the same after the minimum.
            assert_eq!(
                run(&rule, |i| 100.0 + 2.0 * noise(i, 1), |i| 100.0 + 2.0 * noise(i, 2)),
                (Verdict::Same, 6),
                "{rule:?}"
            );
            // 30 % faster with the same jitter: clearly beyond 10 %.
            assert_eq!(
                run(&rule, |i| 100.0 + 2.0 * noise(i, 3), |i| 70.0 + 2.0 * noise(i, 4)),
                (Verdict::Faster, 6),
                "{rule:?}"
            );
            assert_eq!(
                run(&rule, |i| 100.0 + 2.0 * noise(i, 5), |i| 130.0 + 2.0 * noise(i, 6)),
                (Verdict::Slower, 6),
                "{rule:?}"
            );
            // A 10 % move with ±15 ms jitter straddles the threshold: no verdict before max_n.
            assert_eq!(
                run(&rule, |i| 100.0 + 15.0 * noise(i, 7), |i| 110.0 + 15.0 * noise(i, 8)),
                (Verdict::Unclear, 30),
                "{rule:?}"
            );
            // Below the 5 ms floor a 40 % move of a 10 ms query is the same.
            assert_eq!(
                run(&rule, |i| 10.0 + 0.2 * noise(i, 9), |i| 6.0 + 0.2 * noise(i, 10)),
                (Verdict::Same, 6),
                "{rule:?}"
            );
        }
    }

    /// One run slowed by a neighbour does not hold a trimmed query that is the same, where the median's interval (the
    /// extremes at six) takes it in. A fixed sequence decides the same, at the same pair, with the same interval.
    #[test]
    fn an_outlier_run_does_not_hold_a_trimmed_query_and_a_sequence_decides_alike() {
        let base: Vec<f64> = (0..8).map(|i| 100.0 + 2.0 * noise(i, 11)).collect();
        let mut new: Vec<f64> = (0..8).map(|i| 100.0 + 2.0 * noise(i, 12)).collect();
        new[2] = 400.0;
        let first = decide(&T, &base, &new, 0.10, 5.0, 6, 30);
        assert_eq!(first.map(|d| d.verdict), Some(Verdict::Same), "{first:?}");
        assert_eq!(first, decide(&T, &base, &new, 0.10, 5.0, 6, 30));
        assert_eq!(decide(&M, &base[..6], &new[..6], 0.10, 5.0, 6, 30), None);
        let seq = || run(&T, |i| 100.0 + 15.0 * noise(i, 7), |i| 110.0 + 15.0 * noise(i, 8));
        assert_eq!(seq(), seq());
    }

    /// A stall in a fifth of the new arm's runs, which the statistic leaves out: the binomial tail of the pairs past
    /// the threshold (2 of 2: 0.25, 5 of 5: 0.031); the rule's `same` waits while they lean up and no flag stands yet,
    /// then stops `same` with the flag; without the hold it stops `same` at the minimum, unflagged.
    #[test]
    fn a_tail_regression_holds_a_same_until_it_is_flagged() {
        assert!(
            (tail_p(2, 0) - 0.25).abs() < 1e-12
                && (tail_p(5, 0) - 0.03125).abs() < 1e-12
                && (tail_p(3, 3) - 0.65625).abs() < 1e-12
        );
        assert_eq!(tail_p(0, 0), 1.0);
        let base: Vec<f64> = (0..30).map(|i| 100.0 + noise(i, 15)).collect();
        let new: Vec<f64> = (0..30).map(|i| 100.0 + noise(i, 16) + if i % 5 == 1 { 60.0 } else { 0.0 }).collect();
        let stop = |rule: &Rule| {
            (6..=30)
                .find_map(|n| decide(rule, &base[..n], &new[..n], 0.10, 5.0, 6, 30).map(|d| (d.verdict, n)))
                .unwrap()
        };
        let ((held, at), (free, free_at)) = (stop(&M), stop(&M.without_tail_hold()));
        assert_eq!((held, free), (Verdict::Same, Verdict::Same));
        assert!(
            free_at < at && !tail_slower(&base[..free_at], &new[..free_at], 10.0),
            "unheld: same at {free_at}, unflagged"
        );
        assert!(
            tail_slower(&base[..at], &new[..at], 10.0) && !tail_slower(&base[..at - 1], &new[..at - 1], 10.0),
            "held to {at}, the pair that flags"
        );
        assert!(!tail_slower(&new, &base, 10.0), "the base's stalls are no regression");
        // No stall, no hold: the noise never passes the threshold.
        assert_eq!(
            (6..=30).find_map(|n| decide(&M, &base[..n], &base[..n], 0.10, 5.0, 6, 30).map(|d| (d.verdict, n))),
            Some((Verdict::Same, 6))
        );
    }

    /// A count whose pairs agree within a quarter of the threshold decides at three pairs; one that agrees near the
    /// threshold waits; a noisy one is the median's sign test without the tail hold, look for look.
    #[test]
    fn a_count_decides_at_three_pairs_and_noise_falls_back_to_the_sign_test() {
        let c = Rule::COUNT;
        let at = |rule: &Rule, base: &dyn Fn(usize) -> f64, new: &dyn Fn(usize) -> f64| {
            let (mut b, mut n) = (Vec::new(), Vec::new());
            (0..30).find_map(|i| {
                b.push(base(i));
                n.push(new(i));
                decide(rule, &b, &n, 0.10, 0.0, 3, 30).map(|d| (d.verdict, b.len()))
            })
        };
        // 1e9 instructions ± 0.1 %: the same, 20 % more slower, 20 % fewer faster, all at the first look.
        let tight = |i: usize, seed: u64| 1e9 * (1.0 + 0.001 * noise(i, seed));
        assert_eq!(at(&c, &|i| tight(i, 1), &|i| tight(i, 2)), Some((Verdict::Same, 3)));
        assert_eq!(at(&c, &|i| tight(i, 3), &|i| 1.2 * tight(i, 4)), Some((Verdict::Slower, 3)));
        assert_eq!(at(&c, &|i| tight(i, 5), &|i| 0.8 * tight(i, 6)), Some((Verdict::Faster, 3)));
        // A move of the threshold itself: the range straddles it, as does every later interval.
        assert_eq!(at(&c, &|i| tight(i, 7), &|i| 1.1 * tight(i, 8)), Some((Verdict::Unclear, 30)));
        // ± 3 % of noise (a range of 12 % against 2.5 %): the sign test's verdicts at the sign test's pairs.
        let wide = |i: usize, seed: u64| 1e9 * (1.0 + 0.03 * noise(i, seed));
        for shift in [1.0, 1.3, 0.7, 1.08] {
            let (b, n) = (|i| wide(i, 9), move |i| shift * wide(i, 10));
            assert_eq!(at(&c, &b, &n), at(&M.without_tail_hold(), &b, &n), "{shift}");
        }
        assert_eq!(c.label(), "range under 25 % of the threshold, else median");
        assert_eq!(RuleName::Count.rule(), c);
    }

    /// One or two agreeing pairs are no range a count decides on, whatever `min_n` lets look (`settle` looks at one
    /// pair): the median's sign test has no interval there, so `decide` waits and `settle` is unclear; three decide.
    #[test]
    fn a_count_needs_three_pairs_whatever_min_n() {
        let c = Rule::COUNT;
        let (base, same, slower) = ([1e9, 1.0001e9, 0.9999e9], [1e9; 3], [1.2e9; 3]);
        for n in 1..SPREAD_MIN_N {
            assert_eq!(c.interval(&same[..n], 0.05, 1e8), None, "n {n}");
            assert_eq!(decide(&c, &base[..n], &slower[..n], 0.10, 0.0, 1, 30), None, "n {n}");
            assert_eq!(
                settle(&c, &base[..n], &same[..n], 0.10, 0.0).map(|d| d.verdict),
                Some(Verdict::Unclear),
                "n {n}"
            );
            let estimate = |rule: &Rule| {
                settle(rule, &base[..n], &same[..n], 0.10, 0.0).map(|d| (d.verdict, d.estimate, d.interval.0.is_nan()))
            };
            assert_eq!(estimate(&c), estimate(&M.without_tail_hold()), "the median's no-interval, n {n}");
        }
        assert_eq!(decide(&c, &base, &slower, 0.10, 0.0, 1, 30).map(|d| d.verdict), Some(Verdict::Slower));
        assert_eq!(settle(&c, &base, &same, 0.10, 0.0).map(|d| d.verdict), Some(Verdict::Same));
    }

    /// A run judged by a time refuses `count`, naming the metric; the others pass. A step of times given `count` takes
    /// `median`, and each rule names itself to a child by its flag value.
    #[test]
    fn count_is_refused_on_a_time_and_names_itself_to_a_child() {
        let err = RuleName::Count.for_time("build seconds").unwrap_err().to_string();
        assert!(err.contains("--rule count") && err.contains("build seconds"), "{err}");
        assert_eq!((RuleName::Median.for_time("wall").unwrap(), RuleName::Trimmed.for_time("wall").unwrap()), (M, T));
        assert_eq!(
            [RuleName::Median, RuleName::Trimmed, RuleName::Count].map(RuleName::of_times),
            [RuleName::Median, RuleName::Trimmed, RuleName::Median]
        );
        assert_eq!(
            [RuleName::Median, RuleName::Trimmed, RuleName::Count].map(RuleName::arg),
            ["median", "trimmed", "count"]
        );
    }

    #[test]
    fn fsum_is_python_math_fsum() {
        // Python's `math.fsum` of each; a left-to-right sum gives 0.9999999999999999, 0 and 1.0.
        assert_eq!(fsum([0.1; 10]), 1.0);
        assert_eq!(fsum([1e100, 1.0, -1e100, 1e-100, 1e50, -1.0, -1e50]), 1e-100);
        assert_eq!(fsum([1.0, 1e-16, 1e-16]), 1.0000000000000002);
        assert_eq!(fsum([]), 0.0);
        assert_eq!(mean(&[]), 0.0);
    }

    #[test]
    fn quantiles_interpolate() {
        assert_eq!(median(&[3.0, 1.0, 2.0, 10.0]), 2.5);
        assert_eq!(iqr(&[1.0, 2.0, 3.0, 4.0, 5.0]), 2.0);
    }
}
