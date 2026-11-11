//! Spec documents, read as their formats define them (decision 2026-11-10-specs-parsed-by-spec-syntax of accio).
//!
//! Two formats, told apart by the path alone ([`kind_of`]), so a file is the same kind in every tree and at every
//! revision:
//!
//! - **OpenSpec** (openspec.dev): a main spec `<root>/openspec/specs/<capability>/spec.md` and the delta of an open
//!   change `<root>/openspec/changes/<id>/specs/<capability>/spec.md`. A change archived under
//!   `openspec/changes/archive/` is history, no spec ([`is_archived`]). The grammar is `@fission-ai/openspec` 1.16.1's:
//!   a main spec as `MarkdownParser.parseSpec` reads it (`openspec show --json`), a delta as `parseDeltaSpec`
//!   (`requirement-blocks.js`) reads it for `openspec archive`.
//! - **IntelliJ** (`community/.ai/spec/SPEC_GUIDE.md` of intellij): a `*.spec.md` file, its front matter `name`,
//!   `description`, `style` and `targets`, its rules the bullets of `## Requirements`, as `AirSpecReferencesTest` reads
//!   them.
//!
//! Both link tests by `[@test] <path> (`<name>`, …)` lines ([`TestLink`]) and name the code they describe by
//! `targets` ([`Target`]); a path resolves by the rule of its format ([`resolve`]). Lines inside a code fence are
//! no structure (`code-fence.js`), and neither is the front matter. Every place is a byte offset into the bytes given
//! and a 1-based line, so a reader of the original file (accio's records, Branch Deck's review) needs no other text.

/// What a spec file is, by its path (`/`-separated, relative to the repository root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecFile {
    /// `<root>/specs/<capability>/spec.md`, `root` the `openspec` directory (`idecli/openspec`).
    Main { root: String, capability: String },
    /// `<root>/changes/<change>/specs/<capability>/spec.md` of a change not archived.
    Delta { root: String, change: String, capability: String },
    /// An IntelliJ `*.spec.md`.
    IntelliJ,
}

impl SpecFile {
    pub fn format(&self) -> Format {
        match self {
            SpecFile::Main { .. } | SpecFile::Delta { .. } => Format::OpenSpec,
            SpecFile::IntelliJ => Format::IntelliJ,
        }
    }

    /// The capability of an OpenSpec file (`branch/unapply`).
    pub fn capability(&self) -> Option<&str> {
        match self {
            SpecFile::Main { capability, .. } | SpecFile::Delta { capability, .. } => Some(capability),
            SpecFile::IntelliJ => None,
        }
    }

    /// The main spec an OpenSpec file's capability is: itself for a main spec, the one its delta changes otherwise.
    pub fn main_path(&self) -> Option<String> {
        match self {
            SpecFile::Main { root, capability } | SpecFile::Delta { root, capability, .. } => Some(format!("{root}/specs/{capability}/spec.md")),
            SpecFile::IntelliJ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    OpenSpec,
    IntelliJ,
}

/// The kind of spec `path` is, None for any other file (an archived change's included: [`is_archived`]).
pub fn kind_of(path: &str) -> Option<SpecFile> {
    // Both forms end so, and nearly no path does: every path of an answer is asked (`is_test_path`), its segments
    // gathered only for the few that may be one.
    if !path.ends_with("spec.md") {
        return None;
    }
    let segments: Vec<&str> = path.split('/').collect();
    let file = *segments.last()?;
    if let Some(at) = segments.iter().position(|&s| s == "openspec") {
        let root = segments[..=at].join("/");
        let rest = &segments[at + 1..];
        if file != "spec.md" {
            return None;
        }
        return match rest {
            ["specs", capability @ .., _] if !capability.is_empty() => Some(SpecFile::Main { root, capability: capability.join("/") }),
            ["changes", change, "specs", capability @ .., _] if *change != "archive" && !capability.is_empty() => {
                Some(SpecFile::Delta { root, change: change.to_string(), capability: capability.join("/") })
            }
            _ => None,
        };
    }
    (file.len() > ".spec.md".len() && file.ends_with(".spec.md")).then_some(SpecFile::IntelliJ)
}

/// Is `path` under the archive of OpenSpec's changes (`openspec/changes/archive/`): a change `openspec archive` moved
/// there once its deltas were merged into the main specs, history that no answer about the current specs needs.
pub fn is_archived(path: &str) -> bool {
    // No allocation: a reader asks it of every path of a tree.
    const ARCHIVE: &str = "openspec/changes/archive/";
    path.match_indices(ARCHIVE).any(|(at, _)| at == 0 || path.as_bytes()[at - 1] == b'/')
}

/// A byte range of the source with its lines (1-based, `end_line` the last line holding text of it) and the byte its
/// name starts at (the heading's text, a bullet's text, a link's `[`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub end_line: u32,
    pub name_at: usize,
}

/// What a delta does with its requirement; `Main` for a requirement of a main spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Main,
    Added,
    Modified,
    Removed,
    Renamed,
}

