//! A directory of vendored cases with its manifest: each case's files written under the directory, one manifest line
//! per case. The cases are rewritten whole (the directories they live in are emptied first), so a case the source no
//! longer has does not survive as a stale golden.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Goldens {
    root: PathBuf,
    manifest: Vec<String>,
}

/// The order of the manifest's lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// As recorded.
    Recorded,
    /// Sorted by their bytes.
    Sorted,
}

impl Goldens {
    /// The directory `root`, with each of `fresh` (directories relative to it that only cases fill) emptied.
    pub fn create(root: &Path, fresh: &[&str]) -> Result<Self> {
        for dir in fresh {
            let dir = root.join(dir);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).with_context(|| format!("emptying {}", dir.display()))?;
            }
        }
        std::fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
        Ok(Goldens { root: root.to_path_buf(), manifest: Vec::new() })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Writes `bytes` to `rel` under the directory, creating its parents.
    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<PathBuf> {
        let target = self.root.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&target, bytes).with_context(|| format!("writing {}", target.display()))?;
        Ok(target)
    }

    /// Copies `from` to `rel` under the directory.
    pub fn copy(&self, rel: &str, from: &Path) -> Result<PathBuf> {
        let bytes = std::fs::read(from).with_context(|| format!("reading {}", from.display()))?;
        self.write(rel, &bytes)
    }

    /// Records a manifest line (without its line end).
    pub fn record(&mut self, line: String) {
        self.manifest.push(line);
    }

    pub fn recorded(&self) -> &[String] {
        &self.manifest
    }

    /// Writes the manifest to `name` under the directory, each line ended by `\n`, and returns its path.
    pub fn finish(mut self, name: &str, order: Order) -> Result<PathBuf> {
        if order == Order::Sorted {
            self.manifest.sort();
        }
        let text: String = self.manifest.iter().map(|l| format!("{l}\n")).collect();
        self.write(name, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_are_rewritten_whole_with_a_sorted_manifest() {
        let dir = std::env::temp_dir().join(format!("ide-oracle-goldens-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("cases")).unwrap();
        std::fs::write(dir.join("cases/stale"), b"").unwrap();
        std::fs::write(dir.join("README.md"), b"kept").unwrap();
        let mut g = Goldens::create(&dir, &["cases"]).unwrap();
        g.write("cases/b/x.txt", b"x").unwrap();
        g.record("b".into());
        g.record("a".into());
        let manifest = g.finish("manifest.tsv", Order::Sorted).unwrap();
        assert_eq!(std::fs::read_to_string(manifest).unwrap(), "a\nb\n");
        assert!(!dir.join("cases/stale").exists());
        assert_eq!(std::fs::read(dir.join("README.md")).unwrap(), b"kept");
        assert_eq!(std::fs::read(dir.join("cases/b/x.txt")).unwrap(), b"x");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
