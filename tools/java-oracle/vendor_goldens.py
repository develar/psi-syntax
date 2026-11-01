#!/usr/bin/env python3
"""Vendor IntelliJ's Java parsing goldens into testdata/java/.

    tools/java-oracle/vendor_goldens.py [--repo IDEA_CHECKOUT] [--rev REV] [--out testdata/java]

Reads java/java-tests/testData/psi/parser-{full,partial} and the test classes that use
them (java/java-frontback-tests/testSrc/com/intellij/java/parser and the concrete
classes in java/java-tests/testSrc/com/intellij/java/parser) at REV with `git show`,
never from the working tree.

For every test method that checks a golden it writes, under OUT/<parser-full|parser-partial>/<dir>/:
  NAME.java      the parsed text: the test data file for parser-full (verbatim; the test trims it),
                 the string the test passes for parser-partial (the exact argument, not trimmed)
  NAME.txt       DebugUtil.psiToString(file, true, false), the golden as committed
  NAME_node.txt  DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true), as committed
and one manifest line `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM<TAB>TEST` in OUT/manifest.tsv:
  LEVEL  the LanguageLevel the test parses at: its setLanguageLevel(...) or else
         JavaTestUtil.getMaxRegisteredLanguageLevel(), the last LanguageLevel constant (JDK_X)
  ENTRY  `file` (the whole file through JavaParserDefinition) or the parser entry of a partial test
         (see JavaPsiOracle.java)
  TRIM   1 when the test trims the text (ParsingTestFixture.loadFile), else 0
JShell goldens (parser-full/jshell, JShellParserDefinition) are not vendored.
"""

import pathlib
import argparse
import os
import re
import subprocess
import sys

REV = "decc943674edfe7ecc620d2d60b61be38f876994"
DATA = "java/java-tests/testData/psi/"
ABSTRACT = "java/java-frontback-tests/testSrc/com/intellij/java/parser/"
LEVELS = "java/java-syntax/src/com/intellij/pom/java/LanguageLevel.kt"

# The concrete partial tests (java/java-tests/testSrc/.../parser/partial/*Test.java)
# implement each abstract helper with one parser call; ENTRY names that call.
PARTIAL_ENTRIES = {
    ("AbstractBasicAnnotationParserTest", "doParserTest", 1): lambda a: "annotations",
    ("AbstractBasicDeclarationParserTest", "doParserTest", 1): lambda a: "class_body",
    ("AbstractBasicDeclarationParserTest", "doParserTest", 3): lambda a: {
        (False, False): "class_body", (True, False): "class_body_annotation", (False, True): "class_body_enum",
    }[(a[1], a[2])],
    ("AbstractBasicExpressionParserTest", "doParserTest", 1): lambda a: "expression",
    ("AbstractBasicExpressionParserTest", "doParserTest", 0): lambda a: "expression",
    ("AbstractBasicFileParserTest", "doParserTest", 1): lambda a: "file_parser",
    ("AbstractBasicModuleParserTest", "doParserTest", 1): lambda a: "file_parser",
    ("AbstractBasicPatternParserTest", "doParserTest", 1): lambda a: "pattern",
    ("AbstractBasicReferenceParserTest", "doRefParserTest", 2): lambda a: "reference_incomplete" if a[1] else "reference",
    ("AbstractBasicReferenceParserTest", "doTypeParserTest", 1): lambda a: "type",
    ("AbstractBasicReferenceParserTest", "doTypeParamsParserTest", 1): lambda a: "type_params",
    ("AbstractBasicStatementParserTest", "doBlockParserTest", 1): lambda a: "code_block_deep",
    ("AbstractBasicStatementParserTest", "doParserTest", 1): lambda a: "statements",
}
CALLS = ("doTest", "doParserTest", "doRefParserTest", "doTypeParserTest", "doTypeParamsParserTest", "doBlockParserTest")


class Git:
    def __init__(self, repo, rev):
        self.repo, self.rev = repo, rev
        self.batch = subprocess.Popen(["git", "-C", repo, "cat-file", "--batch"], stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE)

    def ls(self, path):
        out = subprocess.run(["git", "-C", self.repo, "ls-tree", "-r", "--name-only", self.rev, path],
                             check=True, capture_output=True, text=True).stdout
        return [line for line in out.splitlines() if line]

    def show(self, path):
        self.batch.stdin.write(f"{self.rev}:{path}\n".encode())
        self.batch.stdin.flush()
        header = self.batch.stdout.readline().decode().split()
        if header[-1] == "missing":
            raise FileNotFoundError(path)
        data = self.batch.stdout.read(int(header[2]))
        self.batch.stdout.read(1)
        return data


def java_string(src, i):
    """Parses the string literal or text block at src[i]; returns (value, end)."""
    if src.startswith('"""', i):
        j = src.index("\n", i + 3) + 1
        end = j
        while True:
            end = src.index('"""', end)
            backslashes = 0
            while src[end - 1 - backslashes] == "\\":
                backslashes += 1
            if backslashes % 2 == 0:
                break
            end += 1
        raw = src[j:end]
        lines = raw.split("\n")
        # JLS 3.10.6: the closing delimiter's line counts for the indentation when it is blank.
        significant = [line for line in lines[:-1] if line.strip()] + [lines[-1]]
        indent = min(len(line) - len(line.lstrip(" \t")) for line in significant)
        stripped = [line[indent:].rstrip(" \t") for line in lines[:-1]] + [lines[-1][indent:]]
        return unescape("\n".join(stripped), text_block=True), end + 3
    assert src[i] == '"'
    j = i + 1
    while src[j] != '"':
        j += 2 if src[j] == "\\" else 1
    return unescape(src[i + 1:j], text_block=False), j + 1