impl Op {
    pub fn name(self) -> &'static str {
        match self {
            Op::Main => "main",
            Op::Added => "added",
            Op::Modified => "modified",
            Op::Removed => "removed",
            Op::Renamed => "renamed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The name archive matches on (`normalizeRequirementName`): the heading without `Requirement:` and its closing
    /// run of `#`; a renamed requirement's new name (`TO:`).
    pub name: String,
    pub op: Op,
    /// The old name of a renamed requirement (`FROM:`).
    pub from: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    /// `scenarioNameFromHeaderText`: the level-4 heading without `Scenario:` and its closing run.
    pub name: String,
    /// Index into [`SpecDoc::requirements`].
    pub requirement: usize,
    pub span: Span,
}

/// A rule of an IntelliJ spec: a top-level bullet of its `## Requirements`, under the `### <group>` it follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The bullet's first line, without its marker.
    pub text: String,
    pub group: Option<String>,
    pub span: Span,
}

/// What a test link proves: the innermost requirement, scenario or rule holding its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    None,
    Requirement(usize),
    Scenario(usize),
    Rule(usize),
}

/// A `[@test] <path>[ (`<name>`, …)]` line: the test file as written and the tests it names (none: the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestLink {
    pub path: String,
    pub names: Vec<String>,
    pub owner: Owner,
    pub span: Span,
}

/// An item of the front matter's `targets`: a file or a glob (`*`, `?`, `**`) as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub style: Option<String>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecDoc {
    pub format: Format,
    pub front: Option<FrontMatter>,
    /// The first level-1 heading.
    pub title: Option<String>,
    /// The front matter's `description`, else the first paragraph of `## Purpose` (OpenSpec) or `## Summary`.
    pub description: Option<String>,
    pub requirements: Vec<Requirement>,
    pub scenarios: Vec<Scenario>,
    pub rules: Vec<Rule>,
    pub links: Vec<TestLink>,
}

/// The spec `src` of a file of kind `kind`.
pub fn parse(kind: &SpecFile, src: &[u8]) -> SpecDoc {
    let lines = Lines::new(src);
    let front = lines.front_matter();
    let headings = lines.headings();
    let mut doc = SpecDoc {
        format: kind.format(),
        description: None,
        title: headings.iter().find(|h| h.level == 1).map(|h| h.title.clone()),
        front,
        requirements: Vec::new(),
        scenarios: Vec::new(),
        rules: Vec::new(),
        links: Vec::new(),
    };
    doc.description = doc.front.as_ref().and_then(|f| f.description.clone()).or_else(|| {
        let section = |title: &str| headings.iter().position(|h| h.title.eq_ignore_ascii_case(title));
        let at = section("Purpose").or_else(|| section("Summary"))?;
        lines.first_paragraph(headings[at].line + 1, headings[at].end)
    });
    match kind {
        SpecFile::Main { .. } => main_requirements(&lines, &headings, &mut doc),
        SpecFile::Delta { .. } => delta_requirements(&lines, &mut doc),
        SpecFile::IntelliJ => intellij_rules(&lines, &headings, &mut doc),
    }
    doc.links = test_links(&lines, &doc);
    doc
}

