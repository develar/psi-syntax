#!/usr/bin/env python3
"""Checks a C# oracle dump directory against the source bytes: `check_offsets.py LIST ROOT DUMPS [SUFFIX]`
(`check_offsets.py testdata/csharp/manifest.tsv testdata/csharp testdata/csharp` for the goldens).

For every dump of the list (SUFFIX `.tree.txt`, or `.lex.txt` for `lex --full`): each token and unstructured trivia
line's `[s..e)` slice of the file's bytes is valid UTF-8 and equals its unescaped `'text'`, a missing token is
empty, every node's range holds its children's, and a compilation unit ends at the file's length. That is the UTF-16
to UTF-8 offset mapping checked on every line. A token or trivia of half a surrogate pair (Roslyn makes them) is
checked within its whole character; a partial entry whose root does not spell the text (Roslyn dropped some) is
counted apart, not checked. Prints the counts and the first few
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


def same_text(data, s, e, text):
    """Whether data[s:e] spells text, where s or e may fall between the two halves of a supplementary character
    (2 of its 4 bytes each): Roslyn's lexer makes a token or a trivia of half a pair, whose text is one surrogate."""
    head = s > 0 and s < len(data) and 0x80 <= data[s] < 0xC0
    tail = e < len(data) and 0x80 <= data[e] < 0xC0
    try:
        whole = data[s - 2 if head else s:e + 2 if tail else e].decode("utf-8")
    except UnicodeDecodeError:
        return None
    units = whole.encode("utf-16-le", "surrogatepass")
    units = units[2 if head else 0:len(units) - (2 if tail else 0)]
    return units == text.encode("utf-16-le", "surrogatepass")


def check(data, dump, problems, path, lossy):
    lines = dump.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    if "Diagnostics:" in lines:
        lines = lines[: lines.index("Diagnostics:")]
    first = LINE.match(lines[0]) if lines else None
    if (first and first.group(3) != "CompilationUnit" and not first.group(2) and first.group(6) is None
            and first.group(7) is None and int(first.group(5)) != len(data)):
        # A partial entry's root that does not spell the text: Roslyn dropped some (seen in ParseMemberDeclaration
        # of `short required implicit operator C(S s) {}`, which loses `required `). The dump is still Roslyn's.
        lossy.append(f"{path}: the root covers [{first.group(4)}..{first.group(5)}) of {len(data)} bytes")
        return 0
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
            ok = same_text(data, s, e, unescape(text))
            if ok is None:
                problems.append(f"{path}:{no}: [{s}..{e}) is not UTF-8 even widened to whole characters")
            elif not ok:
                actual = data[s:e].decode("utf-8", "replace")
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
    problems, lossy, files, lines = [], [], 0, 0
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
        lines += check(data, open(dump_path, encoding="utf-8", errors="surrogateescape").read(), problems, path, lossy)
        files += 1
    print(f"{files} dumps, {lines} token and trivia texts at their offsets, {len(problems)} mismatches, "
          f"{len(lossy)} partial roots that do not spell their text")
    for p in lossy[:5]:
        print("  " + p)
    for p in problems[:20]:
        print("  " + p)
    sys.exit(1 if problems else 0)


main()
