#!/usr/bin/env python3
"""Lexer fuzz inputs for the C# lexer: each output file is a vendored golden input with a few characters or snippets
inserted, replaced or deleted at random places, drawn from what changes the lexer's states (string, verbatim, raw
and interpolated string openers and closers, braces, escapes and unicode escapes, character literals, comment and
doc comment pieces, XML of doc comments, preprocessor directives, numbers and their suffixes, line breaks and
Unicode whitespace, non-ASCII and supplementary letters). Deterministic for a seed; every file is valid UTF-8.

    tools/csharp-oracle/lex_fuzz.py testdata/csharp OUT_DIR [COUNT] [SEED]

writes OUT_DIR/NNNNN.cs and OUT_DIR/list; compare `oracle.sh lex --list OUT_DIR/list --root OUT_DIR` with
`ksdump --lex --lang csharp --list OUT_DIR/list --root OUT_DIR`, and the dumps alike.
"""

import random

import golden_fuzz

PIECES = [
    # strings: regular, verbatim, raw, interpolated, UTF-8
    "\"", "'", "@\"", "$\"", "$@\"", "@$\"", "\"\"", "\"\"\"", "\"\"\"\n", "\"\"\"\"", "$\"\"\"", "$$\"\"\"",
    "$$$\"\"\"\n", "\"\"u8", "u8", "U8", "{", "}", "{{", "}}", "{x}", "{x,5}", "{x:N2}", "{{x}}", "{\"s\"}", "{$\"\"}",
    # escapes
    "\\", "\\\\", "\\u", "\\u0041", "\\u00", "\\U0001F600", "\\U00110000", "\\U0000", "\\x", "\\x41", "\\x4142a",
    "\\u005c", "\\u0022", "\\u0027", "\\ud800", "\\e", "\\0", "\\q",
    # comments and doc comments
    "/", "*", "/*", "*/", "/**", "/**/", "//", "///", "///\n", "/// <summary>", "</summary>", "<see cref=\"",
    "\"/>", "<param name=\"", "<!--", "-->", "<![CDATA[", "]]>", "&amp;", "&#x41;", "&lt;", "<", ">", "</", "/>",
    # preprocessor
    "#", "#if ", "#if X\n", "#if !(A && B)\n", "#else\n", "#elif ", "#elif true\n", "#endif\n", "#region ",
    "#endregion\n", "#define X\n", "#undef X\n", "#pragma warning disable CS0168\n", "#pragma checksum ",
    "#line 1 \"f\"\n", "#line (1,1)-(2,2) 3 \"f\"\n", "#line hidden\n", "#nullable enable\n", "#error x\n",
    "#warning y\n", "#r \"a\"\n", "#load \"b\"\n", "#!", "#:", "#:property x\n",
    # numbers
    "0", "1", "0x", "0b", "0x1F", "0b_1", "1e", "1e+", "1.", ".5", "1.5e-3", "1_000", "1__0", "1UL", "1lu", "1Lu",
    "1m", "1f", "1d", "0x_FF", "9999999999999999999999",
    # line breaks and whitespace
    "\n", "\r\n", "\r", "\t", "\x0b", "\x0c", " ", "\u0085", " ", " ", " ", "﻿", "　",
    "​", "\x00", "\x01", "\x1a", "\x7f",
    # identifiers and punctuation
    "@", "@if", "@class", "_", "$", "é", "ß", "ǅ", "名", "̀", "٠", "\U0001F600", "\U0001D49C", "\U00020000",
    "=>", "??=", ">>>=", ">>", "..", "::", "?.", "(", ")", "[", "]", ";", ",", "class", "record", "var", "await",
    "yield", "field", "extension",
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


if __name__ == "__main__":
    golden_fuzz.write(5000, "csharp-lex", lambda i, texts, rng: (mutate(rng.choice(texts), rng), ""))
