//! A TypeScript project's configuration (`tsconfig.json`, `jsconfig.json`) as tsc reads it for module resolution: the
//! file's own fields from its text (`parse`: JSON with comments and trailing commas, as tsc's `parseJsonText` accepts
//! it), the options its `extends` chain gives it (`aliases`), the projects it references (`project_aliases`) and the
//! module paths a non-relative import specifier names through `compilerOptions.paths` and `baseUrl`
//! (`Aliases::candidates`). A file format of the language, read for every consumer alike (decision
//! 2026-11-13-tsconfig-read-by-js-syntax of accio); which configuration applies to a file and which candidate names a
//! file of the tree is the consumer's: this module reads no directory.
//!
//! Paths are repository-relative with `/` between segments, the root `""`; a path that leaves the root (`../..` above
//! it, an absolute one) names nothing a tree holds and is left out. tsc's rules, each from its `moduleNameResolver.ts`
//! and `commandLineParser.ts`:
//! - `baseUrl` is relative to the configuration that writes it; `paths` relative to the effective `baseUrl`, else to the
//!   configuration that writes `paths` (an inherited `paths` keeps its own directory); `${configDir}` at the start of
//!   either is the directory of the configuration being read, not of the one that writes it.
//! - `extends` (a string or, since TypeScript 5.0, an array, applied in order) names a configuration whose options the
//!   file's own override one by one; a relative name without `.json` that names no file is read with it. A package
//!   name (`@tsconfig/node20/tsconfig.json`) is resolved by tsc from `node_modules`, which this module does not read.
//! - A pattern has at most one `*`; a specifier equal to a pattern without one takes it, else the pattern with a `*`
//!   whose prefix and suffix it has and whose prefix is the longest (the first such one written). Each of its targets,
//!   the `*` replaced by what it matched, is a candidate in order; then the specifier under `baseUrl`. A relative
//!   specifier (`./x`, `../x`, `/x`) is read by neither.

/// The names of a project configuration, in the order tsc looks for them in a directory (`tsconfig.json` first;
/// `jsconfig.json` where no `tsconfig.json` is).
pub const CONFIG_FILES: [&str; 2] = ["tsconfig.json", "jsconfig.json"];

/// The deepest `extends` chain followed: tsc reports a cycle, this module stops at it or at this depth.
const MAX_EXTENDS: usize = 16;

/// How deep the values of a configuration nest before it is no configuration (`parse`): a bound, not tsc's.
const MAX_DEPTH: usize = 64;

/// `compilerOptions.paths`: each pattern with its targets, in the order written.
pub type Patterns = Vec<(String, Vec<String>)>;

/// One configuration file's own fields that module resolution reads, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// `extends`, a string or an array of them.
    pub extends: Vec<String>,
    /// `compilerOptions.baseUrl`.
    pub base_url: Option<String>,
    /// `compilerOptions.paths`: pattern -> targets, in the order written; None when the file writes no `paths`.
    pub paths: Option<Patterns>,
    /// The `path` of each entry of `references`: a configuration file, or a directory holding a `tsconfig.json`.
    pub references: Vec<String>,
}

/// The path aliases of one configuration with its `extends` chain applied: what `paths` and `baseUrl` give an import
/// specifier, every path from the repository root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aliases {
    /// The effective `baseUrl`, a directory.
    pub base_url: Option<String>,
    /// The effective `paths`: pattern -> targets, each target from the root with its `*` kept.
    pub paths: Patterns,
}

impl Aliases {
    /// Whether neither `paths` nor `baseUrl` gives a specifier anything.
    pub fn is_empty(&self) -> bool {
        self.base_url.is_none() && self.paths.is_empty()
    }

