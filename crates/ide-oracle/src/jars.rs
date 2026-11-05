//! The jars an oracle runs on, pinned: a checksum file of `DIGEST  PATH` lines (`shasum` output, PATH relative to the
//! IDE's `Contents`), each jar copied once to a cache directory and checked there. A digest of 40 hex digits is SHA-1,
//! of 64 SHA-256. A jar whose digest differs means another IDE build, whose oracle would be another program, so the
//! fetch refuses; a cached copy that differs is replaced only by an IDE jar that matches.
use anyhow::{Context, Result, bail};
use sha2::Digest as _;
use std::io::Read;
use std::path::{Path, PathBuf};

/// One line of a checksum file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub digest: String,
    /// The jar's path in the IDE, relative to its `Contents`.
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
}

impl Algorithm {
    fn of(digest: &str) -> Option<Self> {
        if !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        match digest.len() {
            40 => Some(Algorithm::Sha1),
            64 => Some(Algorithm::Sha256),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha256 => "sha256",
        }
    }
}

/// The pins of a checksum file, in its order.
pub fn pins(sums: &Path) -> Result<Vec<Pin>> {
    let text = std::fs::read_to_string(sums).with_context(|| format!("reading {}", sums.display()))?;
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (digest, path) = line.split_once("  ").with_context(|| format!("{}: `{line}` is not `DIGEST  PATH`", sums.display()))?;
            if Algorithm::of(digest).is_none() {
                bail!("{}: `{digest}` is neither a SHA-1 nor a SHA-256 digest", sums.display());
            }
            Ok(Pin { digest: digest.to_ascii_lowercase(), path: path.to_owned() })
        })
        .collect()
}

/// The hex digest of a file with the algorithm of `like` (a digest of the same kind).
pub fn digest_of(file: &Path, like: &str) -> Result<String> {
    let algorithm = Algorithm::of(like).with_context(|| format!("`{like}` is not a digest"))?;
    let mut f = std::fs::File::open(file).with_context(|| format!("reading {}", file.display()))?;
    let mut buf = vec![0; 1 << 20];
    let mut sha1 = sha1_smol::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    loop {
        let n = f.read(&mut buf).with_context(|| format!("reading {}", file.display()))?;
        if n == 0 {
            break;
        }
        match algorithm {
            Algorithm::Sha1 => sha1.update(&buf[..n]),
            Algorithm::Sha256 => sha256.update(&buf[..n]),
        }
    }
    Ok(match algorithm {
        Algorithm::Sha1 => sha1.digest().to_string(),
        Algorithm::Sha256 => sha256.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    })
}

/// Copies the pinned jars of `sums` from the IDE at `ide` (its `Contents`) to `lib` where they are missing, checks
/// every cached copy and returns `lib`, the directory to put on the class path as `lib/*`.
pub fn fetch(sums: &Path, ide: &Path, lib: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(lib).with_context(|| format!("creating {}", lib.display()))?;
    for pin in pins(sums)? {
        let name = Path::new(&pin.path).file_name().with_context(|| format!("{}: no file name", pin.path))?;
        let cached = lib.join(name);
        let source = ide.join(&pin.path);
        if cached.exists() && digest_of(&cached, &pin.digest)? == pin.digest {
            continue;
        }
        if !source.exists() {
            bail!("{} is not in {} (pass the IDE whose jars {} pins)", pin.path, ide.display(), sums.display());
        }
        // Copied beside the target, checked, then renamed: the cache holds matching jars only, never a short one.
        let partial = lib.join(format!(".{}.partial", name.to_string_lossy()));
        std::fs::copy(&source, &partial).with_context(|| format!("copying {} to {}", source.display(), partial.display()))?;
        let actual = digest_of(&partial, &pin.digest)?;
        if actual != pin.digest {
            std::fs::remove_file(&partial).ok();
            let algorithm = Algorithm::of(&pin.digest).map_or("digest", Algorithm::name);
            bail!(
                "{}: {algorithm} {actual}, expected {} ({}): the IDE's build changed, refusing to run another oracle",
                source.display(),
                pin.digest,
                sums.display()
            );
        }
        std::fs::rename(&partial, &cached)?;
    }
    Ok(lib.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_name_their_algorithm_by_length() {
        let dir = std::env::temp_dir().join(format!("ide-oracle-jars-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("abc");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(digest_of(&file, &"0".repeat(40)).unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(digest_of(&file, &"0".repeat(64)).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(digest_of(&file, "abc").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fetch_copies_checks_and_refuses_another_build() {
        let dir = std::env::temp_dir().join(format!("ide-oracle-fetch-{}", std::process::id()));
        let ide = dir.join("ide");
        std::fs::create_dir_all(ide.join("lib")).unwrap();
        std::fs::write(ide.join("lib/a.jar"), b"abc").unwrap();
        let sums = dir.join("jars.sha1");
        std::fs::write(&sums, "a9993e364706816aba3e25717850c26c9cd0d89d  lib/a.jar\n").unwrap();
        let lib = dir.join("cache");
        assert_eq!(fetch(&sums, &ide, &lib).unwrap(), lib);
        assert_eq!(std::fs::read(lib.join("a.jar")).unwrap(), b"abc");
        // A stale cached copy is replaced by the IDE's matching jar.
        std::fs::write(lib.join("a.jar"), b"old").unwrap();
        fetch(&sums, &ide, &lib).unwrap();
        assert_eq!(std::fs::read(lib.join("a.jar")).unwrap(), b"abc");
        // Another build: refused.
        std::fs::write(ide.join("lib/a.jar"), b"abd").unwrap();
        std::fs::remove_file(lib.join("a.jar")).unwrap();
        let err = fetch(&sums, &ide, &lib).unwrap_err().to_string();
        assert!(err.contains("refusing"), "{err}");
        assert_eq!(std::fs::read_dir(&lib).unwrap().count(), 0, "a jar of another build never enters the cache");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
