//! The engine of an A/B of two binaries: `pairs` runs each unit once per arm per pair, the first arm alternating, until
//! `stats::decide` gives every deciding metric its verdict; `reserve` queues the measurement for the host and waits
//! for a quiet one; `sys` times a child by its own resource usage and hardware counters. accio's `scripts/bench`
//! (`ab`, `build-ab`, `lang-ab`, `ab-run`) builds its A/B on it; what an arm is prepared with and how a result is
//! filed stay with the command. No accio crate and no path of the accio repository: the psi-syntax mirror builds it.
pub mod pairs;
pub mod reserve;
pub mod stats;
#[allow(unsafe_code)] // the kernel's records of a child and of the host: wait4, proc_pid_rusage, perf_event_open
pub mod sys;
