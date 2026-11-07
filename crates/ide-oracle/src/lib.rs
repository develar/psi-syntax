//! An IDE's own code as the oracle: the parts every oracle of psi-syntax and accio runs on, so that a port is checked
//! against the code it ports and not against a reimplementation of it.
//!
//! - [`jars`]: the IDE's jars the oracle needs, pinned by a checksum file and copied to a cache, refused on a mismatch
//!   (an IDE update cannot change the oracle silently).
//! - [`jvm`]: the IDE's own JVM (its JBR, `$JAVA` overriding it) running a single-file Java oracle on those jars, its
//!   stderr to a side file, its stdout written by us so that a failed write fails the run.
//! - [`compare`]: byte-for-byte comparison of JSON lines (naming the first differing field) and of texts (naming the
//!   first differing line).
//! - [`goldens`]: a directory of vendored cases with its manifest, and [`git`]: a revision's files read through one
//!   `git cat-file --batch` (never the working tree) and a checkout's tracked files.
//! - [`random`] and [`fuzz`]: seeded, deterministic mutations; the generator is Python's `random.Random`, so a
//!   corpus first made by a Python script regenerates byte for byte.
//!
//! The oracle programs themselves stay Java: they must run the IDE's code. Their drivers are binaries of this crate
//! (`java-oracle`) or of accio's measurement crate (`scripts/bench plugin-xml`), both on this library.
pub mod compare;
pub mod fuzz;
pub mod git;
pub mod goldens;
pub mod jars;
pub mod jvm;
pub mod random;

use std::path::PathBuf;

/// `$HOME/.cache`, where the pinned jars and the corpora live.
pub fn cache_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".cache")
}

/// psi-syntax's root, this crate's `../..` (in accio and in the mirror alike): where an oracle's `tools/` and
/// `testdata/` are.
pub fn psi_syntax() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(std::path::Path::parent).unwrap_or(manifest).to_path_buf()
}
