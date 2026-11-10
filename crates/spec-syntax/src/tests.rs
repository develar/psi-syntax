use super::*;

fn main_kind() -> SpecFile {
    kind_of("openspec/specs/branch/unapply/spec.md").unwrap()
}

fn delta_kind() -> SpecFile {
    kind_of("idecli/openspec/changes/add-limits/specs/cli/spec.md").unwrap()
}

fn text_of<'s>(src: &'s str, span: &Span) -> &'s str {
    &src[span.start..span.end]
}

#[test]
fn a_path_names_its_kind() {
    assert_eq!(main_kind(), SpecFile::Main { root: "openspec".into(), capability: "branch/unapply".into() });
    assert_eq!(delta_kind(), SpecFile::Delta { root: "idecli/openspec".into(), change: "add-limits".into(), capability: "cli".into() });
    assert_eq!(delta_kind().main_path().as_deref(), Some("idecli/openspec/specs/cli/spec.md"));
    assert_eq!(kind_of("plugins/air/spec/actions/agents-page.spec.md"), Some(SpecFile::IntelliJ));
    // An archived change, a change's proposal and tasks, a spec directory's other files: no spec.
    for path in [
        "openspec/changes/archive/2026-11-10-204-lease/specs/cli/lease/spec.md",
        "openspec/changes/add-limits/proposal.md",
        "openspec/changes/add-limits/tasks.md",
        "openspec/specs/spec.md",
        "openspec/specs/branch/README.md",
        "openspec/specs/x.spec.md",
        "docs/.spec.md",
        "src/foo.spec.ts",
    ] {
        assert_eq!(kind_of(path), None, "{path}");
    }
    assert!(is_archived("openspec/changes/archive/2026-11-10-204-lease/proposal.md"));
    assert!(is_archived("code-engine/ceh/openspec/changes/archive/2026-01-01-x/specs/a/spec.md"));
    assert!(!is_archived("openspec/changes/add-limits/specs/cli/spec.md"));
    assert!(!is_archived("docs/changes/archive/notes.md"));
}

