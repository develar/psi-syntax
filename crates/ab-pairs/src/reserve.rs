//! The measurement reservation of a host: one measurement at a time per machine, the next one queued on the lock
//! rather than refused, and a run that starts only in a quiet window.
//!
//! The lock is `measure.lock` in `dir()`, an exclusive `flock` held for the measurement's life (the kernel drops it
//! when the holder dies, so a killed run leaves no stale reservation). `measure.holder` beside it names the holder for
//! whoever waits. A child of a holder (`accio-bench reserve -- …` around another tool) sees `ACCIO_BENCH_RESERVATION`
//! and does not wait on its own parent.
use crate::sys;
use anyhow::{Context, Result, bail};
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const HELD: &str = "ACCIO_BENCH_RESERVATION";

/// `ACCIO_BENCH_CACHE`, else `~/.cache/accio-bench`: where the host's one reservation lives, so that every A/B on this
/// machine queues on one lock whichever tool runs it (accio's `scripts/bench` keeps its pinned clones and homes there).
pub fn dir() -> PathBuf {
    std::env::var_os("ACCIO_BENCH_CACHE").map(PathBuf::from).unwrap_or_else(|| {
        let home =
            std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
        home.join(".cache/accio-bench")
    })
}

/// A held reservation; dropping it releases the host.
pub struct Reservation {
    _lock: Option<File>,
    holder: Option<PathBuf>,
}

impl Reservation {
    /// Wait up to `wait` for the host's reservation, printing whom it waits for once a minute.
    pub fn acquire(what: &str, wait: Duration) -> Result<Reservation> {
        if std::env::var_os(HELD).is_some() {
            return Ok(Reservation::none());
        }
        Self::acquire_in(&dir(), what, wait)
    }

    /// No reservation at all: what a run that queues for nobody holds (`pairs::HostPolicy::None`, `--quick`).
    pub fn none() -> Reservation {
        Reservation { _lock: None, holder: None }
    }

    pub fn acquire_in(dir: &Path, what: &str, wait: Duration) -> Result<Reservation> {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        let lock = File::options().create(true).truncate(false).write(true).open(dir.join("measure.lock"))?;
        let holder = dir.join("measure.holder");
        let start = Instant::now();
        let mut told = None::<Instant>;
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(e)) => return Err(e).context("cannot lock the measurement reservation"),
            }
            let held_by = std::fs::read_to_string(&holder).unwrap_or_else(|_| "another measurement".into());
            if start.elapsed() >= wait {
                bail!(
                    "the host's measurement reservation is still held after {} s by {}",
                    wait.as_secs(),
                    held_by.trim()
                );
            }
            if told.is_none_or(|t| t.elapsed() >= Duration::from_secs(60)) {
                eprintln!(
                    "waiting for the measurement reservation ({} s so far), held by {}",
                    start.elapsed().as_secs(),
                    held_by.trim()
                );
                told = Some(Instant::now());
            }
            sys::sleep(Duration::from_millis(500))?;
        }
        let since = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let tmp = dir.join(format!("measure.holder.{}", std::process::id()));
        std::fs::write(&tmp, format!("pid {} since {since} (unix s): {what}\n", std::process::id()))?;
        std::fs::rename(&tmp, &holder)?;
        if start.elapsed() >= Duration::from_secs(1) {
            eprintln!("measurement reservation taken after {} s", start.elapsed().as_secs());
        }
        Ok(Reservation { _lock: Some(lock), holder: Some(holder) })
    }
}

/// Fails once the process whose reservation this one runs under (`HELD`) is gone: a child of a killed `ab-run` or
/// `reserve` must not go on measuring without the reservation, beside the run a resume starts.
///
/// Unix only: elsewhere `sys::pid_alive` cannot tell, and an orphan dies of its closed stderr pipe anyway. A pid the
/// OS gave another process since counts as alive.
pub fn check_holder() -> Result<()> {
    let Some(pid) = std::env::var(HELD).ok().and_then(|p| p.parse::<u32>().ok()) else { return Ok(()) };
    if cfg!(unix) && !sys::pid_alive(pid) {
        bail!("the process that holds this run's reservation (pid {pid}) is gone");
    }
    Ok(())
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(h) = &self.holder {
            let _ = std::fs::remove_file(h);
        }
    }
}

/// What the host looks like right now.
#[derive(Debug, Clone)]
pub struct Quiet {
    pub load: f64,
    pub cores: usize,
    pub busy: Vec<String>,
}

impl Quiet {
    /// A laptop with an IDE and agents idles at a third of its cores, and an IDE build doubles rebuild times (29 s
    /// against 16): the 1-minute load under three quarters of the cores. One process at 100 % of one core passes
    /// that and still moves a full rebuild by 10 % (PITFALLS, 2026-10-21): no process above half a core either.
    pub fn is_quiet(&self) -> bool {
        self.load <= self.cores as f64 * 0.75 && self.busy.is_empty()
    }
}

pub fn probe() -> Quiet {
    Quiet {
        load: sys::load_average().unwrap_or(0.0),
        cores: sys::cpu_count(),
        busy: sys::busy_processes(&sys::processes(), std::process::id(), 50.0),
    }
}

/// Wait until the host is quiet, at most `timeout`; the load when it was. A host that stays loaded is an error that
/// names the load and the busy processes. `force` measures at once.
pub fn wait_quiet(timeout: Duration, force: bool) -> Result<f64> {
    wait_quiet_with(probe, sys::sleep, timeout, force)
}

