#!/usr/bin/env python3
"""A corpus list with preprocessor symbols: `defines.py LIST ROOT > OUT.list`.

Of LIST's files (paths relative to ROOT, the first column), those whose `#if` and `#elif` conditions name a
symbol, each as `PATH<TAB><TAB><TAB><TAB>SYMBOLS`: SYMBOLS every identifier those conditions name, in order of first
appearance, separated by `;` (`true` and `false` are keywords, not names). That is the file as a consumer parses it
that defines every symbol a file's conditions name: the regions disabled at the defaults are active, the `#else`
branches disabled. A directive is a line whose first non-blank character is `#` (blanks may follow it), its condition
the rest of the line before a `//`; `\\uXXXX` and `\\UXXXXXXXX` escapes are decoded, as an identifier's value is. A
line inside a string or a comment counts too: the list is a test configuration, not the file's own.
"""
import re
import sys

DIRECTIVE = re.compile(r"^[ \t\f\v]*#[ \t\f\v]*(?:if|elif)(?![\w])(.*)$")
ESCAPE = re.compile(r"\\u([0-9A-Fa-f]{4})|\\U([0-9A-Fa-f]{8})")
NAME = re.compile(r"[^\W\d]\w*")


def unescape(m):
    code = int(m.group(1) or m.group(2), 16)
    return chr(code) if code <= 0x10FFFF and not 0xD800 <= code <= 0xDFFF else m.group(0)


def symbols(text):
    """The names of the `#if` and `#elif` conditions of `text`, in order."""
    names = []
    for line in text.split("\n"):
        m = DIRECTIVE.match(line.rstrip("\r"))
        if not m:
            continue
        condition = ESCAPE.sub(unescape, m.group(1).split("//", 1)[0])
        for name in NAME.findall(condition):
            if name not in ("true", "false") and name not in names:
                names.append(name)
    return names


def main():
    list_path, root = sys.argv[1], sys.argv[2]
    out = []
    for row in open(list_path, encoding="utf-8"):
        path = row.rstrip("\n").split("\t")[0]
        if not path:
            continue
        try:
            text = open(f"{root}/{path}", encoding="utf-8", newline="").read()
        except (OSError, UnicodeDecodeError):
            continue
        names = symbols(text)
        if names:
            out.append(f"{path}\t\t\t\t{';'.join(names)}\n")
    sys.stdout.write("".join(out))


if __name__ == "__main__":
    main()
