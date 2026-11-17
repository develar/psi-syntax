//! One Bazel BUILD file read without running Bazel: its statements as values (`statements`), the targets it declares
//! with the labels each attribute writes (`read`, `declared`), and its text without those labels (`remainder`), from
//! its bytes alone, over psi-syntax's Starlark tree. Nothing here reads another file, resolves a label to a package
//! directory or follows a loaded symbol into its `.bzl`: accio's Bazel reader does that over a tree
//! (`crates/bazel`), and Branch Deck's review compares two revisions of one file with it (decision
//! 2026-11-17-bazel-build-in-psi-syntax).
//!
//! Lines are 1-based: the line a target's call starts on.

pub mod label;
pub mod remainder;
pub mod statements;
pub mod written;

use std::collections::HashSet;

pub use remainder::remainder;
pub use statements::{Arg, Call, Expr, File, Load, parse};
pub use written::{Glob, Written, collect, declares_no_target, glob_of, not_a_dependency};

/// What a BUILD file declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildFile {
    /// The file may write a label the reader cannot see: a statement it could not read (a `def`, a syntax error), a
    /// call whose name is computed (a comprehension of rule calls, `File::partial`).
    pub partial: bool,
    /// Its `load` statements, in the order of the file.
    pub loads: Vec<Load>,
    /// Every top-level call that may declare a target, named or not, in the order of the file.
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// None for a call without a literal name: a macro (`content_module_jar(module = ":x")`) whose targets it names
    /// itself, kept for the labels it writes.
    pub name: Option<String>,
    /// The rule or macro as called (`jvm_library`, `jps_test`, `native.java_test`).
    pub kind: String,
    /// The 1-based line its call starts on.
    pub line: u32,
    /// A test rule (`*_test`) or a target only tests may use (`testonly = True`).
    pub test: bool,
    /// `module_name = "…"`: the JPS module the target is built from (IntelliJ's generated BUILD files).
    pub module_name: Option<String>,
    /// The first label it computes (`Label(x)`, a joined or formatted label), which no literal names: its labels
    /// may be more.
    pub unknown: Option<String>,
    /// The labels its attributes write, by attribute in the order of its arguments, each (attribute, label) once.
    pub labels: Vec<Dep>,
}

/// A label one attribute of a target writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dep {
    /// The attribute (`deps`, `runtime_deps`, `exports`, `associates`, `data`, …).
    pub attr: String,
    /// As written (`//platform/util`, `@lib//:kotlin-stdlib`, `:x`, a bare `x` that names a target of the file).
    pub label: String,
}

/// What the BUILD file `src` declares.
pub fn read(src: &[u8]) -> BuildFile {
    declared(&parse(src))
}

/// What the statements of a BUILD file declare: each call but `declares_no_target`'s with the labels of each
/// attribute but `not_a_dependency`'s and the unpacked `*args` and `**kwargs`. A variable of the file is read in its
/// place (`deps = DEPS + […]`), a loaded name is not followed, and a string that is not label-shaped is a label only
/// where the file declares a target of that name (`deps = ["media"]`; `srcs = ["A.kt"]` writes none). A label of a
/// source file (`data = ["//a:spec.md"]`) is kept: only a reader of the tree can tell it from a target.
pub fn declared(file: &File) -> BuildFile {
    let loads = file.loaded();
    let calls: Vec<&Call> = file.calls.iter().filter(|c| !declares_no_target(&c.func)).collect();
    let names: HashSet<&str> = calls.iter().filter_map(|c| c.kw_str("name")).collect();
    let mut targets = Vec::with_capacity(calls.len());
    for c in calls {
        let testonly = matches!(c.kw("testonly"), Some(Expr::Name(v)) if v == "True");
        let mut t = Target {
            name: c.kw_str("name").map(str::to_owned),
            kind: c.func.clone(),
            line: c.line,
            test: c.func.ends_with("_test") || testonly,
            module_name: c.kw_str("module_name").map(str::to_owned),
            unknown: None,
            labels: Vec::new(),
        };
        for a in &c.args {
            let Some(attr) = a.name.as_deref().filter(|n| !not_a_dependency(n) && !n.starts_with('*')) else {
                continue;
            };
            let mut seen = Written::at(c.line);
            collect(&a.value, file, &loads, &mut seen, &mut HashSet::new());
            t.unknown = t.unknown.take().or(seen.building);
            let bare = seen.bare.into_iter().filter(|b| names.contains(b.as_str()));
            for label in seen.labels.into_iter().chain(bare) {
                if !t.labels.iter().any(|d| d.attr == attr && d.label == label) {
                    t.labels.push(Dep { attr: attr.to_owned(), label });
                }
            }
        }
        targets.push(t);
    }
    BuildFile { partial: file.partial(), loads: file.loads.clone(), targets }
}

#[cfg(test)]
mod tests;
