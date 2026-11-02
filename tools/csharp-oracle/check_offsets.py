#!/usr/bin/env python3
"""Checks a C# oracle dump directory against the source bytes: `check_offsets.py LIST ROOT DUMPS [SUFFIX]`
(`check_offsets.py testdata/csharp/manifest.tsv testdata/csharp testdata/csharp` for the goldens).

For every dump of the list (SUFFIX `.tree.txt`, or `.lex.txt` for `lex --full`): each token and unstructured trivia
line's `[s..e)` slice of the file's bytes is valid UTF-8 and equals its unescaped `'text'`, a missing token is
empty, every node's range holds its children's, and the dump's last line before the diagnostics ends at the file's
length. That is the UTF-16 to UTF-8 offset mapping checked on every line. Prints the counts and the first few
mismatches; exits 1 on any."""
import os
import re
import sys

LINE = re.compile(r"^( *)(L:|T:)?(\w+) \[(\d+)\.\.(\d+)\)(?: (missing)|(?: '(.*)'))?$", re.S)
ESCAPE = re.compile(r"\\(u\{([0-9A-F]{4})\}|.)")


def unescape(s):
    def one(m):
        if m.group(2):
            return chr(int(m.group(2), 16))
        return {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", "'": "'"}[m.group(1)]

    return ESCAPE.sub(one, s)


def check(data, dump, problems, path):
    lines = dump.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    if "Diagnostics:" in lines:
        lines = lines[: lines.index("Diagnostics:")]
    stack = []  # (depth, start, end) of open nodes
    checked = 0
    end_max = 0
    for no, line in enumerate(lines, 1):
        m = LINE.match(line)
        if not m:
            problems.append(f"{path}:{no}: unparsed line {line[:80]!r}")
            return checked
        depth, trivia, kind, s, e, missing, text = len(m.group(1)) // 2, m.group(2), m.group(3), int(m.group(4)), int(m.group(5)), m.group(6), m.group(7)
        end_max = max(end_max, e)
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if stack and not (stack[-1][1] <= s <= e <= stack[-1][2]):
            problems.append(f"{path}:{no}: [{s}..{e}) outside its parent [{stack[-1][1]}..{stack[-1][2]})")
        if not (0 <= s <= e <= len(data)):
            problems.append(f"{path}:{no}: [{s}..{e}) outside the file ({len(data)} bytes)")
            continue
        if missing:
            if s != e:
                problems.append(f"{path}:{no}: a missing token spans [{s}..{e})")
        elif text is not None:
            try:
                actual = data[s:e].decode("utf-8")
            except UnicodeDecodeError:
                problems.append(f"{path}:{no}: [{s}..{e}) splits a character")
                continue
            if actual != unescape(text):
                problems.append(f"{path}:{no}: [{s}..{e}) is {actual[:40]!r}, the line says {unescape(text)[:40]!r}")
            checked += 1
        else:
            stack.append((depth, s, e))
    if end_max != len(data) and lines and LINE.match(lines[0]) and LINE.match(lines[0]).group(3) == "CompilationUnit":
        problems.append(f"{path}: the dump ends at {end_max}, the file at {len(data)}")
    return checked


def main():
    list_path, root, dumps = sys.argv[1], sys.argv[2], sys.argv[3]
    suffix = sys.argv[4] if len(sys.argv) > 4 else ".tree.txt"
    problems, files, lines = [], 0, 0
    for row in open(list_path, encoding="utf-8"):
        path = row.rstrip("\n").split("\t")[0]
        if not path:
            continue
        # A manifest line of testdata/csharp names NAME: the input NAME.cs, the dump NAME.tree.txt.
        source, dump_path = (path, path + suffix) if path.endswith(".cs") else (path + ".cs", path + ".tree.txt")
        dump_path = os.path.join(dumps, dump_path)
        if not os.path.exists(dump_path):
            continue
        data = open(os.path.join(root, source), "rb").read()
        lines += check(data, open(dump_path, encoding="utf-8").read(), problems, path)
        files += 1
    print(f"{files} dumps, {lines} token and trivia texts at their offsets, {len(problems)} mismatches")
    for p in problems[:20]:
        print("  " + p)
    sys.exit(1 if problems else 0)


main()