/// The repository path a link or a target `raw` of the spec at `spec_path` names, by the rule of its format: an
/// IntelliJ spec's paths are from its own directory (`AirSpecReferencesTest`: `spec.parent.resolve(link)`), an
/// OpenSpec spec's from the repository root, unless they start `./` or `../` (Branch Deck's `check-spec-links`). None
/// for a URL or a path above the root.
pub fn resolve(spec_path: &str, format: Format, raw: &str) -> Option<String> {
    if raw.is_empty() || raw.contains("://") || raw.starts_with('#') {
        return None;
    }
    let relative = format == Format::IntelliJ || raw.starts_with("./") || raw.starts_with("../");
    let mut parts: Vec<&str> = if relative && !raw.starts_with('/') { spec_path.split('/').collect() } else { Vec::new() };
    if !parts.is_empty() {
        parts.pop(); // the spec's own file name
    }
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Does the glob `glob` (a target resolved from the root: [`resolve`] keeps `*` and `?`) match `path`, as
/// `AirRepoScan.globToRegex` reads it: `**/` any directories, `**` anything, `*` and `?` within one segment.
pub fn glob_matches(glob: &str, path: &str) -> bool {
    fn at(g: &[u8], p: &[u8]) -> bool {
        match g {
            [] => p.is_empty(),
            [b'*', b'*', b'/', rest @ ..] => (0..=p.len()).any(|i| (i == 0 || p[i - 1] == b'/') && at(rest, &p[i..])),
            [b'*', b'*', rest @ ..] => (0..=p.len()).any(|i| at(rest, &p[i..])),
            [b'*', rest @ ..] => (0..=p.len()).take_while(|&i| i == 0 || p[i - 1] != b'/').any(|i| at(rest, &p[i..])),
            [b'?', rest @ ..] => p.first().is_some_and(|&c| c != b'/') && at(rest, &p[1..]),
            [c, rest @ ..] => p.first() == Some(c) && at(rest, &p[1..]),
        }
    }
    at(glob.as_bytes(), path.as_bytes())
}

/// Is `raw` a glob ([`glob_matches`]) rather than a file?
pub fn is_glob(raw: &str) -> bool {
    raw.contains('*') || raw.contains('?')
}

/// The spec path a code line points back to (`// @spec <path from the root>`, `AirSpecReferencesTest.BACKLINK`).
pub fn backlink(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("//")?.trim_start().strip_prefix("@spec")?;
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    rest.split_whitespace().next()
}

struct Line<'a> {
    start: usize,
    /// The end of its text, before `\r\n` or `\n`.
    end: usize,
    text: &'a str,
    /// Inside a code fence (its fence lines included) or the front matter: no structure.
    masked: bool,
}

struct Lines<'a> {
    lines: Vec<Line<'a>>,
    /// Lines `1..front_end` are the front matter, the first and the closing `---` included; 0 without one.
    front_end: usize,
}

struct Heading {
    level: usize,
    title: String,
    /// The byte its title starts at.
    title_at: usize,
    line: usize,
    /// The line its section ends before: the next heading of its level or above.
    end: usize,
    parent: Option<usize>,
}

impl<'a> Lines<'a> {
    fn new(src: &'a [u8]) -> Self {
        let mut lines = Vec::new();
        let mut start = if src.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
        while start < src.len() {
            let end = memchr_newline(src, start);
            let next = if end < src.len() { end + 1 } else { end };
            let text_end = if end > start && src[end - 1] == b'\r' { end - 1 } else { end };
            // A line that is no UTF-8 holds no structure a reader could name.
            let text = std::str::from_utf8(&src[start..text_end]).unwrap_or("");
            lines.push(Line { start, end: text_end, text, masked: false });
            start = next;
        }
        let mut front_end = 0;
        if lines.first().is_some_and(|l| l.text.trim_end() == "---")
            && let Some(close) = lines.iter().skip(1).position(|l| l.text.trim_end() == "---")
        {
            front_end = close + 2;
        }
        let mut fence: Option<(u8, usize)> = None;
        for (i, line) in lines.iter_mut().enumerate() {
            if i < front_end {
                line.masked = true;
                continue;
            }
            match fence {
                None => {
                    if let Some(open) = fence_marker(line.text) {
                        fence = Some(open);
                        line.masked = true;
                    }
                }
                Some(open) => {
                    line.masked = true;
                    if is_closing_fence(line.text, open) {
                        fence = None;
                    }
                }
            }
        }
        Lines { lines, front_end }
    }

    fn span(&self, from: usize, to: usize, name_at: usize) -> Span {
        // The text of a block ends at its last non-blank line (`raw.trimEnd()`).
        let mut last = to.max(from + 1) - 1;
        while last > from && self.lines[last].text.trim().is_empty() {
            last -= 1;
        }
        Span { start: self.lines[from].start, end: self.lines[last].end, line: from as u32 + 1, end_line: last as u32 + 1, name_at }
    }