def unescape(s, text_block):
    out = []
    i = 0
    simple = {"n": "\n", "t": "\t", "b": "\b", "r": "\r", "f": "\f", "s": " ", '"': '"', "'": "'", "\\": "\\"}
    while i < len(s):
        c = s[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        n = s[i + 1]
        if n in simple:
            out.append(simple[n])
            i += 2
        elif n == "\n" and text_block:
            i += 2
        elif n in "01234567":
            m = re.match(r"[0-3][0-7]{0,2}|[0-7]{1,2}", s[i + 1:])
            out.append(chr(int(m.group(0), 8)))
            i += 1 + len(m.group(0))
        else:
            raise ValueError(f"escape \\{n}")
    return "".join(out)


def skip_space(src, i):
    while src[i] in " \t\r\n":
        i += 1
    return i


def parse_args(src, i):
    """Parses the arguments of a call whose '(' is at src[i]: concatenated strings and booleans."""
    args = []
    i = skip_space(src, i + 1)
    if src[i] == ")":
        return args, i + 1
    while True:
        i = skip_space(src, i)
        if src[i] == '"':
            parts = []
            while True:
                value, i = java_string(src, i)
                parts.append(value)
                i = skip_space(src, i)
                if src[i] != "+":
                    break
                i = skip_space(src, i + 1)
            args.append("".join(parts))
        else:
            m = re.compile(r"true|false").match(src, i)
            if not m:
                raise ValueError(f"argument at {src[i:i + 40]!r}")
            args.append(m.group(0) == "true")
            i = m.end()
        i = skip_space(src, i)
        if src[i] == ")":
            return args, i + 1
        assert src[i] == ",", src[i:i + 40]
        i += 1


def test_methods(src):
    """Yields (method name, level or None, call name, args) for each @Test method."""
    for m in re.finditer(r"public void (test\w+)\(\)\s*\{", src):
        body_start = m.end()
        next_method = src.find("public void test", body_start)
        call = re.compile(r"\b(" + "|".join(CALLS) + r")\(").search(src, body_start,
                                                                      next_method if next_method >= 0 else len(src))
        if call is None:
            yield m.group(1), None, None, None
            continue
        level = re.compile(r"setLanguageLevel\(LanguageLevel\.(\w+)\)").search(src, body_start, call.start())
        args, _ = parse_args(src, call.end() - 1)
        yield m.group(1), level.group(1) if level else None, call.group(1), args


def test_name(method):
    # PlatformTestUtil.getTestName(name, lowercaseFirstLetter = false)
    return method[len("test"):] if method.startswith("test") else method


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default=str(pathlib.Path.home() / ".cache/psi-syntax/intellij-community"))
    parser.add_argument("--rev", default=REV)
    parser.add_argument("--out", default=os.path.join(os.path.dirname(__file__), "..", "..", "testdata", "java"))
    options = parser.parse_args()
    git = Git(options.repo, options.rev)
    levels_src = git.show(LEVELS).decode()
    enum_body = levels_src[levels_src.index("enum class LanguageLevel {"):]
    max_level = re.findall(r"^\s*(JDK_\w+)\(", enum_body[:enum_body.index(";")], re.M)[-1]

    data_files = set(git.ls(DATA + "parser-full") + git.ls(DATA + "parser-partial"))
    manifest = []
    used = set()
    unmatched = []
    for path in git.ls(ABSTRACT):
        cls = os.path.basename(path)[:-len(".java")]
        if not cls.startswith("AbstractBasic") or not path.endswith("Test.java"):
            continue
        src = git.show(path).decode()
        data_dir = re.search(r'super\("(parser-[^"]+)"', src).group(1)
        partial = data_dir.startswith("parser-partial")
        for method, level, call, args in test_methods(src):
            name = test_name(method)
            if call is None:
                unmatched.append(f"{cls}.{method}: compares no dump")
                continue
            base = f"{DATA}{data_dir}/{name}"
            if base + ".txt" not in data_files:
                unmatched.append(f"{cls}.{method}: no {name}.txt")
                continue
            if call == "doTest" and args == [False]:
                unmatched.append(f"{cls}.{method}: doTest(false) does not compare {name}.txt")
                continue
            if partial:
                key = (cls, call, len(args))
                entry = PARTIAL_ENTRIES[key](args)
                text = git.show(base + ".java") if not args else args[0].encode()
                trim = 1 if not args else 0
            else:
                assert call == "doTest" and args == [True], (cls, method, call, args)
                entry, trim = "file", 1
                text = git.show(base + ".java")
            rel = f"{data_dir}/{name}"
            target = os.path.join(options.out, rel)
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target + ".java", "wb") as f:
                f.write(text)
            used.add(base + ".java")
            for suffix in (".txt", "_node.txt"):
                if base + suffix in data_files:
                    with open(target + suffix, "wb") as f:
                        f.write(git.show(base + suffix))
                    used.add(base + suffix)
            manifest.append(f"{rel}.java\t{level or max_level}\t{entry}\t{trim}\t{cls}.{method}")
    manifest.sort()
    with open(os.path.join(options.out, "manifest.tsv"), "w") as f:
        f.write("".join(line + "\n" for line in manifest))
    left = sorted(p for p in data_files - used if "/jshell/" not in p)
    print(f"vendored {len(manifest)} tests ({sum(1 for l in manifest if l.split(chr(9))[2] == 'file')} full, "
          f"{sum(1 for l in manifest if l.split(chr(9))[2] != 'file')} partial); default level {max_level}")
    print(f"test methods without a golden: {len(unmatched)}")
    for line in unmatched:
        print("  " + line)
    print(f"data files no test reads (jshell excluded): {len(left)}")
    for p in left:
        print("  " + p[len(DATA):])


if __name__ == "__main__":
    sys.exit(main())
