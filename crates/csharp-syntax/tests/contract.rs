//! What AGENTS.md's consumer contract promises of a C# tree, checked: a contextual keyword the parser does not
//! convert stays an `IdentifierToken` (the lexer makes every contextual keyword an identifier, `ContextualKind`
//! aside), one it converts carries its keyword kind, in `leaves()` as anywhere. Each expectation is Roslyn 5.9.0's tree
//! of the same text (`tools/csharp-oracle/oracle.sh dump`).

use csharp_syntax::{FileKind, NodeKind, Tree, dump};

/// The kind names of the token leaves of `text` spelled `word`, in document order, as `leaves()` gives them.
fn kinds_of(text: &str, word: &str) -> Vec<&'static str> {
    let tree = Tree::parse(text, FileKind::default());
    assert!(tree.diagnostics().is_empty(), "{text}: {:?}", tree.diagnostics());
    tree.leaves()
        .filter(|&(kind, ref range)| !dump::is_trivia(kind) && &text.as_bytes()[range.clone()] == word.as_bytes())
        .map(|(kind, _)| kind.debug_name())
        .collect()
}

#[test]
fn contextual_keywords_used_as_names_stay_identifiers() {
    let cases: &[(&str, &str, usize)] = &[
        // `var` as a type and `nameof` as a call are identifiers in Roslyn's tree, whatever the binder makes of them
        ("class C { void M() { var x = 1; } }", "var", 1),
        ("class C { void M(int x) { var n = nameof(x); } }", "nameof", 1),
        ("class C { int partial; void M() { partial = 1; } }", "partial", 2),
        ("class C { void M() { var async = 1; async++; } }", "async", 2),
        ("class C { int record; int M() => record; }", "record", 2),
        ("class C { void M() { int where = 0; where++; } }", "where", 2),
        ("class C { void M() { int await = 0, yield = 1, from = 2, select = 3; } }", "await", 1),
        ("class C { void M() { int await = 0, yield = 1, from = 2, select = 3; } }", "yield", 1),
        ("class C { void M() { int await = 0, yield = 1, from = 2, select = 3; } }", "from", 1),
        ("class C { int get, set, value, global, when, with; }", "get", 1),
        ("class C { int get, set, value, global, when, with; }", "value", 1),
        ("class C { int get, set, value, global, when, with; }", "global", 1),
        ("class C { int get, set, value, global, when, with; }", "when", 1),
        ("class C { void M() { int scoped = 0; scoped++; } }", "scoped", 2),
        ("class C { int allows, field, extension; }", "allows", 1),
        ("class C { int allows, field, extension; }", "field", 1),
        ("class C { int allows, field, extension; }", "extension", 1),
    ];
    for &(text, word, count) in cases {
        assert_eq!(kinds_of(text, word), vec!["IdentifierToken"; count], "{word} in {text}");
    }
}

#[test]
fn contextual_keywords_the_parser_converts_carry_their_kind() {
    let cases: &[(&str, &str, &str)] = &[
        ("partial class C { }", "partial", "PartialKeyword"),
        ("class C { async System.Threading.Tasks.Task M() { await M(); } }", "async", "AsyncKeyword"),
        ("class C { async System.Threading.Tasks.Task M() { await M(); } }", "await", "AwaitKeyword"),
        ("record R(int X);", "record", "RecordKeyword"),
        ("record struct S(int X);", "record", "RecordKeyword"),
        ("class C<T> where T : class { }", "where", "WhereKeyword"),
        ("class C { int P { get; set; } }", "get", "GetKeyword"),
        ("class C { int P { get; set; } }", "set", "SetKeyword"),
        ("class C { event System.Action E { add { } remove { } } }", "add", "AddKeyword"),
        ("class C { System.Collections.IEnumerable M() { yield return 1; } }", "yield", "YieldKeyword"),
        ("class C { object M(int[] xs) => from x in xs where x > 0 select x; }", "from", "FromKeyword"),
        ("class C { object M(int[] xs) => from x in xs where x > 0 select x; }", "where", "WhereKeyword"),
        ("class C { object M(int[] xs) => from x in xs where x > 0 select x; }", "select", "SelectKeyword"),
        ("class C { global::System.Object o; }", "global", "GlobalKeyword"),
        ("class C { bool M(object o) => o is not null and int; }", "not", "NotKeyword"),
        ("class C { bool M(object o) => o is not null and int; }", "and", "AndKeyword"),
        ("record R(int X) { R M() => this with { X = 1 }; }", "with", "WithKeyword"),
        ("class C { void M(object o) { switch (o) { case int i when i > 0: break; } } }", "when", "WhenKeyword"),
        ("file class F { }", "file", "FileKeyword"),
        ("class C { required int X { get; init; } }", "required", "RequiredKeyword"),
        ("class C { required int X { get; init; } }", "init", "InitKeyword"),
        ("class C { void M(scoped ref int x) { } }", "scoped", "ScopedKeyword"),
        ("class C<T> where T : allows ref struct { }", "allows", "AllowsKeyword"),
        ("class C { int P { get => field; } }", "field", "FieldKeyword"),
        ("static class E { extension(int i) { } }", "extension", "ExtensionKeyword"),
    ];
    for &(text, word, kind) in cases {
        assert_eq!(kinds_of(text, word), [kind], "{word} in {text}");
    }
}

/// The same name is an identifier and a keyword in one file, each where Roslyn puts it.
#[test]
fn one_word_both_ways_in_one_file() {
    let text = "partial class C { int partial; async void M() { var async = 1; await System.Threading.Tasks.Task.Delay(async); } }";
    assert_eq!(kinds_of(text, "partial"), ["PartialKeyword", "IdentifierToken"]);
    assert_eq!(kinds_of(text, "async"), ["AsyncKeyword", "IdentifierToken", "IdentifierToken"]);
    // `field` is a keyword only in an accessor (C# 14, `FileKind::default`'s Latest)
    assert_eq!(
        kinds_of("class C { int field; int P { get => field; } }", "field"),
        ["IdentifierToken", "FieldKeyword"]
    );
    let tree = Tree::parse(text, FileKind::default());
    let class = tree.descendants(tree.root()).find(|&id| tree.node_kind(id) == Some(NodeKind::ClassDeclaration));
    assert!(class.is_some());
}
