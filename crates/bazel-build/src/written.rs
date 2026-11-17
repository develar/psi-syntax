//! What the values of a call write: every label-shaped string, every other string (a label only where it names a
//! target of the package, which the caller decides), the loaded names it uses, the globs it lists, and a label it
//! computes. A variable of the file is read in its place; a loaded name is kept, not followed (its definition is in
//! another file). Bazel defines which attributes are no dependency (`not_a_dependency`); every other attribute may
//! write one, which over-approximates the attributes a rule reads and needs no list of them.
use std::collections::{HashMap, HashSet};

use crate::label;
use crate::statements::{Call, Expr, File};

/// The attributes Bazel defines as no dependency (bazel.build/reference/be/common-definitions).
pub fn not_a_dependency(attr: &str) -> bool {
    matches!(attr, "name" | "visibility" | "tags" | "licenses" | "default_visibility")
        || attr.ends_with("_compatible_with")
}

/// The calls of a BUILD file that declare no target.
pub fn declares_no_target(func: &str) -> bool {
    matches!(func, "package" | "exports_files" | "licenses" | "load")
}

/// What the values of one call write, as `collect` reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    /// The line of the call (1-based), which a computed label's reason names.
    pub line: u32,
    /// Label-shaped strings (`//…`, `@…`, `:…`, `label::shaped`).
    pub labels: Vec<String>,
    /// Other strings: a label only where they name a target of the package.
    pub bare: Vec<String>,
    /// The loaded names it uses, whose definitions may write more.
    pub loaded: Vec<String>,
    /// The first label it computes (`Label(x)`, a joined or formatted label), which no literal names.
    pub building: Option<String>,
    /// The globs it lists: patterns, not labels.
    pub globs: Vec<Glob>,
}

impl Written {
    /// Nothing written yet by the call at `line`.
    pub fn at(line: u32) -> Written {
        Written { line, ..Written::default() }
    }
}

/// `glob(include, exclude = …)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Glob {
    /// An exclude the reader cannot read is no exclude, which only adds files.
    Patterns { include: Vec<String>, exclude: Vec<String> },
    /// The include is not a list of strings: any file of the package.
    Unread,
}

/// The strings, names and computed labels of `e` into `out`; a variable of the file is read in its place, once
/// (`seen`), a name `loads` binds (`File::loaded`) kept in `out.loaded`.
pub fn collect(
    e: &Expr,
    file: &File,
    loads: &HashMap<String, (String, String)>,
    out: &mut Written,
    seen: &mut HashSet<String>,
) {
    match e {
        Expr::Str(s) => {
            if label::shaped(s) {
                out.labels.push(s.clone());
            } else {
                out.bare.push(s.clone());
            }
        }
        Expr::Name(n) => {
            if let Some(v) = file.assigns.get(n) {
                if seen.insert(n.clone()) {
                    collect(v, file, loads, out, seen);
                }
            } else if loads.contains_key(n) {
                out.loaded.push(n.clone());
            }
        }
        Expr::Call(c) => {
            if c.func == "glob" || c.func == "native.glob" {
                // Patterns, not labels.
                out.globs.push(glob_of(c));
                return;
            }
            if c.func == "Label" && !matches!(c.pos(0), Some(Expr::Str(_))) && out.building.is_none() {
                out.building = Some(format!("Label() of a value at line {}", c.line));
            }
            if loads.contains_key(&c.func) {
                out.loaded.push(c.func.clone());
            }
            for a in &c.args {
                collect(&a.value, file, loads, out, seen);
            }
        }
        Expr::Bin(op, a, b) => {
            let shaped = |e: &Expr| match e {
                Expr::Str(s) => label::shaped(s),
                Expr::Name(n) => matches!(file.assigns.get(n), Some(Expr::Str(s)) if label::shaped(s)),
                _ => false,
            };
            let literal = |e: &Expr| matches!(e, Expr::Str(_));
            if matches!(*op, "+" | "%")
                && (shaped(a) && !literal(b) || shaped(b) && !literal(a))
                && out.building.is_none()
            {
                out.building = Some(format!("a label joined at line {}", out.line));
            }
            collect(a, file, loads, out, seen);
            collect(b, file, loads, out, seen);
        }
        Expr::Method(recv, m, args) => {
            if m == "format" && matches!(recv.as_ref(), Expr::Str(s) if label::shaped(s)) && out.building.is_none() {
                out.building = Some(format!("a label formatted at line {}", out.line));
            }
            collect(recv, file, loads, out, seen);
            for a in args {
                collect(&a.value, file, loads, out, seen);
            }
        }
        Expr::List(v) | Expr::Other(v) => v.iter().for_each(|e| collect(e, file, loads, out, seen)),
        Expr::Dict(v) => v.iter().for_each(|(k, e)| {
            collect(k, file, loads, out, seen);
            collect(e, file, loads, out, seen);
        }),
        Expr::Attr(r, _) => collect(r, file, loads, out, seen),
        Expr::Cond(a, b) => {
            collect(a, file, loads, out, seen);
            collect(b, file, loads, out, seen);
        }
    }
}

/// The patterns of a `glob(…)` call.
pub fn glob_of(c: &Call) -> Glob {
    let strings = |e: Option<&Expr>| -> Option<Vec<String>> {
        match e? {
            Expr::List(v) => v.iter().map(|e| if let Expr::Str(s) = e { Some(s.clone()) } else { None }).collect(),
            Expr::Str(s) => Some(vec![s.clone()]),
            _ => None,
        }
    };
    let include = strings(c.kw("include").or_else(|| c.pos(0)));
    let exclude = strings(c.kw("exclude").or_else(|| c.pos(1))).unwrap_or_default();
    match include {
        Some(include) => Glob::Patterns { include, exclude },
        None => Glob::Unread,
    }
}
