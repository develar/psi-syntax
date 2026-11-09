//! Git as the oracles read it: a checkout's tracked files (the corpus lists), and a revision's tree and blobs read
//! through one `git cat-file --batch` (goldens are vendored from a pinned revision, never from a working tree).
use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn git(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo);
    cmd
}

fn nul_separated(out: Vec<u8>, what: &str) -> Result<Vec<String>> {
    out.split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8(p.to_vec()).map_err(|e| anyhow::anyhow!("{what}: a path that is not UTF-8: {}", String::from_utf8_lossy(e.as_bytes()))))
        .collect()
}

fn output(cmd: &mut Command, what: &str) -> Result<Vec<u8>> {
    let out = cmd.stderr(Stdio::inherit()).output().with_context(|| format!("{what}: starting git"))?;
    if !out.status.success() {
        bail!("{what} failed ({})", out.status);
    }
    Ok(out.stdout)
}

/// The tracked files of a checkout matching `pathspecs` (`*.java` matches in every directory), as `git ls-files`
/// lists them: paths relative to `checkout`, in the index's order, unquoted.
pub fn tracked(checkout: &Path, pathspecs: &[&str]) -> Result<Vec<String>> {
    let what = format!("git ls-files in {}", checkout.display());
    let out = output(git(checkout).args(["ls-files", "-z", "--"]).args(pathspecs), &what)?;
    nul_separated(out, &what)
}

/// A revision of a repository: its tree listed with `ls-tree`, its blobs read through one `cat-file --batch`.
pub struct Revision {
    repo: PathBuf,
    rev: String,
    batch: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Revision {
    pub fn open(repo: &Path, rev: &str) -> Result<Self> {
        let mut batch = git(repo)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("git cat-file in {}", repo.display()))?;
        let stdin = batch.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(batch.stdout.take().expect("piped stdout"));
        Ok(Revision { repo: repo.to_path_buf(), rev: rev.to_owned(), batch, stdin, stdout })
    }

    pub fn rev(&self) -> &str {
        &self.rev
    }

    /// The files under `path` at the revision, recursively, in the tree's order.
    pub fn ls(&self, path: &str) -> Result<Vec<String>> {
        let what = format!("git ls-tree {} {path} in {}", self.rev, self.repo.display());
        let out = output(git(&self.repo).args(["ls-tree", "-r", "-z", "--name-only", &self.rev, "--", path]), &what)?;
        nul_separated(out, &what)
    }

    /// The bytes of `path` at the revision; an error when the revision has no such file.
    pub fn show(&mut self, path: &str) -> Result<Vec<u8>> {
        if path.contains('\n') {
            bail!("{path:?}: a path with a line end cannot be asked of cat-file --batch");
        }
        writeln!(self.stdin, "{}:{path}", self.rev)?;
        self.stdin.flush()?;
        let mut header = String::new();
        self.stdout.read_line(&mut header)?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        match fields.as_slice() {
            [_, "missing"] => bail!("{path}: not in {} at {}", self.repo.display(), self.rev),
            [_, "blob", size] => {
                let size: usize = size.parse().with_context(|| format!("cat-file header {header:?}"))?;
                let mut data = vec![0; size];
                self.stdout.read_exact(&mut data)?;
                let mut lf = [0];
                self.stdout.read_exact(&mut lf)?;
                Ok(data)
            }
            _ => bail!("{path}: cat-file answered {header:?}, not a blob"),
        }
    }
}

impl Drop for Revision {
    fn drop(&mut self) {
        // Closing stdin ends cat-file; it is reaped here, not left a zombie.
        let _ = self.stdin.flush();
        self.batch.kill().ok();
        self.batch.wait().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_revision_is_read_from_its_tree_not_the_working_tree() {
        let repo = std::env::temp_dir().join(format!("ide-oracle-git-{}", std::process::id()));
        std::fs::create_dir_all(repo.join("d")).unwrap();
        std::fs::write(repo.join("d/a.java"), b"committed\n").unwrap();
        std::fs::write(repo.join("b.txt"), b"x").unwrap();
        let run = |args: &[&str]| assert!(git(&repo).args(args).output().unwrap().status.success(), "{args:?}");
        run(&["init", "-q"]);
        run(&["add", "."]);
        run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "--no-verify", "-m", "c"]);
        std::fs::write(repo.join("d/a.java"), b"edited\n").unwrap();
        assert_eq!(tracked(&repo, &["*.java"]).unwrap(), ["d/a.java"]);
        let mut rev = Revision::open(&repo, "HEAD").unwrap();
        assert_eq!(rev.ls("d").unwrap(), ["d/a.java"]);
        assert_eq!(rev.show("d/a.java").unwrap(), b"committed\n");
        assert_eq!(rev.show("b.txt").unwrap(), b"x");
        assert!(rev.show("nope").is_err());
        drop(rev);
        // Git for Windows' launcher is killed by the drop; the `cat-file` it started may still hold the directory.
        let _ = std::fs::remove_dir_all(&repo);
    }
}