    fn front_matter(&self) -> Option<FrontMatter> {
        if self.front_end == 0 {
            return None;
        }
        let mut front = FrontMatter::default();
        let mut in_targets = false;
        for i in 1..self.front_end - 1 {
            let line = &self.lines[i];
            let trimmed = line.text.trim();
            if trimmed == "targets:" {
                in_targets = true;
                continue;
            }
            if in_targets {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    let path = item.trim();
                    let at = line.start + line.text.find(path).unwrap_or(0);
                    front.targets.push(Target { path: path.to_string(), span: self.span(i, i + 1, at) });
                    continue;
                }
                if !trimmed.is_empty() {
                    in_targets = false;
                }
            }
            let value = |key: &str| line.text.strip_prefix(key).map(|v| unquote(v.trim()).to_string());
            if let Some(v) = value("name:") {
                front.name = Some(v);
            } else if let Some(v) = value("description:") {
                front.description = Some(v);
            } else if let Some(v) = value("style:") {
                front.style = Some(v);
            }
        }
        Some(front)
    }

    /// The headings `MarkdownParser.parseSections` reads (`^(#{1,6})\s+(.+)$` on a line outside a fence), each with its
    /// section and its parent.
    fn headings(&self) -> Vec<Heading> {
        let mut out: Vec<Heading> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            if line.masked {
                continue;
            }
            let Some((level, title_at)) = atx(line.text) else { continue };
            while stack.last().is_some_and(|&h| out[h].level >= level) {
                out[stack.pop().expect("checked")].end = i;
            }
            let title = strip_closing_sequence(&line.text[title_at..]).to_string();
            out.push(Heading { level, title, title_at: line.start + title_at, line: i, end: self.lines.len(), parent: stack.last().copied() });
            stack.push(out.len() - 1);
        }
        out
    }

    /// `getContentUntilNextHeader(..).trim()` is not empty: a heading with a body.
    fn has_body(&self, from: usize, to: usize) -> bool {
        self.lines[from..to].iter().any(|l| !l.text.trim().is_empty())
    }

    fn first_paragraph(&self, from: usize, to: usize) -> Option<String> {
        let text: Vec<&str> = self.lines[from..to]
            .iter()
            .skip_while(|l| l.text.trim().is_empty())
            .take_while(|l| !l.text.trim().is_empty() && (l.masked || atx(l.text).is_none()))
            .map(|l| l.text.trim())
            .collect();
        (!text.is_empty()).then(|| text.join(" "))
    }
}

fn memchr_newline(src: &[u8], from: usize) -> usize {
    src[from..].iter().position(|&b| b == b'\n').map_or(src.len(), |i| from + i)
}

fn unquote(v: &str) -> &str {
    v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))).unwrap_or(v)
}

/// `code-fence.js` `getFenceMarker`: `^\s*(`{3,}|~{3,})(.*)`, a backtick fence's info string without a backtick.
fn fence_marker(text: &str) -> Option<(u8, usize)> {
    let t = text.trim_start();
    let c = *t.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let n = t.bytes().take_while(|&b| b == c).count();
    if n < 3 || (c == b'`' && t[n..].contains('`')) {
        return None;
    }
    Some((c, n))
}

/// `isClosingFence`: `^\s*(`{3,}|~{3,})\s*$` of the opening's marker, at least as long.
fn is_closing_fence(text: &str, (c, n): (u8, usize)) -> bool {
    let t = text.trim();
    let run = t.bytes().take_while(|&b| b == c).count();
    run >= n && run == t.len()
}

/// An ATX heading `^(#{1,6})\s+(.+)$`: its level and the byte of the line its title starts at.
fn atx(text: &str) -> Option<(usize, usize)> {
    let level = text.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &text[level..];
    if !rest.starts_with(|c: char| c.is_whitespace()) || rest.trim_start().is_empty() {
        // `(.+)` after `\s+`: a heading of whitespace alone still has a title, but none worth a name.
        return (rest.chars().count() >= 2 && rest.starts_with(|c: char| c.is_whitespace())).then_some((level, text.len()));
    }
    Some((level, text.len() - rest.trim_start().len()))
}

/// `stripClosingSequence`: `[ \t]+#+[ \t]*$` removed, then trimmed; `C#` keeps its `#`.
fn strip_closing_sequence(title: &str) -> &str {
    let t = title.trim_end_matches([' ', '\t']);
    let hashes = t.len() - t.trim_end_matches('#').len();
    if hashes > 0 {
        let before = &t[..t.len() - hashes];
        if before.ends_with([' ', '\t']) {
            return before.trim();
        }
    }
    title.trim()
}