    /// The module paths a non-relative import specifier names, in the order tsc tries them (see the module's rules):
    /// the targets of its pattern, then the specifier under `baseUrl`. A path is as the target writes it, an extension
    /// kept (`src/lib/x`, `src/lib/x.ts`); empty for a relative specifier and for one nothing here matches.
    pub fn candidates(&self, specifier: &str) -> Vec<String> {
        if specifier.is_empty() || specifier.starts_with('.') || specifier.starts_with('/') {
            return Vec::new();
        }
        let mut out: Vec<String> = Vec::new();
        if let Some((matched, targets)) = self.best_pattern(specifier) {
            for target in targets {
                if let Some(path) = normalize(&target.replacen('*', matched, 1)) {
                    out.push(path);
                }
            }
        }
        if let Some(base) = &self.base_url
            && let Some(path) = join(base, specifier)
            && !out.contains(&path)
        {
            out.push(path);
        }
        out
    }

    /// The pattern `specifier` takes and what its `*` matched (empty for an exact one), with the pattern's targets.
    fn best_pattern<'a>(&'a self, specifier: &'a str) -> Option<(&'a str, &'a [String])> {
        if let Some((_, targets)) =
            self.paths.iter().find(|(pattern, _)| !pattern.contains('*') && pattern == specifier)
        {
            return Some(("", targets));
        }
        let mut best: Option<(usize, &str, &[String])> = None;
        for (pattern, targets) in &self.paths {
            let Some((prefix, suffix)) = pattern.split_once('*') else { continue };
            if suffix.contains('*') || specifier.len() < prefix.len() + suffix.len() {
                continue;
            }
            let Some(matched) = specifier.strip_prefix(prefix).and_then(|rest| rest.strip_suffix(suffix)) else {
                continue;
            };
            if best.is_none_or(|(len, _, _)| prefix.len() > len) {
                best = Some((prefix.len(), matched, targets));
            }
        }
        best.map(|(_, matched, targets)| (matched, targets))
    }
}

/// The fields of the configuration `text`; None for a text that is no JSON object (comments and trailing commas
/// allowed, a string in single quotes too, as tsc reads one).
pub fn parse(text: &[u8]) -> Option<Config> {
    let text = text.strip_prefix(b"\xef\xbb\xbf").unwrap_or(text);
    let mut reader = Reader { s: text, i: 0 };
    let root = reader.value(0)?;
    reader.skip_trivia();
    if reader.i != text.len() {
        return None;
    }
    let Value::Object(_) = root else { return None };
    let strings = |v: Option<&Value>| -> Vec<String> {
        match v {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => Vec::new(),
        }
    };
    let options = root.get("compilerOptions");
    let paths = match options.and_then(|o| o.get("paths")) {
        Some(Value::Object(entries)) => Some(
            entries
                .iter()
                .map(|(pattern, targets)| {
                    let targets = match targets {
                        Value::Array(items) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                        _ => Vec::new(),
                    };
                    (pattern.clone(), targets)
                })
                .collect(),
        ),
        _ => None,
    };
    let references = match root.get("references") {
        Some(Value::Array(items)) => {
            items.iter().filter_map(|r| r.get("path").and_then(Value::as_str)).map(str::to_string).collect()
        }
        _ => Vec::new(),
    };
    Some(Config {
        extends: strings(root.get("extends")),
        base_url: options.and_then(|o| o.get("baseUrl")).and_then(Value::as_str).map(str::to_string),
        paths,
        references,
    })
}