/// Wait until no other process is busy (above half a core), at most `timeout`, whatever the load average: between the
/// builds of a run, whose own load the 1-minute average still holds, another session's tests are what moves the next
/// build (a build A/B on the Studio once ran 8 s and 42 s builds in one run). The load when it was.
pub fn wait_unshared(timeout: Duration, force: bool) -> Result<f64> {
    wait_until(probe, sys::sleep, timeout, force, |q| q.busy.is_empty())
}

pub fn wait_quiet_with(
    probe: impl FnMut() -> Quiet,
    sleep: impl FnMut(Duration) -> Result<()>,
    timeout: Duration,
    force: bool,
) -> Result<f64> {
    wait_until(probe, sleep, timeout, force, Quiet::is_quiet)
}

/// The milliseconds this process has slept waiting for a quiet or unshared host (`wait_until`), whichever wait it was.
static QUIET_WAITED_MS: AtomicU64 = AtomicU64::new(0);

/// How long this process has waited for a quiet or unshared host so far: what an A/B result records (`waited_quiet_s`)
/// so that a caller (`ab-run`) can split a step's wall into the work and the waits its child spent.
pub fn quiet_waited() -> Duration {
    Duration::from_millis(QUIET_WAITED_MS.load(Ordering::Relaxed))
}

fn wait_until(
    mut probe: impl FnMut() -> Quiet,
    mut sleep: impl FnMut(Duration) -> Result<()>,
    timeout: Duration,
    force: bool,
    quiet: impl Fn(&Quiet) -> bool,
) -> Result<f64> {
    let step = Duration::from_secs(5);
    let mut waited = Duration::ZERO;
    let mut last_told = None::<Duration>;
    loop {
        let q = probe();
        if force || quiet(&q) {
            if waited > Duration::ZERO {
                eprintln!("quiet after {} s: load {:.1} on {} cores", waited.as_secs(), q.load, q.cores);
                QUIET_WAITED_MS.fetch_add(u64::try_from(waited.as_millis()).unwrap_or(u64::MAX), Ordering::Relaxed);
            }
            return Ok(q.load);
        }
        if waited >= timeout {
            let mut msg = format!("not quiet after {} s: load {:.1} on {} cores", waited.as_secs(), q.load, q.cores);
            if !q.busy.is_empty() {
                msg.push_str(", busy process(es):\n  ");
                msg.push_str(&q.busy.join("\n  "));
            }
            bail!("{msg}\nthe numbers would be noise: wait, stop them, or --force");
        }
        if last_told.is_none_or(|t| waited - t >= Duration::from_secs(60)) {
            eprintln!(
                "waiting for a quiet host: load {:.1} on {} cores{}",
                q.load,
                q.cores,
                if q.busy.is_empty() { String::new() } else { format!(", busy: {}", q.busy.join("; ")) }
            );
            last_told = Some(waited);
        }
        sleep(step)?;
        waited += step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_measurement_waits_for_the_first_and_names_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let first = Reservation::acquire_in(&dir, "speed on the community clone", Duration::ZERO).unwrap();
        assert!(std::fs::read_to_string(dir.join("measure.holder")).unwrap().contains("speed on the community clone"));
        let err = Reservation::acquire_in(&dir, "ab", Duration::ZERO).err().unwrap().to_string();
        assert!(err.contains("still held") && err.contains("speed on the community clone"), "{err}");
        let d2 = dir.clone();
        let waiter = std::thread::spawn(move || {
            let t = Instant::now();
            let r = Reservation::acquire_in(&d2, "ab", Duration::from_secs(30)).unwrap();
            (t.elapsed(), std::fs::read_to_string(d2.join("measure.holder")).unwrap(), r)
        });
        std::thread::sleep(Duration::from_millis(700));
        drop(first);
        let (waited, holder, _r) = waiter.join().unwrap();
        assert!(waited >= Duration::from_millis(500), "{waited:?}");
        assert!(holder.contains(": ab"), "{holder}");
    }

    #[test]
    fn a_loaded_host_is_waited_for_then_measured_or_refused() {
        let busy = || Quiet { load: 2.0, cores: 8, busy: vec!["99% pid 7 cargo".into()] };
        let quiet = Quiet { load: 2.0, cores: 8, busy: vec![] };
        let mut probes = vec![busy(), busy(), quiet.clone()].into_iter();
        let mut slept = 0;
        let before = quiet_waited();
        let load = wait_quiet_with(
            || probes.next().unwrap(),
            |_| {
                slept += 1;
                Ok(())
            },
            Duration::from_secs(600),
            false,
        )
        .unwrap();
        assert_eq!((load, slept), (2.0, 2));
        // The two 5 s waits count toward what this process waited (other tests may add theirs).
        assert!(quiet_waited() - before >= Duration::from_secs(10));
        let err = wait_quiet_with(busy, |_| Ok(()), Duration::from_secs(10), false).unwrap_err().to_string();
        assert!(err.contains("not quiet after 10 s") && err.contains("pid 7 cargo"), "{err}");
        assert!(wait_quiet_with(busy, |_| Ok(()), Duration::ZERO, true).is_ok());
        assert!(!Quiet { load: 6.1, cores: 8, busy: vec![] }.is_quiet());
        // Between the builds of a run the load is the run's own: only another busy process waits.
        let after_build = || Quiet { load: 30.0, cores: 8, busy: vec![] };
        assert_eq!(
            wait_until(after_build, |_| panic!("no wait"), Duration::ZERO, false, |q| q.busy.is_empty()).unwrap(),
            30.0
        );
        let mut probes = vec![busy(), after_build()].into_iter();
        let mut slept = 0;
        wait_until(
            || probes.next().unwrap(),
            |_| {
                slept += 1;
                Ok(())
            },
            Duration::from_secs(600),
            false,
            |q| q.busy.is_empty(),
        )
        .unwrap();
        assert_eq!(slept, 1);
    }
}
