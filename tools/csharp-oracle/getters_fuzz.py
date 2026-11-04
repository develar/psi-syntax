#!/usr/bin/env python3
"""Fuzz inputs for the syntax getters (`getters` mode, crates/csharp-syntax/src/syntax_getters): texts on which Roslyn's
recovery fills slots with what Syntax.xml does not name (`event A.this`, a `;` as a separated list's separator), so
that the port's assignment of children to slots (`syntax_getters/slots.rs`) meets them. Half are mixes of declaration,
name and expression pieces, half vendored golden inputs with one to six such pieces inserted; each parsed at one of
the entries a member, a statement or an expression is read at. Deterministic for a seed; every file is valid UTF-8.

    tools/csharp-oracle/getters_fuzz.py testdata/csharp OUT_DIR [COUNT] [SEED]

writes OUT_DIR/NNNNN.cs and OUT_DIR/list (`PATH<TAB><TAB>ENTRY`); compare `oracle.sh getters --list OUT_DIR/list
--root OUT_DIR` with `ksdump --lang csharp --getters --check`.
"""

import random
import sys
from pathlib import Path

PIECES = [
    # declarations and their parts
    "class ", "struct ", "record ", "record struct ", "record class ", "interface ", "enum ", "delegate ", "union ",
    "extension", "extension(int i) ", "namespace ", "namespace A.B;", "using ", "global using ", "using static ",
    "using unsafe ", "extern alias ", "alias ", "static ", "public ", "private ", "protected ", "internal ", "file ",
    "partial ", "sealed ", "abstract ", "virtual ", "override ", "new ", "readonly ", "ref ", "required ", "scoped ",
    "unsafe ", "extern ", "const ", "volatile ", "async ", "params ", "this ", "in ", "out ", "event ", "operator ",
    "implicit ", "explicit ", "checked ", "get", "set", "init", "add", "remove", "where ", "allows ", "unmanaged",
    "notnull", "default", "__arglist", "~", "[A] ", "[assembly: B] ", "[return: C] ", "[field: D(1)] ",
    # names and types
    "A", "b", "@c", "_", "T", "I.", "global::", "E::", "A.B", "List<int>", "Dictionary<,>", "int ", "var ", "dynamic ",
    "int*", "int?", "int[]", "int[,]", "(int, string)", "(int a, int b)", "delegate*<int, void>", "nint ", "nuint ",
    # punctuation and expressions
    "::", ".", "?.", ",", ";", ":", "=", "=>", "(", ")", "[", "]", "{", "}", "<", ">", "?", "*", "!", "&", "..",
    "1", "\"s\"", "$\"{x}\"", "x => x", "async x => x", "delegate { }", "new() ", "new A(1) { }", "a.b.c()", "a?.b",
    "typeof(A<>)", "default(int)", "x switch { _ => 1 }", "is ", "as ", "with { }", "{ X: 1 }", "[1, ..xs]",
    # trivia
    "#if X\n", "#endif\n", "#region\n", "#pragma warning disable 1\n", "/// <summary>a</summary>\n", "// c\n",
    "/* d */", "\n", " ",
]

ENTRIES = ["compilation-unit", "compilation-unit", "member", "statement", "expression"]


def mix(rng: random.Random) -> str:
    return "".join(rng.choice(PIECES) for _ in range(rng.randint(1, 60)))


def insert(text: str, rng: random.Random) -> str:
    for _ in range(rng.randint(1, 6)):
        pos = rng.randint(0, len(text))
        text = text[:pos] + rng.choice(PIECES) + text[pos:]
    return text


def main() -> None:
    goldens = Path(sys.argv[1])
    out = Path(sys.argv[2])
    count = int(sys.argv[3]) if len(sys.argv) > 3 else 20000
    seed = sys.argv[4] if len(sys.argv) > 4 else "csharp-getters"
    rng = random.Random(seed)
    texts = [p.read_text(encoding="utf-8") for p in sorted(goldens.rglob("*.cs"))]
    out.mkdir(parents=True, exist_ok=True)
    lines = []
    for i in range(count):
        name = f"{i:05d}.cs"
        text = mix(rng) if i % 2 == 0 else insert(rng.choice(texts), rng)
        (out / name).write_bytes(text.encode("utf-8"))
        lines.append(f"{name}\t\t{rng.choice(ENTRIES)}\n")
    (out / "list").write_text("".join(lines))


if __name__ == "__main__":
    main()
