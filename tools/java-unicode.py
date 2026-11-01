#!/usr/bin/env python3
"""Writes crates/java-syntax/src/unicode.rs: the non-ASCII members of `[:jletter:]` and `[:jletterdigit:]` of
_JavaLexer.flex (`[:jletter:]` of _JavaDocLexer.flex is checked to be the same set), decoded from the DFA tables JFlex generated into
_JavaLexer.kt and _JavaDocLexer.kt (java/java-syntax/gen at the revision of AGENTS.md).

A code point's membership is not read from a character class number but from the DFA itself: for each class the
script runs the generated tables on a representative and asks whether it starts an identifier (`x` lexed alone is
IDENTIFIER), continues one (`a` + `x` is one IDENTIFIER), and, for the JavaDoc lexer, whether it is `{ALPHA}` (lexed
alone in DOC_TAG_VALUE it is DOC_TAG_VALUE_TOKEN). It also checks the hand-classified ASCII range of the lexers and
prints the classes of the line terminators JFlex's `.` excludes.

    tools/java-unicode.py GEN_DIR > crates/java-syntax/src/unicode.rs

GEN_DIR holds _JavaLexer.kt and _JavaDocLexer.kt, e.g. from
`git -C idea show REV:community/java/java-syntax/gen/com/intellij/java/syntax/lexer/_JavaLexer.kt`.
"""

import re
import sys
from pathlib import Path


def packed_strings(source: str, name: str) -> list[str]:
    """The packed string constants NAME_PACKED_0, _1, ... in order, decoded."""
    out = []
    for i in range(100):
        m = re.search(r"private const val %s_PACKED_%d: String\s*=\s*((?:\"[^\"]*\"\s*\+?\s*)+)" % (name, i), source)
        if not m:
            break
        parts = re.findall(r"\"([^\"]*)\"", m.group(1))
        text = "".join(parts)
        out.append(re.sub(r"\\u([0-9a-fA-F]{4})", lambda x: chr(int(x.group(1), 16)), text))
    if not out:
        raise SystemExit(f"no {name} in source")
    return out


def unpack_rle(packed: list[str]) -> list[int]:
    result = []
    for s in packed:
        for i in range(0, len(s), 2):
            result.extend([ord(s[i + 1])] * ord(s[i]))
    return result


def unpack_trans(packed: list[str]) -> list[int]:
    result = []
    for s in packed:
        for i in range(0, len(s), 2):
            result.extend([ord(s[i + 1]) - 1] * ord(s[i]))
    return result


def unpack_rowmap(packed: list[str]) -> list[int]:
    result = []
    for s in packed:
        for i in range(0, len(s), 2):
            result.append((ord(s[i]) << 16) | ord(s[i + 1]))
    return result


class Dfa:
    def __init__(self, path: Path):
        src = path.read_text()
        self.top = unpack_rle(packed_strings(src, "ZZ_CMAP_TOP"))
        self.blocks = unpack_rle(packed_strings(src, "ZZ_CMAP_BLOCKS"))
        self.action = unpack_rle(packed_strings(src, "ZZ_ACTION"))
        self.rowmap = unpack_rowmap(packed_strings(src, "ZZ_ROWMAP"))
        self.trans = unpack_trans(packed_strings(src, "ZZ_TRANS"))
        self.attr = unpack_rle(packed_strings(src, "ZZ_ATTRIBUTE"))
        m = re.search(r"ZZ_LEXSTATE: IntArray = intArrayOf\(([^)]*)\)", src)
        self.lexstate = [int(x) for x in m.group(1).replace("\n", " ").split(",") if x.strip()]
        self.states = {k: int(v) for k, v in re.findall(r"public const val (\w+): Int = (\d+)", src)}

    def cmap(self, cp: int) -> int:
        offset = cp & 0xFF
        return self.blocks[offset] if offset == cp else self.blocks[self.top[cp >> 8] | offset]

    def scan(self, cps: list[int], lexical_state: int = 0) -> tuple[int, int]:
        """(action number, matched length in code points) of the longest match at the start of `cps`."""
        state = self.lexstate[lexical_state]
        action, marked = -1, 0
        if self.attr[state] & 1:
            action = state
        for i, cp in enumerate(cps):
            nxt = self.trans[self.rowmap[state] + self.cmap(cp)]
            if nxt == -1:
                break
            state = nxt
            if self.attr[state] & 1:
                action, marked = state, i + 1
                if self.attr[state] & 8:
                    break
        return (self.action[action] if action >= 0 else -1), marked


