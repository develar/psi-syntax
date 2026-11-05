#!/usr/bin/env python3
"""One-place mutations of the C# goldens for the oracle differential.

    tools/csharp-oracle/mutate.py [--goldens testdata/csharp] [--per-file 2] [--seed csharp-mut] OUT_DIR

`java-oracle mutate` (crates/ide-oracle/src/bin/java-oracle/mutate.rs) for C#. Each operator applies to every input, not to hand-picked cases, so a gap shows
wherever the operator's construct is. A mutant changes one place of its original (`--per-file N` places per
operator and file, picked by a generator seeded with the seed, the file and the operator), so a gap is not hidden
behind an error the file has elsewhere. The inputs are the goldens of testdata/csharp/manifest.tsv whose dump has
no diagnostic, every entry, each copied under `original/`. OUT_DIR/origins.json maps every mutant to its original
and OUT_DIR/list is an oracle list: `PATH<TAB>VERSION<TAB>ENTRY<TAB>DOC[<TAB>SYMBOLS]`, the golden's (SYMBOLS only
when it defines any). Every text is valid UTF-8.
"""
import argparse
import json
import os
import random
import re

KEYWORDS = ["abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked", "class", "const",
            "continue", "decimal", "default", "delegate", "do", "double", "else", "enum", "event", "explicit",
            "extern", "false", "finally", "fixed", "float", "for", "foreach", "goto", "if", "implicit", "in", "int",
            "interface", "internal", "is", "lock", "long", "namespace", "new", "null", "object", "operator", "out",
            "override", "params", "private", "protected", "public", "readonly", "ref", "return", "sbyte", "sealed",
            "short", "sizeof", "stackalloc", "static", "string", "struct", "switch", "this", "throw", "true", "try",
            "typeof", "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "virtual", "void", "volatile",
            "while"]
# Contextual keywords of SyntaxKindFacts.GetContextualKeywordKind.
SOFT = ["add", "alias", "allows", "and", "args", "ascending", "async", "await", "by", "descending", "dynamic",
        "equals", "extension", "field", "file", "from", "get", "global", "group", "init", "into", "join", "let",
        "managed", "nameof", "nint", "not", "notnull", "nuint", "on", "or", "orderby", "partial", "record", "remove",
        "required", "scoped", "select", "set", "unmanaged", "value", "var", "when", "where", "with", "yield", "_"]
KEYWORD_SET = set(KEYWORDS)
OPERATORS = [">>>=", ">>=", "<<=", "??=", "...", "=>", "->", "::", "++", "--", "&&", "||", "==", "!=", "<=", ">=",
             "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", "??", "?.", ".."]
PIECES = ["(", ")", "{", "}", "[", "]", "<", ">", ",", ":", ";", "=", "=>", ".", "..", "::", "?", "?.", "??", "!",
          "@", "[A] ", "*", "&", "&&", "^", "~", "is ", "as ", "new ", "class ", "struct ", "record ", "interface ",
          "enum ", "delegate ", "namespace ", "using ", "if ", "else ", "for ", "foreach ", "while ", "do ", "try ",
          "catch ", "finally ", "switch ", "case ", "default ", "return ", "throw ", "break ", "continue ", "goto ",
          "yield ", "await ", "async ", "var ", "ref ", "out ", "in ", "this", "base", "null", "static ", "public ",
          "partial ", "where ", "when ", "with ", "and ", "or ", "not ", "from ", "select ", "let ", "x", "1", "1L",
          "0x", "1e", "1.0m", '"', '@"', '$"', '"""', "'", "<T>", "(int a)", "() => {}", "int ", "void ", "int[] ",
          "int? ", "int* ", "delegate*<void> ", "#", "#if X\n", "#endif\n", "#region\n", "\\", "/**", "/*", "//",
          "///", "\n", "{x}", '$$"""', "stackalloc ", "checked ", "operator ", "implicit ", "extension "]
