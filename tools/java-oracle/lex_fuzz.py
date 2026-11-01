#!/usr/bin/env python3
"""Lexer fuzz inputs for the Java and JavaDoc lexers: each output file is a vendored golden input with a few
characters or snippets inserted, replaced or deleted at random places, drawn from what changes the lexers' states
(unicode escapes, backslashes, quotes, text blocks, string templates, comment openers and closers, the JavaDoc tag and
markdown characters, non-ASCII letters and whitespace, a lone surrogate's escape). Deterministic for a seed.

    tools/java-oracle/lex_fuzz.py testdata/java OUT_DIR [COUNT] [SEED]

writes OUT_DIR/NNNNN.java and OUT_DIR/list; compare `JavaLexOracle.java --list OUT_DIR/list --root OUT_DIR` with
`ksdump --lex --list OUT_DIR/list --root OUT_DIR`.
"""

import random
import sys
from pathlib import Path

PIECES = [
    "\\", "\\\\", "\\u", "\\u0041", "\\uuu005c", "\\u005c", "\\u0022", "\\u0027", "\\u000a", "\\u001a", "\\u007b",
    "\\u007d", "\\u002f", "\\u002a", "\\ud800", "\\{", "\\}", "\"", "'", "\"\"\"", "\"\"\"\n", "\n", "\r\n", "\t",
    "\x0c", " ", "\x1a", "{", "}", "/", "*", "/*", "*/", "/**", "/**/", "//", "///", "///\n", "#", "#!", "@", "@param ",
    "@see ", "@value ", "@code ", "@literal ", "@snippet ", "@return ", "{@", "{@code ", "{@link ", "{@snippet :\n",
    "<", ">", "<T>", "(", ")", "[", "]", ",", ".", "..", "...", "-", "--", "->", "::", "%d", "%", "`", "```", "~~~", "=",
    ":", "?", "!", "1", "0x", "0b", "1e", "1.", ".5", "0x1p", "1L", "_", "$", "é", "ß", "ǅ", " ", " ",
    "　", "̀", "\U0001F600", "\x00", "\x01", "\x7f", "assert", "enum", "non-sealed", "record", "\\[", "\\]",
    "\\`", "\\(", "\\)",
]


def mutate(text: str, rng: random.Random) -> str:
    for _ in range(rng.randint(1, 4)):
        pos = rng.randint(0, len(text))
        op = rng.random()
        if op < 0.6:
            text = text[:pos] + rng.choice(PIECES) + text[pos:]
        elif op < 0.85:
            end = min(len(text), pos + rng.randint(1, 3))
            text = text[:pos] + rng.choice(PIECES) + text[end:]
        else:
            end = min(len(text), pos + rng.randint(1, 5))
            text = text[:pos] + text[end:]
    return text


def main() -> None:
    goldens = Path(sys.argv[1])
    out = Path(sys.argv[2])
    count = int(sys.argv[3]) if len(sys.argv) > 3 else 5000
    seed = sys.argv[4] if len(sys.argv) > 4 else "java-lex"
    rng = random.Random(seed)
    sources = sorted(p for p in goldens.rglob("*.java"))
    texts = [p.read_text(encoding="utf-8") for p in sources]
    out.mkdir(parents=True, exist_ok=True)
    names = []
    for i in range(count):
        name = f"{i:05d}.java"
        (out / name).write_bytes(mutate(rng.choice(texts), rng).encode("utf-8", "surrogatepass"))
        names.append(name)
    (out / "list").write_text("".join(n + "\n" for n in names))


if __name__ == "__main__":
    main()