const MAIN: &str = "---
name: Unapply
description: \"Takes a branch out\"
style: plain-1
targets:
  - crates/unapply/src/lib.rs
  - src/lib/*.ts
---
# Unapply

## Purpose

What unapply does,
in two lines.

## Requirements

### Requirement: Unapply: The branch leaves the checkout ###

The system SHALL take the branch out.

#### Scenario: A clean branch
- **WHEN** the user unapplies
- **THEN** the files leave
  [@test] crates/unapply/src/lib_test.rs (`unapplies_a_clean_branch`)

#### Scenario: Empty

#### Scenario: C#
- **THEN** it keeps its hash

```md
### Requirement: Not one
#### Scenario: Not one either
```

### The system SHALL keep parked branches
Body.

## Testing

  [@test] tests/e2e/unapply.spec.ts (`unapplies
    twice`, `undoes`)
";

#[test]
fn a_main_spec_reads_as_openspec_show_reads_it() {
    let doc = parse(&main_kind(), MAIN.as_bytes());
    let front = doc.front.as_ref().unwrap();
    assert_eq!(front.name.as_deref(), Some("Unapply"));
    assert_eq!(front.style.as_deref(), Some("plain-1"));
    assert_eq!(front.targets.iter().map(|t| t.path.as_str()).collect::<Vec<_>>(), ["crates/unapply/src/lib.rs", "src/lib/*.ts"]);
    assert_eq!(text_of(MAIN, &front.targets[1].span), "  - src/lib/*.ts");
    assert_eq!(doc.title.as_deref(), Some("Unapply"));
    assert_eq!(doc.description.as_deref(), Some("Takes a branch out"));
    let names: Vec<&str> = doc.requirements.iter().map(|r| r.name.as_str()).collect();
    // The closing run goes, a fenced header is no requirement, a heading without `Requirement:` is one.
    assert_eq!(names, ["Unapply: The branch leaves the checkout", "The system SHALL keep parked branches"]);
    assert!(doc.requirements.iter().all(|r| r.op == Op::Main));
    let first = &doc.requirements[0];
    assert_eq!(first.span.line, 18);
    assert!(MAIN[first.span.name_at..].starts_with("Unapply: The branch"));
    assert!(text_of(MAIN, &first.span).ends_with("```"), "the block runs to its last non-blank line");
    // A scenario without a body is none; `C#` keeps its `#`.
    let scenarios: Vec<(&str, usize)> = doc.scenarios.iter().map(|s| (s.name.as_str(), s.requirement)).collect();
    assert_eq!(scenarios, [("A clean branch", 0), ("C#", 0)]);
    assert_eq!(doc.links.len(), 2);
    assert_eq!(doc.links[0].path, "crates/unapply/src/lib_test.rs");
    assert_eq!(doc.links[0].names, ["unapplies_a_clean_branch"]);
    assert_eq!(doc.links[0].owner, Owner::Scenario(0));
    // A group of names may close on the next line of its paragraph.
    assert_eq!(doc.links[1].names, ["unapplies\n    twice", "undoes"]);
    assert_eq!(doc.links[1].owner, Owner::None);
    assert_eq!((doc.links[1].span.line, doc.links[1].span.end_line), (42, 43));
}

#[test]
fn a_description_falls_back_to_purpose() {
    let doc = parse(&main_kind(), b"# x\n\n## Purpose\n\nFirst line\nsecond.\n\nNot this.\n\n## Requirements\n");
    assert_eq!(doc.description.as_deref(), Some("First line second."));
    assert!(doc.front.is_none());
}

#[test]
fn bom_and_crlf_change_no_structure() {
    let src = "\u{feff}## Requirements\r\n\r\n### Requirement: A\r\nBody\r\n\r\n#### Scenario: S\r\n- THEN x\r\n";
    let doc = parse(&main_kind(), src.as_bytes());
    assert_eq!(doc.requirements.len(), 1);
    assert_eq!(doc.requirements[0].name, "A");
    assert_eq!(doc.scenarios[0].name, "S");
    assert_eq!(text_of(src, &doc.scenarios[0].span), "#### Scenario: S\r\n- THEN x");
}

const DELTA: &str = "## ADDED Requirements

### Requirement: Limits show 10 rows
The system SHALL show 10 rows.

#### Scenario: Few rows
- THEN all show

### Not a requirement header

## Modified requirements

###Requirement: The header names its counts
Body.

## REMOVED Requirements

### Requirement: Old output
**Reason**: gone

- `### Requirement: Older output`
* ### Requirement: Oldest output

## RENAMED Requirements

- FROM: `### Requirement: Start`
- TO: `### Requirement: Launch`

## ADDED Requirements

### Requirement: A second added block
Body.
";

#[test]
fn a_delta_reads_as_archive_reads_it() {
    let doc = parse(&delta_kind(), DELTA.as_bytes());
    let got: Vec<(&str, Op, Option<&str>)> = doc.requirements.iter().map(|r| (r.name.as_str(), r.op, r.from.as_deref())).collect();
    assert_eq!(
        got,
        [
            ("Limits show 10 rows", Op::Added, None),
            ("The header names its counts", Op::Modified, None),
            ("Old output", Op::Removed, None),
            ("Older output", Op::Removed, None),
            ("Oldest output", Op::Removed, None),
            ("Launch", Op::Renamed, Some("Start")),
            ("A second added block", Op::Added, None),
        ]
    );
    // A stray `###` is part of the block before it, as `parseRequirementBlocksFromSection` absorbs it.
    assert!(text_of(DELTA, &doc.requirements[0].span).ends_with("### Not a requirement header"));
    assert_eq!(doc.scenarios.iter().map(|s| (s.name.as_str(), s.requirement)).collect::<Vec<_>>(), [("Few rows", 0)]);
    let renamed = &doc.requirements[5];
    assert_eq!((renamed.span.line, renamed.span.end_line), (26, 27));
    assert!(DELTA[renamed.span.name_at..].starts_with("Launch`"));
}

const INTELLIJ: &str = "---
name: Agents page
description: The page of agents
style: plain-1
targets:
  - ../../frontend/src/AgentsPage.kt
  - ../../frontend/src/*.kt
---

# Agents page

## Summary

The page lists agents.

## Requirements

- The page lists every agent.
  [@test] ../../frontend/testSrc/AgentsPageTest.kt (`lists every agent`, `lists none`)
- The page sorts by name.
  - a detail of the rule
### Sorting
- Ties keep their order.
- [@test](../../frontend/testSrc/SortTest.kt)

## Testing / Local Run

The `[@test]` links name the tests.
[@test] ../../frontend/testSrc/SmokeTest.kt
";

#[test]
fn an_intellij_spec_has_rules_and_links() {
    let kind = kind_of("plugins/air/spec/actions/agents-page.spec.md").unwrap();
    let doc = parse(&kind, INTELLIJ.as_bytes());
    assert_eq!(doc.format, Format::IntelliJ);
    assert_eq!(doc.description.as_deref(), Some("The page of agents"));
    assert!(doc.requirements.is_empty());
    let rules: Vec<(&str, Option<&str>)> = doc.rules.iter().map(|r| (r.text.as_str(), r.group.as_deref())).collect();
    assert_eq!(rules, [("The page lists every agent.", None), ("The page sorts by name.", None), ("Ties keep their order.", Some("Sorting"))]);
    assert_eq!(doc.rules[1].span.end_line, 21, "a nested bullet is its rule's");
    let links: Vec<(&str, &[String], Owner)> = doc.links.iter().map(|l| (l.path.as_str(), l.names.as_slice(), l.owner)).collect();
    assert_eq!(links.len(), 3, "prose naming `[@test]` is no link");
    assert_eq!(links[0], ("../../frontend/testSrc/AgentsPageTest.kt", &["lists every agent".to_string(), "lists none".to_string()][..], Owner::Rule(0)));
    assert_eq!((links[1].0, links[1].2), ("../../frontend/testSrc/SortTest.kt", Owner::Rule(2)));
    assert_eq!((links[2].0, links[2].2), ("../../frontend/testSrc/SmokeTest.kt", Owner::None));
    let resolved = resolve("plugins/air/spec/actions/agents-page.spec.md", doc.format, &doc.links[0].path);
    assert_eq!(resolved.as_deref(), Some("plugins/air/frontend/testSrc/AgentsPageTest.kt"));
}

#[test]
fn a_path_resolves_by_its_format() {
    let spec = "openspec/specs/branch/unapply/spec.md";
    assert_eq!(resolve(spec, Format::OpenSpec, "crates/a/src/lib.rs").as_deref(), Some("crates/a/src/lib.rs"));
    assert_eq!(resolve(spec, Format::OpenSpec, "../x/spec.md").as_deref(), Some("openspec/specs/branch/x/spec.md"));
    assert_eq!(resolve(spec, Format::OpenSpec, "./a.rs").as_deref(), Some("openspec/specs/branch/unapply/a.rs"));
    let air = "plugins/air/spec/a.spec.md";
    assert_eq!(resolve(air, Format::IntelliJ, "../src/A.kt").as_deref(), Some("plugins/air/src/A.kt"));
    assert_eq!(resolve(air, Format::IntelliJ, "B.kt").as_deref(), Some("plugins/air/spec/B.kt"));
    assert_eq!(resolve(air, Format::IntelliJ, "../../../../x").as_deref(), None, "above the root");
    assert_eq!(resolve(air, Format::IntelliJ, "https://google.com").as_deref(), None);
    assert_eq!(resolve(air, Format::IntelliJ, "../src/*.kt").as_deref(), Some("plugins/air/src/*.kt"));
}

#[test]
fn globs_match_as_air_reads_them() {
    assert!(glob_matches("a/*.kt", "a/B.kt"));
    assert!(!glob_matches("a/*.kt", "a/b/C.kt"));
    assert!(glob_matches("a/**/*.kt", "a/C.kt"));
    assert!(glob_matches("a/**/*.kt", "a/b/c/C.kt"));
    assert!(glob_matches("a/**", "a/b/c"));
    assert!(glob_matches("a/?.kt", "a/B.kt"));
    assert!(!glob_matches("a/?.kt", "a/BB.kt"));
    assert!(is_glob("a/*.kt") && !is_glob("a/b.kt"));
}

#[test]
fn a_backlink_names_its_spec() {
    assert_eq!(backlink("  // @spec plugins/air/spec/a.spec.md"), Some("plugins/air/spec/a.spec.md"));
    assert_eq!(backlink("//@spec x.md trailing"), Some("x.md"));
    assert_eq!(backlink("// @specs x.md"), None);
    assert_eq!(backlink("let a = 1; // @spec x.md"), None);
}

#[test]
fn fences_hide_structure() {
    // A backtick fence's info string holds no backtick (```npm test``` is inline code); a longer fence closes it.
    let src = "## Requirements\n\n```npm test```\n### Requirement: Kept\nBody.\n````\n```\n### Requirement: Hidden\n````\n";
    let doc = parse(&main_kind(), src.as_bytes());
    assert_eq!(doc.requirements.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Kept"]);
}