fn strip_prefix_ignore_case<'s>(s: &'s str, prefix: &str) -> Option<&'s str> {
    (s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix)).then(|| &s[prefix.len()..])
}

/// `scenarioNameFromHeaderText`.
fn scenario_name(title: &str) -> String {
    let t = strip_closing_sequence(title);
    strip_prefix_ignore_case(t, "Scenario:").map_or(t, |r| r.trim_start()).trim().to_string()
}

/// The byte of `line` the name `name` is written at, after `from`; `fallback` where an escape spells it otherwise.
fn name_at(line: &Line<'_>, from: usize, name: &str, fallback: usize) -> usize {
    line.text.get(from..).and_then(|t| t.find(name)).map_or(fallback, |i| line.start + from + i)
}

/// A main spec's requirements and scenarios, as `MarkdownParser.parseSpec` reads them: the children of the first
/// section titled `Requirements` (any level, any case), each named without `Requirement:`; a requirement's scenarios
/// its children with a body.
fn main_requirements(lines: &Lines<'_>, headings: &[Heading], doc: &mut SpecDoc) {
    let Some(section) = headings.iter().position(|h| h.title.eq_ignore_ascii_case("Requirements")) else { return };
    for (r, h) in headings.iter().enumerate().filter(|(_, h)| h.parent == Some(section)) {
        let title = strip_prefix_ignore_case(&h.title, "Requirement:").map_or(h.title.as_str(), |t| t.trim_start());
        let name = strip_closing_sequence(title).to_string();
        let line = &lines.lines[h.line];
        let at = name_at(line, h.title_at - line.start, &name, h.title_at);
        let index = doc.requirements.len();
        doc.requirements.push(Requirement { name, op: Op::Main, from: None, span: lines.span(h.line, h.end, at) });
        for s in headings.iter().skip(r + 1).take_while(|s| s.line < h.end).filter(|s| s.parent == Some(r)) {
            if !lines.has_body(s.line + 1, s.end) {
                continue;
            }
            let name = scenario_name(&s.title);
            let line = &lines.lines[s.line];
            let at = name_at(line, s.title_at - line.start, &name, s.title_at);
            doc.scenarios.push(Scenario { name, requirement: index, span: lines.span(s.line, s.end, at) });
        }
    }
}

/// `REQUIREMENT_HEADER_REGEX` `^###\s*Requirement:\s*(.+)\s*$` (any case): the byte its name starts at.
fn requirement_header(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("###")?;
    let after_ws = rest.trim_start();
    let name = strip_prefix_ignore_case(after_ws, "Requirement:")?;
    let name_trimmed = name.trim_start();
    (!name_trimmed.trim_end().is_empty()).then(|| text.len() - name_trimmed.len())
}

/// `^##\s+(.+)$`: a top-level section of a delta (`splitTopLevelSections`), its title.
fn delta_section(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("##")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let title = rest.trim_start();
    (!title.is_empty()).then(|| strip_closing_sequence(title))
}

/// `FROM:` or `TO:` of a rename: `^\s*[-*+]?\s*<side>:\s*`?###\s*Requirement:\s*(.+?)`?\s*$`.
fn rename_side<'t>(text: &'t str, side: &str) -> Option<&'t str> {
    let t = text.trim_start();
    let t = t.strip_prefix(['-', '*', '+']).unwrap_or(t).trim_start();
    let t = t.strip_prefix(side)?.strip_prefix(':')?.trim_start();
    removed_bullet_name(t)
}

/// `` `?###\s*Requirement:\s*(.+?)`?\s*$ ``: the name of a bullet that lists a requirement header (these patterns,
/// unlike the header's, are case-sensitive).
fn removed_bullet_name(t: &str) -> Option<&str> {
    let t = t.strip_prefix('`').unwrap_or(t);
    let t = t.strip_prefix("###")?.trim_start();
    let t = t.strip_prefix("Requirement:")?.trim_start();
    let t = t.trim_end();
    let t = t.strip_suffix('`').unwrap_or(t).trim_end();
    (!t.is_empty()).then_some(t)
}