CHAR_CONTENTS = ["\\q", "\\u0000", "ab", "", "a\nb", "\\", "'", "\\U00010000", "\\x41", "\\777", "\\U0001F600"]
STRING_EDITS = [("unclosed", lambda s: s[:-1]), ("break", lambda s: s[:len(s) // 2] + "\n" + s[len(s) // 2:]),
                ("bad-escape", lambda s: s[:-1] + "\\q" + s[-1]), ("raw-open", lambda s: '""' + s + '""'),
                ("interpolated", lambda s: "$" + s), ("verbatim", lambda s: "@" + s)]
BAD_NAMES = [("emoji-suffix", lambda n: n + "\U0001F4A9"), ("digit-start", lambda n: "١" + n),
             ("unicode-escape", lambda n: "\\u%04x" % ord(n[0]) + n[1:]), ("verbatim", lambda n: "@" + n),
             ("supplementary-letter", lambda n: n + "\U0001D49C")]
DECLARATION_KEYWORDS = {"class", "struct", "interface", "enum", "record", "delegate", "void", "int", "bool",
                        "string", "namespace"}
MODIFIER_RUNS = [("private-twice", "private private "), ("attributes", "[A] [B(x)] [C.D(1, 2)] "),
                 ("all", "public protected internal private abstract static sealed virtual override readonly "
                         "volatile unsafe extern new async partial required file ref ")]

word_re = re.compile(r"@?[^\W\d]\w*", re.U)
number_re = re.compile(r"[0-9](?:[0-9a-zA-Z_]|\.(?=[0-9]))*|\.[0-9][0-9a-zA-Z_]*")


class Token:
    __slots__ = ("kind", "start", "end", "text", "line")

    def __init__(self, kind, start, end, text, line):
        self.kind, self.start, self.end, self.text, self.line = kind, start, end, text, line


def string_end(text, i):
    """The end of the string literal starting at i (after its `$`/`@` prefixes), approximately: raw strings to
    their closing quote run, verbatim ones with doubled quotes, interpolations with their braces balanced."""
    n = len(text)
    j = i
    dollars = verbatim = 0
    while j < n and text[j] in "$@":
        dollars += text[j] == "$"
        verbatim += text[j] == "@"
        j += 1
    quotes = 0
    while j + quotes < n and text[j + quotes] == '"':
        quotes += 1
    if quotes >= 3:
        close = text.find('"' * quotes, j + quotes)
        return n if close < 0 else close + quotes
    j += 1
    depth = 0
    while j < n:
        c = text[j]
        if dollars and c == "{":
            if text.startswith("{{", j) and depth == 0:
                j += 2
                continue
            depth += 1
        elif dollars and c == "}" and depth:
            depth -= 1
        elif depth == 0 and c == '"':
            if verbatim and text.startswith('""', j):
                j += 2
                continue
            return j + 1
        elif depth == 0 and c == "\\" and not verbatim:
            j += 2
            continue
        elif depth == 0 and c == "\n" and not verbatim:
            return j
        j += 1
    return n


def tokenize(text):
    """Tokens with offsets and lines; strings, characters, comments and directives are opaque."""
    tokens = []
    i, n = 0, len(text)
    line_start = True

    def push(kind, start, end):
        tokens.append(Token(kind, start, end, text[start:end], text.count("\n", 0, start) + 1))

    while i < n:
        c = text[i]
        if c == "\n":
            line_start = True
            i += 1
            continue
        if c.isspace():
            i += 1
            continue
        if c == "#" and line_start:
            while i < n and text[i] != "\n":
                i += 1
            continue
        line_start = False
        if text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
            continue
        start = i
        if c == '"' or c in "$@" and re.match(r'[$@]+"', text[i:i + 6]):
            i = string_end(text, i)
            push("string", start, i)
            continue
        if c == "'":
            i += 1
            while i < n and text[i] != "'" and text[i] != "\n":
                i += 2 if text[i] == "\\" else 1
            i = min(n, i + 1)
            push("char", start, i)
            continue
        m = word_re.match(text, i)
        if m:
            i = m.end()
            push("word", start, i)
            continue
        m = number_re.match(text, i)
        if m:
            i = m.end()
            push("number", start, i)
            continue
        op = next((o for o in OPERATORS if text.startswith(o, i)), None)
        i += len(op) if op else 1
        push("punct", start, i)
    return tokens


def edit(text, start, end, insert):
    return text[:start] + insert + text[end:]


def is_name(t):
    return t.kind == "word" and t.text not in KEYWORD_SET and not t.text.startswith("@")


def matching(tokens, k, open_, close):
    depth = 0
    for i in range(k, len(tokens)):
        if tokens[i].text == open_:
            depth += 1
        elif tokens[i].text == close:
            depth -= 1
            if depth == 0:
                return i
    return -1


def method_body(tokens, k):
    """For a `)` at k followed by `{`, the token range of that block, else None."""
    if tokens[k].text != ")" or k + 1 >= len(tokens) or tokens[k + 1].text != "{":
        return None
    end = matching(tokens, k + 1, "{", "}")
    return None if end < 0 else (k + 1, end)


def operators(text, tokens):
    """(name, sites, apply): apply(site, rng) returns [(variant, mutated text)]."""
    ops = []
    nxt = lambda k: tokens[k + 1] if k + 1 < len(tokens) else None
    sites = lambda pred: [k for k in range(len(tokens)) if pred(k)]
    opaque = lambda t: t.kind in ("string", "char")

    ops.append(("comment-between-tokens",
                sites(lambda k: nxt(k) is not None and not opaque(tokens[k])),
                lambda k, r: [("", edit(text, tokens[k].end, tokens[k].end, "/*c*/"))]))
    ops.append(("delete-token", sites(lambda k: True),
                lambda k, r: [("", edit(text, tokens[k].start, tokens[k].end, ""))]))
    ops.append(("duplicate-token", sites(lambda k: True),
                lambda k, r: [("", edit(text, tokens[k].end, tokens[k].end, " " + tokens[k].text))]))
    ops.append(("swap-tokens", sites(lambda k: nxt(k) is not None and tokens[k].text != nxt(k).text),
                lambda k, r: [("", text[:tokens[k].start] + tokens[k + 1].text
                               + text[tokens[k].end:tokens[k + 1].start] + tokens[k].text + text[tokens[k + 1].end:])]))
    ops.append(("truncate", sites(lambda k: nxt(k) is not None), lambda k, r: [("", text[:tokens[k].end])]))
    ops.append(("keyword-swap", sites(lambda k: tokens[k].text in KEYWORD_SET),
                lambda k, r: [("", edit(text, tokens[k].start, tokens[k].end, r.choice(KEYWORDS)))]))
    ops.append(("name-to-keyword", sites(lambda k: is_name(tokens[k])),
                lambda k, r: [(w, edit(text, tokens[k].start, tokens[k].end, w)) for w in r.sample(SOFT, 4)]
                + [("hard", edit(text, tokens[k].start, tokens[k].end, r.choice(KEYWORDS)))]))
    ops.append(("double-semi", sites(lambda k: tokens[k].text == ";"),
                lambda k, r: [(v, edit(text, tokens[k].end, tokens[k].end, more))
                              for v, more in (("", ";"), ("comment", "/*c*/;"), ("space", " ;"))]))
    ops.append(("modifier-run", sites(lambda k: tokens[k].text in DECLARATION_KEYWORDS
                                      and (k == 0 or tokens[k - 1].line != tokens[k].line)),
                lambda k, r: [(v, edit(text, tokens[k].start, tokens[k].start, run)) for v, run in MODIFIER_RUNS]))
    ops.append(("bad-name", sites(lambda k: is_name(tokens[k])),
                lambda k, r: [(v, edit(text, tokens[k].start, tokens[k].end, make(tokens[k].text)))
                              for v, make in BAD_NAMES]))
    ops.append(("char-content", sites(lambda k: tokens[k].kind == "char"),
                lambda k, r: [(str(i), edit(text, tokens[k].start, tokens[k].end, f"'{c}'"))
                              for i, c in enumerate(CHAR_CONTENTS)]))
    ops.append(("string-content", sites(lambda k: tokens[k].kind == "string" and len(tokens[k].text) >= 2),
                lambda k, r: [(v, edit(text, tokens[k].start, tokens[k].end, f(tokens[k].text)))
                              for v, f in STRING_EDITS]))
    ops.append(("drop-body", sites(lambda k: method_body(tokens, k) is not None),
                lambda k, r: [(v, edit(text, tokens[method_body(tokens, k)[0]].start,
                                       tokens[method_body(tokens, k)[1]].end, repl))
                              for v, repl in (("semicolon", ";"), ("arrow", "=> x;"), ("none", ""))]))
    ops.append(("insert-piece", sites(lambda k: True),
                lambda k, r: [("", edit(text, tokens[k].start, tokens[k].start,
                                        (lambda p: p if p.endswith((" ", "\n")) else p + " ")(r.choice(PIECES))))]))
    return ops


def rename(text, tokens, suffix):
    out, last = [], 0
    for t in tokens:
        if is_name(t) and t.text not in SOFT and re.match(r"[A-Za-z]", t.text):
            out.append(text[last:t.end] + suffix)
            last = t.end
    out.append(text[last:])
    return "".join(out)


def mutants_of(text, per_file, seed, rel):
    tokens = tokenize(text)
    out = []
    for name, site_list, apply in operators(text, tokens):
        rng = random.Random(f"{seed}:{rel}:{name}")
        picked = sorted(rng.sample(site_list, per_file)) if len(site_list) > per_file else site_list
        for index, site in enumerate(picked):
            for variant, mutated in apply(site, rng):
                if mutated != text:
                    out.append((f"{name}/{variant}" if variant else name, index, mutated))
    for name, suffix in (("unicode-latin", "é"), ("unicode-cjk", "名"), ("unicode-supplementary", "\U00020000")):
        mutated = rename(text, tokens, suffix)
        if mutated != text:
            out.append((name, 0, mutated))
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    parser.add_argument("--goldens", default=os.path.join(os.path.dirname(__file__), "..", "..", "testdata", "csharp"))
    parser.add_argument("--per-file", type=int, default=2)
    parser.add_argument("--seed", default="csharp-mut")
    options = parser.parse_args()
    lines, origins = [], {}
    inputs = skipped = 0

    def write(rel, text):
        dest = os.path.join(options.out, rel)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        with open(dest, "w", encoding="utf-8", newline="") as f:
            f.write(text)

    for row in open(os.path.join(options.goldens, "manifest.tsv"), encoding="utf-8"):
        name, version, entry, doc, symbols = row.rstrip("\n").split("\t")[:5]
        columns = f"{version}\t{entry}\t{doc}" + (f"\t{symbols}" if symbols else "")
        dump = open(os.path.join(options.goldens, name + ".tree.txt"), encoding="utf-8").read()
        if "\nDiagnostics:\n" in dump:
            skipped += 1
            continue
        inputs += 1
        text = open(os.path.join(options.goldens, name + ".cs"), encoding="utf-8", newline="").read()
        original = os.path.join("original", name + ".cs")
        write(original, text)
        lines.append(f"{original}\t{columns}")
        for directory, index, mutated in mutants_of(text, options.per_file, options.seed, name):
            mutant = os.path.join(directory, f"{name}~{index}.cs")
            write(mutant, mutated)
            origins[mutant] = original
            lines.append(f"{mutant}\t{columns}")
    with open(os.path.join(options.out, "origins.json"), "w", encoding="utf-8") as f:
        json.dump(origins, f, sort_keys=True)
        f.write("\n")
    with open(os.path.join(options.out, "list"), "w", encoding="utf-8") as f:
        f.write("".join(line + "\n" for line in lines))
    print(f"mutate: {inputs} inputs without a diagnostic ({skipped} with one left out), "
          f"{len(lines) - inputs} mutants, {len(lines)} files in {options.out}/list")


if __name__ == "__main__":
    main()