def classes(dfa: Dfa) -> dict[int, list[int]]:
    """Representatives: class number -> code points (all of them)."""
    out: dict[int, list[int]] = {}
    for cp in range(0x110000):
        out.setdefault(dfa.cmap(cp), []).append(cp)
    return out


def ranges(cps: list[int]) -> list[tuple[int, int]]:
    out = []
    for cp in sorted(cps):
        if out and out[-1][1] + 1 == cp:
            out[-1] = (out[-1][0], cp)
        else:
            out.append((cp, cp))
    return out


def emit(name: str, doc: str, cps: list[int]) -> str:
    rs = ranges([c for c in cps if c >= 0x80])
    lines = [f"/// {doc}", f"pub(crate) static {name}: [(u32, u32); {len(rs)}] = ["]
    lines += [f"    (0x{a:X}, 0x{b:X})," for a, b in rs]
    lines.append("];")
    return "\n".join(lines)


def main() -> None:
    gen = Path(sys.argv[1])
    java = Dfa(gen / "_JavaLexer.kt")
    doc = Dfa(gen / "_JavaDocLexer.kt")

    ident = java.scan([ord("a")])[0]
    jletter, jletterdigit = [], []
    for cls, cps in classes(java).items():
        rep = cps[0]
        start = java.scan([rep]) == (ident, 1)
        part = java.scan([ord("a"), rep]) == (ident, 2)
        if start:
            jletter.extend(cps)
        if part:
            jletterdigit.extend(cps)
        if start and not part:
            raise SystemExit(f"class {cls}: starts but does not continue an identifier")

    # In DOC_TAG_VALUE_IN_LTGT a lone {ALPHA} is an {IDENTIFIER}, DOC_TAG_VALUE_TOKEN; a digit, `:`, `.` or `-` alone
    # is not.
    in_ltgt = doc.states["DOC_TAG_VALUE_IN_LTGT"]
    value_token = doc.scan([ord("a")], in_ltgt)[0]
    doc_alpha = []
    for cls, cps in classes(doc).items():
        if cps != [ord("?")] and doc.scan([cps[0]], in_ltgt) == (value_token, 1):
            doc_alpha.extend(cps)

    if sorted(doc_alpha) != sorted(jletter):
        raise SystemExit("[:jletter:] of _JavaDocLexer.flex differs from _JavaLexer.flex's: emit both")

    # The ASCII members the lexers classify by hand.
    ascii_start = sorted(c for c in jletter if c < 0x80)
    ascii_part = sorted(c for c in jletterdigit if c < 0x80)
    print("// ascii jletter:", "".join(chr(c) if 32 < c < 127 else f"\\x{c:02x}" for c in ascii_start), file=sys.stderr)
    print("// ascii jletterdigit:", "".join(chr(c) if 32 < c < 127 else f"\\x{c:02x}" for c in ascii_part), file=sys.stderr)
    print("// doc alpha ascii:", "".join(chr(c) for c in doc_alpha if c < 0x80), file=sys.stderr)
    print("// doc alpha == java jletter:", sorted(doc_alpha) == sorted(jletter), "above ASCII:",
          sorted(c for c in doc_alpha if c >= 0x80) == sorted(c for c in jletter if c >= 0x80), file=sys.stderr)
    for name in ("COMMENT_DATA", "DOC_TAG_VALUE", "INLINE_TAG_LITERAL_SPACE"):
        st = doc.states[name]
        print(f"// {name}: x ->", doc.scan([ord("x")], st), "per terminator:",
              {hex(c): doc.scan([c], st) for c in (0x0A, 0x0B, 0x0C, 0x0D, 0x85, 0x2028, 0x2029)}, file=sys.stderr)

    print('''//! Unicode tables of the two JFlex lexers of java-syntax, decoded from the DFA tables JFlex 1.10.18 generated into
//! `_JavaLexer.kt` (`[:jletter:]`, `[:jletterdigit:]` of _JavaLexer.flex) and `_JavaDocLexer.kt` (`[:jletter:]` of
//! _JavaDocLexer.flex, the same set as _JavaLexer.flex's: the script checks it), so identifiers split exactly where
//! IntelliJ's lexers split them. Code points below 0x80 are classified by hand in the lexers; these ranges hold only
//! the non-ASCII members, sorted and disjoint. Generated by `tools/java-unicode.py`; do not edit by hand.
''')
    print(emit("JAVA_LETTER", "`[:jletter:]` of _JavaLexer.flex above ASCII.", jletter))
    print()
    print(emit("JAVA_LETTER_DIGIT", "`[:jletterdigit:]` of _JavaLexer.flex above ASCII.", jletterdigit))


if __name__ == "__main__":
    main()