/// A delta's requirements, as `parseDeltaSpec` reads them: the requirement blocks of its ADDED, MODIFIED and REMOVED
/// sections (any case, a title repeated), the bullets of REMOVED, the FROM/TO pairs of RENAMED.
fn delta_requirements(lines: &Lines<'_>, doc: &mut SpecDoc) {
    let n = lines.lines.len();
    let starts: Vec<(usize, String)> =
        (0..n).filter(|&i| !lines.lines[i].masked).filter_map(|i| delta_section(lines.lines[i].text).map(|t| (i, t.to_ascii_lowercase()))).collect();
    for (k, (from, title)) in starts.iter().enumerate() {
        let to = starts.get(k + 1).map_or(n, |s| s.0);
        let body = from + 1..to;
        let op = match title.as_str() {
            "added requirements" => Op::Added,
            "modified requirements" => Op::Modified,
            "removed requirements" => Op::Removed,
            "renamed requirements" => Op::Renamed,
            _ => continue,
        };
        if op == Op::Renamed {
            let mut pending: Option<(usize, String)> = None;
            for i in body {
                let line = &lines.lines[i];
                if line.masked {
                    continue;
                }
                if let Some(name) = rename_side(line.text, "FROM") {
                    pending = Some((i, strip_closing_sequence(name).to_string()));
                } else if let Some(name) = rename_side(line.text, "TO")
                    && let Some((at, old)) = pending.take()
                {
                    let name = strip_closing_sequence(name).to_string();
                    let byte = name_at(line, 0, &name, line.start);
                    doc.requirements.push(Requirement { name, op, from: Some(old), span: lines.span(at, i + 1, byte) });
                }
            }
            continue;
        }
        let is_header = |i: usize| !lines.lines[i].masked && requirement_header(lines.lines[i].text).is_some();
        let is_top = |i: usize| !lines.lines[i].masked && delta_section(lines.lines[i].text).is_some();
        // `parseRemovedNames` reads the bullets of every line of the section, a block's lines included.
        let removed_bullet = |i: usize| -> Option<Requirement> {
            let line = &lines.lines[i];
            if op != Op::Removed || line.masked {
                return None;
            }
            let name = line.text.trim_start().strip_prefix(['-', '*', '+']).and_then(|t| removed_bullet_name(t.trim_start()))?;
            let name = strip_closing_sequence(name).to_string();
            let byte = name_at(line, 0, &name, line.start);
            Some(Requirement { name, op, from: None, span: lines.span(i, i + 1, byte) })
        };
        let mut i = body.start;
        while i < body.end {
            let line = &lines.lines[i];
            if !is_header(i) {
                doc.requirements.extend(removed_bullet(i));
                i += 1;
                continue;
            }
            let name_from = requirement_header(line.text).expect("a header");
            let name = strip_closing_sequence(&line.text[name_from..]).to_string();
            let at = name_at(line, name_from, &name, line.start + name_from);
            let header = i;
            i += 1;
            while i < body.end && !is_header(i) && !is_top(i) {
                i += 1;
            }
            let index = doc.requirements.len();
            doc.requirements.push(Requirement { name, op, from: None, span: lines.span(header, i, at) });
            doc.requirements.extend((header + 1..i).filter_map(removed_bullet));
            // The block's scenarios as `show` reads them (`toRequirements`): the children of its heading in the tree of
            // headings, each with a body; a heading of its level or above (a stray `###` the block absorbed) ends them.
            let mut min_level = usize::MAX;
            for s in header + 1..i {
                let sl = &lines.lines[s];
                let Some((level, title_at)) = (!sl.masked).then(|| atx(sl.text)).flatten() else { continue };
                if level <= 3 {
                    break;
                }
                let child = level <= min_level;
                min_level = min_level.min(level);
                if !child {
                    continue;
                }
                let end = (s + 1..i).find(|&e| !lines.lines[e].masked && atx(lines.lines[e].text).is_some_and(|(l, _)| l <= level)).unwrap_or(i);
                if lines.has_body(s + 1, end) {
                    let name = scenario_name(&sl.text[title_at..]);
                    let at = name_at(sl, title_at, &name, sl.start + title_at);
                    doc.scenarios.push(Scenario { name, requirement: index, span: lines.span(s, end, at) });
                }
            }
        }
    }
}

