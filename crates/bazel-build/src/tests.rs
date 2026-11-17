use super::*;

/// IntelliJ's shape of a generated BUILD file: loads, a variable, a test macro, a library with a module name, a test
/// library with a bare name and a computed label.
const MEDIA: &str = r#"load("@rules_jvm//:jvm.bzl", "jvm_library")
load("@community//build:tests-options.bzl", "jps_test", opts = "options")

DEPS = ["@community//platform/util"]

jps_test(
    name = "media_test",
    data = ["//plugins/air:spec/x.md"],
    runtime_deps = [":media_test_lib"],
)

jvm_library(
    name = "media",
    module_name = "intellij.air.media",
    srcs = glob(["src/**/*.kt"]),
    visibility = ["//visibility:public"],
    deps = DEPS + ["//plugins/air/view", "@lib//:kotlin-stdlib"],
    exports = ["//plugins/air/view"],
)

jvm_library(
    name = "media_test_lib",
    testonly = True,
    srcs = ["test/A.kt"],
    associates = ["//plugins/air/media"],
    deps = ["media", "media", Label(X)],
)
"#;

fn labels(t: &Target) -> Vec<(&str, &str)> {
    t.labels.iter().map(|d| (d.attr.as_str(), d.label.as_str())).collect()
}

#[test]
fn each_target_with_its_labels_by_attribute() {
    let f = read(MEDIA.as_bytes());
    assert!(!f.partial);
    assert_eq!(
        f.loads.iter().map(|l| l.module.as_str()).collect::<Vec<_>>(),
        ["@rules_jvm//:jvm.bzl", "@community//build:tests-options.bzl"]
    );
    assert_eq!(
        f.loads[1].names,
        [("jps_test".to_string(), "jps_test".to_string()), ("opts".to_string(), "options".to_string())]
    );
    let shown: Vec<_> = f.targets.iter().map(|t| (t.name.as_deref(), t.kind.as_str(), t.line, t.test)).collect();
    assert_eq!(
        shown,
        [
            (Some("media_test"), "jps_test", 6, true),
            (Some("media"), "jvm_library", 12, false),
            (Some("media_test_lib"), "jvm_library", 21, true)
        ]
    );
    // A label of a file is kept: only the tree tells it from a target.
    assert_eq!(labels(&f.targets[0]), [("data", "//plugins/air:spec/x.md"), ("runtime_deps", ":media_test_lib")]);
    let lib = &f.targets[1];
    assert_eq!(lib.module_name.as_deref(), Some("intellij.air.media"));
    assert_eq!(lib.unknown, None);
    // The variable read in its place; a glob, the module name and the visibility write no label.
    assert_eq!(
        labels(lib),
        [
            ("deps", "@community//platform/util"),
            ("deps", "//plugins/air/view"),
            ("deps", "@lib//:kotlin-stdlib"),
            ("exports", "//plugins/air/view")
        ]
    );
    let test_lib = &f.targets[2];
    // A bare name of a target of the file is a label, once per attribute; a file name is none.
    assert_eq!(labels(test_lib), [("associates", "//plugins/air/media"), ("deps", "media")]);
    assert!(
        test_lib.unknown.as_deref().is_some_and(|u| u == "Label() of a value at line 26"),
        "{:?}",
        test_lib.unknown
    );
}

#[test]
fn a_macro_without_a_name_keeps_its_labels() {
    let f = read(b"load(\"//:m.bzl\", \"jar\")\njar(module = \":a\")\njvm_library(name = \"a\")\nexports_files([\"x\"])\npackage(default_visibility = [\"//visibility:public\"])\n");
    assert!(!f.partial);
    assert_eq!(f.targets.len(), 2, "exports_files and package declare no target");
    let jar = &f.targets[0];
    assert_eq!((jar.name.as_deref(), jar.kind.as_str(), jar.line), (None, "jar", 2));
    assert_eq!(labels(jar), [("module", ":a")]);
}

/// A statement the reader cannot read, or a comprehension of calls, hides what it writes.
#[test]
fn what_the_reader_cannot_name_makes_the_file_partial() {
    assert!(read(b"[jvm_library(name = n) for n in NAMES]\n").partial);
    assert!(read(b"def f():\n    pass\njvm_library(name = \"a\")\n").partial);
    assert!(read(b"jvm_library(name = \"a\", deps = [\n").partial);
    assert!(!read(b"jvm_library(name = \"a\")\n").partial);
}

/// A joined or formatted label and a loaded value: the first computed label is the target's unknown; a loaded name is
/// not followed.
#[test]
fn computed_and_loaded_values() {
    let f = read(b"load(\"//:defs.bzl\", \"OPENS\")\nN = \"x\"\na(name = \"a\", deps = [\"//p:\" + N], runtime_deps = OPENS)\nb(name = \"b\", deps = [\"//p:{}\".format(v)])\n");
    assert_eq!(f.targets[0].unknown.as_deref(), Some("a label joined at line 3"));
    assert_eq!(labels(&f.targets[0]), [("deps", "//p:")]);
    assert_eq!(f.targets[1].unknown.as_deref(), Some("a label formatted at line 4"));
}