/// The aliases of the configuration at `path` with its `extends` chain (see the module's rules); `read` gives a file's
/// bytes, None for a file the tree lacks. None when `path` itself cannot be read or parsed; a configuration it extends
/// that cannot is left out, as a cycle is.
pub fn aliases(path: &str, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Option<Aliases> {
    let config = parse(&read(path)?)?;
    Some(aliases_of(path, &config, read))
}

/// The aliases of each project the configuration at `path` stands for: its own when it has any, then those of each
/// configuration its `references` name that has any (one level, as a solution-style `tsconfig.json` with `"files": []`
/// leaves its files to them), in that order. Empty when none has any.
pub fn project_aliases(path: &str, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Vec<Aliases> {
    let Some(config) = read(path).and_then(|text| parse(&text)) else { return Vec::new() };
    let dir = dir_of(path);
    let mut out = vec![aliases_of(path, &config, read)];
    for reference in &config.references {
        let Some(at) = join(dir, reference) else { continue };
        let at = if at.ends_with(".json") { at } else { join_name(&at, CONFIG_FILES[0]) };
        if at != path
            && let Some(more) = aliases(&at, read)
            && !out.contains(&more)
        {
            out.push(more);
        }
    }
    out.retain(|a| !a.is_empty());
    out
}

/// `baseUrl` and `paths` as a chain left them: each with the directory of the configuration that wrote it.
#[derive(Default)]
struct Layer {
    base_url: Option<(String, String)>,
    paths: Option<(String, Patterns)>,
}

impl Layer {
    /// `top` over `self`: each option `top` writes replaces this one's.
    fn under(&mut self, top: Layer) {
        if top.base_url.is_some() {
            self.base_url = top.base_url;
        }
        if top.paths.is_some() {
            self.paths = top.paths;
        }
    }
}

fn aliases_of(path: &str, config: &Config, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Aliases {
    let mut seen = vec![path.to_string()];
    let layer = chain(path, config, read, &mut seen);
    let config_dir = dir_of(path);
    // `${configDir}` is the directory of the configuration read, whichever of its chain writes it.
    let resolve = |dir: &str, value: &str| match value.strip_prefix("${configDir}") {
        Some(rest) => join(config_dir, rest.trim_start_matches('/')),
        None => join(dir, value),
    };
    let base_url = layer.base_url.and_then(|(dir, value)| resolve(&dir, &value));
    let paths = layer.paths.map_or_else(Vec::new, |(dir, patterns)| {
        let base = base_url.clone().unwrap_or(dir);
        patterns
            .into_iter()
            .filter(|(pattern, _)| pattern.matches('*').count() <= 1)
            .map(|(pattern, targets)| {
                let targets =
                    targets.iter().filter(|t| t.matches('*').count() <= 1).filter_map(|t| resolve(&base, t)).collect();
                (pattern, targets)
            })
            .collect()
    });
    Aliases { base_url, paths }
}

/// The options of `config` (at `path`) over those of the configurations it extends, in order.
fn chain(path: &str, config: &Config, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>, seen: &mut Vec<String>) -> Layer {
    let dir = dir_of(path);
    let mut layer = Layer::default();
    for name in &config.extends {
        if seen.len() >= MAX_EXTENDS {
            break;
        }
        // A package's configuration lives in `node_modules`, which tsc resolves and this module does not read.
        if !(name.starts_with("./") || name.starts_with("../")) {
            continue;
        }
        let Some(at) = join(dir, name) else { continue };
        let mut found = read(&at).map(|text| (at.clone(), text));
        if found.is_none() && !at.ends_with(".json") {
            let with = format!("{at}.json");
            found = read(&with).map(|text| (with, text));
        }
        let Some((at, text)) = found else { continue };
        if seen.contains(&at) {
            continue;
        }
        let Some(base) = parse(&text) else { continue };
        seen.push(at.clone());
        layer.under(chain(&at, &base, read, seen));
    }
    layer.under(Layer {
        base_url: config.base_url.clone().map(|value| (dir.to_string(), value)),
        paths: config.paths.clone().map(|paths| (dir.to_string(), paths)),
    });
    layer
}

/// The directory of a repository-relative path, `""` for the root's files.
fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// `dir/name` for a directory and a plain name.
fn join_name(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

/// `rel` read from directory `dir`, normalized (`normalize`); None for an absolute `rel` (`/x`, `C:/x`), which names
/// no path of the tree.
fn join(dir: &str, rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    let bytes = rel.as_bytes();
    if rel.starts_with('/') || (bytes.len() > 1 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()) {
        return None;
    }
    normalize(&join_name(dir, &rel))
}

/// `path` without `.` and empty segments, each `..` taking the segment before it away; None when a `..` leaves the
/// root.
fn normalize(path: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

/// A JSON value as a configuration holds it: what module resolution reads of one, the rest `Other`.
enum Value {
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
    String(String),
    Other,
}

impl Value {
    /// The value of `key` in an object, the last one written (as `JSON.parse` and tsc keep it).
    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

/// The reader of `parse`: one pass over the bytes, comments skipped as whitespace.
struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    /// Whitespace and comments (`// …`, `/* … */`).
    fn skip_trivia(&mut self) {
        while let Some(b) = self.peek() {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                b'/' if self.s.get(self.i + 1) == Some(&b'/') => {
                    self.i = memchr::memchr(b'\n', &self.s[self.i..]).map_or(self.s.len(), |n| self.i + n);
                }
                b'/' if self.s.get(self.i + 1) == Some(&b'*') => {
                    self.i =
                        memchr::memmem::find(&self.s[self.i + 2..], b"*/").map_or(self.s.len(), |n| self.i + n + 4);
                }
                _ => return,
            }
        }
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_trivia();
        match self.peek()? {
            b'{' => {
                self.i += 1;
                let mut entries = Vec::new();
                loop {
                    self.skip_trivia();
                    match self.peek()? {
                        b'}' => {
                            self.i += 1;
                            return Some(Value::Object(entries));
                        }
                        b'"' | b'\'' => {}
                        _ => return None,
                    }
                    let key = self.string()?;
                    self.skip_trivia();
                    if self.peek()? != b':' {
                        return None;
                    }
                    self.i += 1;
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                    if !self.separator(b'}')? {
                        self.i += 1;
                        return Some(Value::Object(entries));
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_trivia();
                    if self.peek()? == b']' {
                        self.i += 1;
                        return Some(Value::Array(items));
                    }
                    items.push(self.value(depth + 1)?);
                    if !self.separator(b']')? {
                        self.i += 1;
                        return Some(Value::Array(items));
                    }
                }
            }
            b'"' | b'\'' => self.string().map(Value::String),
            _ => {
                // A number, `true`, `false` or `null`: what no option of module resolution reads.
                let start = self.i;
                while self.peek().is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.')) {
                    self.i += 1;
                }
                (self.i > start).then_some(Value::Other)
            }
        }
    }

    /// After an entry: true at a `,` (consumed), false at `close` (not consumed); None at anything else.
    fn separator(&mut self, close: u8) -> Option<bool> {
        self.skip_trivia();
        match self.peek()? {
            b',' => {
                self.i += 1;
                Some(true)
            }
            b if b == close => Some(false),
            _ => None,
        }
    }

    /// A string in double or single quotes with JSON's escapes; a byte sequence that is no UTF-8 is read lossily.
    fn string(&mut self) -> Option<String> {
        let quote = self.peek()?;
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let b = self.peek()?;
            self.i += 1;
            match b {
                b'\n' => return None,
                b if b == quote => return Some(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    let c = match e {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => self.unicode_escape()?,
                        other => {
                            out.push(other);
                            continue;
                        }
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                b => out.push(b),
            }
        }
    }

    /// The character of `\uXXXX` after its `\u`, a surrogate pair read as one; an unpaired surrogate is U+FFFD.
    fn unicode_escape(&mut self) -> Option<char> {
        let unit = self.hex4()?;
        if (0xD800..0xDC00).contains(&unit) && self.s.get(self.i..self.i + 2) == Some(b"\\u") {
            let at = self.i;
            self.i += 2;
            let low = self.hex4()?;
            if (0xDC00..0xE000).contains(&low) {
                return char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00));
            }
            self.i = at;
        }
        Some(char::from_u32(unit).unwrap_or('\u{fffd}'))
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.s.get(self.i..self.i + 4)?;
        let unit = u32::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()?;
        self.i += 4;
        Some(unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn files(entries: &[(&str, &str)]) -> HashMap<String, Vec<u8>> {
        entries.iter().map(|(p, t)| (p.to_string(), t.as_bytes().to_vec())).collect()
    }

    fn reader(files: &HashMap<String, Vec<u8>>) -> impl FnMut(&str) -> Option<Vec<u8>> + '_ {
        |path| files.get(path).cloned()
    }

    fn pairs(entries: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        entries.iter().map(|(p, t)| (p.to_string(), t.iter().map(|s| s.to_string()).collect())).collect()
    }

    /// Comments, trailing commas and single quotes, as tsc accepts them; a comma or a comment inside a string stays.
    #[test]
    fn a_config_is_json_with_comments_and_trailing_commas() {
        let text = "\u{feff}{\n  // the alias of the app\n  \"compilerOptions\": {\n    \"baseUrl\": \".\", /* the root */\n    \"paths\": { \"@/*\": [\"./src/*\",], '~,}': ['lib/x,} // y'] },\n    \"strict\": true, \"target\": 7.5e1, \"lib\": null,\n  },\n  \"extends\": [\"./a.json\", \"./b\"],\n  \"references\": [{ \"path\": \"./tsconfig.app.json\" }, { \"path\": \"pkg\" },],\n}\n";
        assert_eq!(
            parse(text.as_bytes()),
            Some(Config {
                extends: vec!["./a.json".into(), "./b".into()],
                base_url: Some(".".into()),
                paths: Some(pairs(&[("@/*", &["./src/*"]), ("~,}", &["lib/x,} // y"])])),
                references: vec!["./tsconfig.app.json".into(), "pkg".into()],
            })
        );
        assert_eq!(
            parse(br#"{"extends": "../base.json", "compilerOptions": {"baseUrl": "\u0073rc\\x"}}"#)
                .map(|c| (c.extends, c.base_url)),
            Some((vec!["../base.json".into()], Some("src\\x".into())))
        );
        // No object, a value cut short, text after the object: no configuration; nesting past the bound too.
        for bad in ["[]", "{\"a\": [1, 2}", "{} x", "", "{\"a\" 1}", &format!("{}{}", "[".repeat(100), "]".repeat(100))]
        {
            assert_eq!(parse(bad.as_bytes()), None, "{bad}");
        }
        assert_eq!(parse(b"{}"), Some(Config::default()));
        // Any bytes read without a panic: every prefix of the text, and the text with each byte replaced by each of
        // the bytes that open or end something.
        let bytes = text.as_bytes();
        for end in 0..bytes.len() {
            parse(&bytes[..end]);
        }
        for at in 0..bytes.len() {
            for b in [b'"', b'\'', b'\\', b'/', b'*', b'{', b'[', b'}', b']', b',', b':', 0xff] {
                let mut mutated = bytes.to_vec();
                mutated[at] = b;
                parse(&mutated);
            }
        }
        assert_eq!(parse(b"{\"a\": \"\\ud83d\\ude00\\ud800x\"}"), Some(Config::default()));
    }

    /// `paths` relative to `baseUrl`, else to the configuration that writes them; exact before a `*`, the longest prefix
    /// first; then the specifier under `baseUrl`; a relative specifier takes nothing.
    #[test]
    fn a_specifier_takes_its_best_pattern_then_the_base_url() {
        let tree = files(&[
            (
                "web/tsconfig.json",
                r#"{"compilerOptions": {"paths": {"@/*": ["./src/*", "./gen/*"], "@/ui/*": ["../ui/*"], "env": ["./src/env.ts"], "@/*/x": ["./bad/*"], "a*b*": ["./c"]}}}"#,
            ),
            ("app/tsconfig.json", r#"{"compilerOptions": {"baseUrl": "src", "paths": {"$/*": ["lib/*"]}}}"#),
        ]);
        let web = aliases("web/tsconfig.json", &mut reader(&tree)).unwrap();
        assert_eq!(web.base_url, None);
        assert_eq!(web.candidates("@/lib/diff/find"), ["web/src/lib/diff/find", "web/gen/lib/diff/find"]);
        assert_eq!(web.candidates("@/ui/button"), ["ui/button"]);
        assert_eq!(web.candidates("env"), ["web/src/env.ts"]);
        // `@/*` and `@/*/x` have one prefix: the first written.
        assert_eq!(web.candidates("@/a/x"), ["web/src/a/x", "web/gen/a/x"]);
        for none in ["react", "./x", "../x", "/abs", "", "ab"] {
            assert!(web.candidates(none).is_empty(), "{none}");
        }
        let app = aliases("app/tsconfig.json", &mut reader(&tree)).unwrap();
        assert_eq!(app.candidates("$/x"), ["app/src/lib/x", "app/src/$/x"]);
        assert_eq!(app.candidates("components/Foo"), ["app/src/components/Foo"]);
    }

    /// `extends` in order, the file's own options last; an inherited `paths` keeps its directory unless a `baseUrl`
    /// is set; `${configDir}` is the directory of the configuration read; a package, a cycle and a missing file are
    /// left out.
    #[test]
    fn extends_applies_its_chain_in_order() {
        let tree = files(&[
            ("tsconfig.base.json", r#"{"compilerOptions": {"paths": {"@shared/*": ["packages/shared/src/*"]}}}"#),
            (
                "packages/a/tsconfig.json",
                r#"{"extends": ["@tsconfig/node20", "../../tsconfig.base", "./missing.json"]}"#,
            ),
            (
                "packages/b/tsconfig.json",
                r#"{"extends": "../../tsconfig.base.json", "compilerOptions": {"baseUrl": "."}}"#,
            ),
            ("packages/c/tsconfig.json", r#"{"extends": "../../templates/tsconfig.json"}"#),
            ("templates/tsconfig.json", r#"{"compilerOptions": {"paths": {"~/*": ["${configDir}/src/*"]}}}"#),
            ("loop/a.json", r#"{"extends": "./b.json", "compilerOptions": {"baseUrl": "a"}}"#),
            ("loop/b.json", r#"{"extends": "./a.json", "compilerOptions": {"baseUrl": "b"}}"#),
            (
                "over/tsconfig.json",
                r#"{"extends": ["../loop/b.json", "../tsconfig.base.json"], "compilerOptions": {"paths": {}}}"#,
            ),
        ]);
        let read = |path: &str| aliases(path, &mut reader(&tree)).unwrap();
        assert_eq!(read("packages/a/tsconfig.json").candidates("@shared/x"), ["packages/shared/src/x"]);
        assert_eq!(
            read("packages/b/tsconfig.json").candidates("@shared/x"),
            ["packages/b/packages/shared/src/x", "packages/b/@shared/x"]
        );
        assert_eq!(read("packages/c/tsconfig.json").candidates("~/x"), ["packages/c/src/x"]);
        assert_eq!(read("loop/a.json").base_url.as_deref(), Some("loop/a"));
        // An own empty `paths` replaces the inherited one; each base gives what the later ones do not write.
        assert_eq!(read("over/tsconfig.json"), Aliases { base_url: Some("loop/b".into()), paths: Vec::new() });
        assert_eq!(aliases("nothing.json", &mut reader(&tree)), None);
    }

    /// A solution-style configuration: its own aliases first, then each referenced project's; a target that leaves the
    /// root names nothing.
    #[test]
    fn a_project_reads_its_references() {
        let tree = files(&[
            (
                "tsconfig.json",
                r#"{"files": [], "references": [{"path": "./tsconfig.app.json"}, {"path": "./tools"}, {"path": "./tsconfig.node.json"}]}"#,
            ),
            ("tsconfig.app.json", r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"], "up/*": ["../../*"]}}}"#),
            ("tools/tsconfig.json", r#"{"compilerOptions": {"baseUrl": "."}}"#),
            ("tsconfig.node.json", r#"{"compilerOptions": {}}"#),
        ]);
        let projects = project_aliases("tsconfig.json", &mut reader(&tree));
        assert_eq!(
            projects,
            [
                Aliases { base_url: None, paths: pairs(&[("@/*", &["src/*"]), ("up/*", &[])]) },
                Aliases { base_url: Some("tools".into()), paths: Vec::new() }
            ]
        );
        assert!(project_aliases("tsconfig.node.json", &mut reader(&tree)).is_empty());
    }
}