/// An IntelliJ spec's rules: the top-level bullets (`- `, `* `) of its `## Requirements`, each under the last `###`
/// heading above it; a rule runs to the next bullet or heading.
fn intellij_rules(lines: &Lines<'_>, headings: &[Heading], doc: &mut SpecDoc) {
    let Some(section) = headings.iter().find(|h| h.level == 2 && h.title.eq_ignore_ascii_case("Requirements")) else { return };
    let mut group: Option<String> = None;
    let mut open: Option<(usize, usize, String)> = None;
    let close = |open: &mut Option<(usize, usize, String)>, to: usize, group: &Option<String>, rules: &mut Vec<Rule>| {
        if let Some((from, at, text)) = open.take() {
            rules.push(Rule { text, group: group.clone(), span: lines.span(from, to, at) });
        }
    };
    for i in section.line + 1..section.end {
        let line = &lines.lines[i];
        if line.masked {
            continue;
        }
        if let Some((level, title_at)) = atx(line.text) {
            close(&mut open, i, &group, &mut doc.rules);
            if level == 3 {
                group = Some(strip_closing_sequence(&line.text[title_at..]).to_string());
            }
            continue;
        }
        if let Some(text) = line.text.strip_prefix("- ").or_else(|| line.text.strip_prefix("* "))
            && !text.trim_start().starts_with("[@test]")
        {
            close(&mut open, i, &group, &mut doc.rules);
            let text = text.trim();
            open = Some((i, name_at(line, 2, text, line.start + 2), text.to_string()));
        }
    }
    close(&mut open, section.end, &group, &mut doc.rules);
}

/// The `[@test]` lines outside a fence: `[@test] <path>` or `[@test](<path>)` at the start of a line or of a bullet,
/// then the optional group of backticked names, which may close on a later line of its paragraph
/// (`AirSpecReferencesTest.testLinkMethodNames`); a `[@test]` inside prose is no link.
fn test_links(lines: &Lines<'_>, doc: &SpecDoc) -> Vec<TestLink> {
    let mut out = Vec::new();
    for (i, line) in lines.lines.iter().enumerate() {
        if line.masked {
            continue;
        }
        let t = line.text.trim_start();
        let t = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).map_or(t, str::trim_start);
        let Some(rest) = t.strip_prefix("[@test]") else { continue };
        let link_at = line.start + (line.text.len() - t.len());
        let (path, after) = if let Some(inner) = rest.strip_prefix('(') {
            let close = inner.find(')').unwrap_or(inner.len());
            (&inner[..close], &inner[(close + 1).min(inner.len())..])
        } else if rest.starts_with([' ', '\t']) {
            let r = rest.trim_start();
            let end = r.find(|c: char| c.is_whitespace() || c == ')').unwrap_or(r.len());
            (r[..end].trim_end_matches([',', '.', ';']), &r[end..])
        } else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let (names, last) = link_names(lines, i, after);
        let span = lines.span(i, last + 1, link_at);
        out.push(TestLink { path: path.to_string(), names, owner: owner_of(doc, span.start), span });
    }
    out
}

/// The names of the group that opens `after` on line `i`, and the line it closes on; none (and line `i`) when no group
/// opens there or it never closes within its paragraph.
fn link_names(lines: &Lines<'_>, i: usize, after: &str) -> (Vec<String>, usize) {
    let after = after.trim_start_matches([' ', '\t']);
    if !after.starts_with('(') {
        return (Vec::new(), i);
    }
    let mut text = String::new();
    let mut depth = 0usize;
    let mut chunk = after;
    let mut at = i;
    loop {
        for (k, c) in chunk.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        text.push_str(&chunk[..k]);
                        let parts: Vec<&str> = text.split('`').collect();
                        let names = parts.iter().skip(1).step_by(2).map(|p| p.trim()).filter(|p| !p.is_empty()).map(str::to_string).collect();
                        return (names, at);
                    }
                }
                _ => {}
            }
        }
        text.push_str(chunk);
        text.push('\n');
        at += 1;
        match lines.lines.get(at) {
            Some(next) if !next.masked && !next.text.trim().is_empty() => chunk = next.text,
            _ => return (Vec::new(), i),
        }
    }
}

fn owner_of(doc: &SpecDoc, at: usize) -> Owner {
    let holds = |s: &Span| s.start <= at && at <= s.end;
    if let Some(i) = doc.scenarios.iter().rposition(|s| holds(&s.span)) {
        return Owner::Scenario(i);
    }
    if let Some(i) = doc.requirements.iter().rposition(|r| holds(&r.span)) {
        return Owner::Requirement(i);
    }
    doc.rules.iter().rposition(|r| holds(&r.span)).map_or(Owner::None, Owner::Rule)
}

#[cfg(test)]
mod tests;
