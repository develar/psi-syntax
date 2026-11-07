//! The IDE's JVM running a single-file Java oracle (`java -cp LIB/* Program.java ARGS`, the source launcher) on the
//! pinned jars of [`crate::jars`].
use anyhow::{Context, Result, bail};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The default IDE: the installed IntelliJ IDEA's `Contents` on macOS.
pub const DEFAULT_IDE: &str = "/Applications/Idea.app/Contents";

/// The `java` an oracle runs on: `$JAVA` when set, else the IDE's own runtime (its JBR: `jbr/Contents/Home` on macOS,
/// `jbr` elsewhere), the JVM the IDE runs its code on.
pub fn java(ide: &Path) -> PathBuf {
    if let Some(java) = std::env::var_os("JAVA") {
        return PathBuf::from(java);
    }
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let mac = ide.join("jbr/Contents/Home/bin").join(exe);
    if mac.exists() { mac } else { ide.join("jbr/bin").join(exe) }
}

/// A JVM with the oracle's class path and options.
#[derive(Debug, Clone)]
pub struct Jvm {
    java: PathBuf,
    classpath: OsString,
    options: Vec<OsString>,
}

/// Where a run's output goes: `None` inherits the stream.
#[derive(Debug, Clone, Copy, Default)]
pub struct Streams<'a> {
    /// The program's stdout, copied by us to this file: a write that fails (a full disk) fails the run, which the
    /// JVM's `System.out` would not report.
    pub stdout: Option<&'a Path>,
    /// The program's stderr (the exception of each input it rejects, its warnings).
    pub stderr: Option<&'a Path>,
}

impl Jvm {
    /// `java` with the class path `lib/*`, the jars [`crate::jars::fetch`] returned.
    pub fn new(java: PathBuf, lib: &Path) -> Self {
        Jvm { java, classpath: lib.join("*").into_os_string(), options: Vec::new() }
    }

    /// Adds one pinned oracle jar to the class path.
    pub fn with_classpath(mut self, path: &Path) -> Result<Self> {
        self.classpath = std::env::join_paths([self.classpath.as_os_str(), path.as_os_str()])?;
        Ok(self)
    }

    /// A JVM option (`-Xss1g`), before the program.
    pub fn option(mut self, option: impl Into<OsString>) -> Self {
        self.options.push(option.into());
        self
    }

    /// The command that runs `program` (a `.java` source file) with `args`, for a caller that drives its streams
    /// itself (`--serve`).
    pub fn command(&self, program: &Path, args: &[&OsStr]) -> Command {
        let mut cmd = Command::new(&self.java);
        cmd.args(&self.options).arg("-cp").arg(&self.classpath).arg(program).args(args);
        cmd
    }

    /// Runs `program` with `args` to its end; a nonzero exit is an error naming the stderr file.
    pub fn run(&self, program: &Path, args: &[&OsStr], streams: Streams) -> Result<()> {
        let mut cmd = self.command(program, args);
        if let Some(err) = streams.stderr {
            cmd.stderr(std::fs::File::create(err).with_context(|| format!("creating {}", err.display()))?);
        }
        let name = program.file_name().unwrap_or(program.as_os_str()).to_string_lossy().into_owned();
        let status = match streams.stdout {
            None => cmd.status().with_context(|| format!("starting {}", self.java.display()))?,
            Some(out) => {
                let mut child =
                    cmd.stdout(Stdio::piped()).spawn().with_context(|| format!("starting {}", self.java.display()))?;
                let copied = copy_to(child.stdout.take().expect("piped stdout"), out);
                if copied.is_err() {
                    child.kill().ok();
                }
                let status = child.wait()?;
                copied.with_context(|| format!("writing {}'s output to {}", name, out.display()))?;
                status
            }
        };
        if !status.success() {
            match streams.stderr {
                Some(err) => bail!("{name} failed ({status}; see {})", err.display()),
                None => bail!("{name} failed ({status})"),
            }
        }
        Ok(())
    }

    /// Runs an oracle over a list the way every oracle here takes one, `--list LIST --root ROOT --out OUT EXTRA`,
    /// its stderr to `err`.
    pub fn run_list(
        &self,
        program: &Path,
        list: &Path,
        root: &Path,
        out: &Path,
        extra: &[&str],
        err: &Path,
    ) -> Result<()> {
        let mut args: Vec<&OsStr> = vec![
            "--list".as_ref(),
            list.as_os_str(),
            "--root".as_ref(),
            root.as_os_str(),
            "--out".as_ref(),
            out.as_os_str(),
        ];
        args.extend(extra.iter().map(OsStr::new));
        self.run(program, &args, Streams { stdout: None, stderr: Some(err) })
    }
}

fn copy_to(mut from: impl std::io::Read, to: &Path) -> std::io::Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(to)?);
    std::io::copy(&mut from, &mut file)?;
    file.flush()?;
    file.into_inner().map_err(|e| e.into_error())?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_is_the_override_or_the_ides_runtime() {
        // `$JAVA` is the process's; only the IDE layout is checked here.
        if std::env::var_os("JAVA").is_none() {
            assert_eq!(
                java(Path::new("/nowhere")),
                Path::new("/nowhere/jbr/bin").join(if cfg!(windows) { "java.exe" } else { "java" })
            );
        }
    }

    #[test]
    fn an_oracle_jar_is_added_after_the_pinned_class_path() {
        let jvm = Jvm::new(PathBuf::from("java"), Path::new("lib")).with_classpath(Path::new("plugin.jar")).unwrap();
        assert_eq!(
            std::env::split_paths(&jvm.classpath).collect::<Vec<_>>(),
            [PathBuf::from("lib/*"), PathBuf::from("plugin.jar")]
        );
    }

    #[test]
    fn a_failed_write_fails_the_run() {
        // The parent of the output is a file: creating the output fails, and the run with it.
        let dir = std::env::temp_dir().join(format!("ide-oracle-jvm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("file");
        std::fs::write(&blocker, b"").unwrap();
        assert!(copy_to(&b"line\n"[..], &blocker.join("out")).is_err());
        copy_to(&b"line\n"[..], &dir.join("out")).unwrap();
        assert_eq!(std::fs::read(dir.join("out")).unwrap(), b"line\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