/// A variable appended to, a `select`, a variable used twice and one that refers to itself.
#[test]
fn variables_are_read_in_their_place() {
    let f = read(b"D = [\"//a\"]\nD += [\":b\"]\nL = L + [\"//l\"]\nx(name = \"x\", deps = D + select({\"//c:on\": [\"//on\"], \"//conditions:default\": []}), exports = D, runtime_deps = L)\n");
    assert_eq!(
        labels(&f.targets[0]),
        [
            ("deps", "//a"),
            ("deps", ":b"),
            ("deps", "//c:on"),
            ("deps", "//on"),
            ("deps", "//conditions:default"),
            ("exports", "//a"),
            ("exports", ":b"),
            ("runtime_deps", "//l")
        ]
    );
}

fn same(old: &str, new: &str) -> bool {
    remainder(old.as_bytes()) == remainder(new.as_bytes())
}

const LIB: &str = "load(\"@rules_jvm//:jvm.bzl\", \"jvm_library\")\n\njvm_library(\n    name = \"a\",\n    srcs = glob([\"src/**/*.kt\"]),\n    deps = [\n        \"//b\",\n        \":c\",\n    ],\n    runtime_deps = [\"//d\"],\n)\n\njvm_library(name = \"c\")\n";

/// The remainders of two texts that differ only in their dependency lists are equal: a label changed, added,
/// removed, a list reformatted, a first or last dependency attribute added or removed, a `select` of labels.
#[test]
fn dependency_lists_are_not_in_the_remainder() {
    for new in [
        LIB.replace("\"//b\"", "\"//b2\""),
        LIB.replace("        \":c\",\n", "        \":c\",\n        \"//e\",\n"),
        LIB.replace("    runtime_deps = [\"//d\"],\n", ""),
        LIB.replace("    deps = [\n        \"//b\",\n        \":c\",\n    ],\n", "    deps = [\"//b\", \":c\"],\n"),
        LIB.replace(
            "    deps = [\n        \"//b\",\n        \":c\",\n    ],\n",
            "    deps = [\"//b\", \"c\"] + select({\"//cond:on\": [\"//on\"], \"//conditions:default\": []}),\n",
        ),
        LIB.replace("jvm_library(name = \"c\")", "jvm_library(name = \"c\", deps = [\"//x\"])"),
        LIB.replace("jvm_library(name = \"c\")", "jvm_library(name = \"c\", deps = [], exports = [\"//x\"],)"),
        LIB.replace(
            "    srcs = glob([\"src/**/*.kt\"]),\n",
            "    srcs = glob([\"src/**/*.kt\"]), associates = [\"//x\"],\n",
        ),
    ] {
        assert!(same(LIB, &new), "{new}");
    }
}

/// Anything else is in the remainder: a name, the sources, a visibility, a new target or load, a comment, a
/// variable's labels, a list holding a file or a loaded value, a `testonly`.
#[test]
fn everything_else_is_in_the_remainder() {
    for new in [
        LIB.replace("name = \"a\"", "name = \"a2\""),
        LIB.replace("src/**/*.kt", "src/**/*.java"),
        LIB.replace("    runtime_deps", "    visibility = [\"//visibility:public\"],\n    runtime_deps"),
        format!("{LIB}jvm_library(name = \"e\")\n"),
        LIB.replace("\"jvm_library\")\n", "\"jvm_library\")\nload(\"//:defs.bzl\", \"x\")\n"),
        LIB.replace("\n\njvm_library(name", "\n\n# the second library\njvm_library(name"),
        LIB.replace("    runtime_deps = [\"//d\"],\n", "    runtime_deps = [\"//d\", \"A.kt\"],\n"),
        LIB.replace("    runtime_deps = [\"//d\"],\n", "    runtime_deps = OPENS,\n"),
        LIB.replace("    runtime_deps = [\"//d\"],\n", "    runtime_deps = [\"//d\"],\n    testonly = True,\n"),
        LIB.replace("jvm_library(name = \"c\")", "jvm_library(name = \"c\", deps = [\"//p:\" + n])"),
    ] {
        assert!(!same(LIB, &new), "{new}");
    }
    // A variable's value stays; the attribute naming it is cut.
    let vars = "D = [\"//a\"]\nx(name = \"x\", deps = D)\n";
    assert!(!same(vars, &vars.replace("//a", "//b")));
    assert!(same(vars, "D = [\"//a\"]\nx(name = \"x\")\n"));
    // A tuple's trailing comma is its value, a call's is not.
    assert!(!same("x(name = \"x\", srcs = (\"a\",))\n", "x(name = \"x\", srcs = (\"a\"))\n"));
    assert!(same("x(name = \"x\",)\n", "x(\n    name = \"x\"\n)\n"));
}

/// The remainder is the file's tokens, whitespace left out, each on a line of its own.
#[test]
fn the_remainder_is_the_tokens_each_on_a_line() {
    assert_eq!(
        String::from_utf8(remainder(b"x(name = \"x\", deps = [\"//a\"])  # c\n")).unwrap(),
        "x\n(\nname\n=\n\"x\"\n)\n# c\n"
    );
    assert_eq!(remainder(b""), b"");
}
