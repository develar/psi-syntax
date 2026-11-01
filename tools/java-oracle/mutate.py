#!/usr/bin/env python3
"""One-place mutations of the Java goldens for the PSI differential.

    tools/java-oracle/mutate.py [--goldens testdata/java] [--per-file 2] [--seed java-mut] OUT_DIR

psi-tools' tools/psi-gate/mutate.js for Java. Each operator applies to every input, not to hand-picked
cases, so a gap shows wherever the operator's construct is. A mutant changes one place of its
original (`--per-file N` places per operator and file, picked by a generator seeded with the seed,
the file and the operator), so a gap is not hidden behind an error the file has elsewhere. The
inputs are the PSI-clean goldens of testdata/java/manifest.tsv (no `PsiErrorElement` in NAME.txt),
whole files and parser-partial fragments alike, each copied under `original/` as the test parses it
(trimmed where the test trims). OUT_DIR/origins.json maps every mutant to its original and
OUT_DIR/list is an oracle list: `PATH<TAB>LEVEL<TAB>ENTRY<TAB>0`, the golden's level and entry,
no trimming. Every text is valid UTF-8.
"""
import argparse
import json
import os
import random
import re

KEYWORDS = ["abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue",
            "default", "do", "double", "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if",
            "implements", "import", "instanceof", "int", "interface", "long", "native", "new", "package", "private",
            "protected", "public", "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
            "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false", "null"]
# Contextual words of java-syntax (JavaKeywords.kt and the module/record/sealed/yield/when parsing).
SOFT = ["var", "yield", "record", "sealed", "permits", "non-sealed", "when", "module", "open", "opens", "requires",
        "exports", "uses", "provides", "transitive", "to", "with", "value", "_"]
KEYWORD_SET = set(KEYWORDS)
OPERATORS = [">>>=", "<<=", ">>=", "...", "->", "::", "++", "--", "&&", "||", "==", "!=", "<=", ">=", "+=", "-=",
             "*=", "/=", "%=", "&=", "|=", "^=", "<<"]
PIECES = ["(", ")", "{", "}", "[", "]", "<", ">", ",", ":", ";", "=", "->", ".", "::", "?", "@", "@A ", "*", "&&",
          "!", "...", "instanceof ", "new ", "class ", "interface ", "enum ", "record ", "@interface ", "if ",
          "else ", "for ", "while ", "do ", "try ", "catch ", "finally ", "switch ", "case ", "default ", "yield ",
          "return ", "throw ", "break ", "continue ", "assert ", "synchronized ", "this", "super", "null", "var ",
          "final ", "static ", "public ", "sealed ", "non-sealed ", "permits ", "extends ", "implements ", "throws ",
          "import ", "package ", "module ", "when ", "x", "1", "1L", "0x", "1e", '"', '"""', "'", "<T>", "<?>",
          "(int a)", "() -> {}", "int ", "void ", "int[] ", "#", "\\", "/**", "/*", "//", "\n"]
CHAR_CONTENTS = ["\\q", "\\u0000", "ab", "", "a\nb", "\\", "'", "\\u000a", "\\777"]
STRING_EDITS = [("unclosed", lambda s: s[:-1]), ("break", lambda s: s[:len(s) // 2] + "\n" + s[len(s) // 2:]),
                ("bad-escape", lambda s: s[:-1] + "\\q" + s[-1]), ("block-open", lambda s: '""' + s + '""')]
BAD_NAMES = [("emoji-suffix", lambda n: n + "\U0001F4A9"), ("digit-start", lambda n: "١" + n),
             ("unicode-escape", lambda n: "\\u%04x" % ord(n[0]) + n[1:])]
DECLARATION_KEYWORDS = {"class", "interface", "enum", "record", "void", "int", "boolean", "long"}
MODIFIER_RUNS = [("private-twice", "private private "), ("annotations", "@A @B(x) @C.D({1, 2}) "),
                 ("all", "public protected private abstract static final transient volatile synchronized native "
                         "strictfp default sealed non-sealed ")]

word_re = re.compile(r"[^\W\d]\w*|[$_][\w$]*", re.U)
number_re = re.compile(r"[0-9](?:[0-9a-zA-Z_]|\.(?=[0-9]))*|\.[0-9][0-9a-zA-Z_]*")


class Token:
    __slots__ = ("kind", "start", "end", "text", "line")

    def __init__(self, kind, start, end, text, line):
        self.kind, self.start, self.end, self.text, self.line = kind, start, end, text, line


def tokenize(text):
    """Tokens with offsets and lines; strings, text blocks, characters and comments are opaque."""
    tokens = []
    i, n = 0, len(text)

    def push(kind, start, end):
        tokens.append(Token(kind, start, end, text[start:end], text.count("\n", 0, start) + 1))

    while i < n:
        c = text[i]
        if c.isspace():
            i += 1
            continue
        if text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
            continue
        if text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = n if end < 0 else end + 2
            continue
        start = i
        if text.startswith('"""', i):
            i += 3
            while i < n and not text.startswith('"""', i):
                i += 2 if text[i] == "\\" else 1
            i = min(n, i + 3)
            push("string", start, i)
            continue
        if c in "\"'":
            i += 1
            while i < n and text[i] != c and text[i] != "\n":
                i += 2 if text[i] == "\\" else 1
            i = min(n, i + 1)
            push("string" if c == '"' else "char", start, i)
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
    return t.kind == "word" and t.text not in KEYWORD_SET


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
                              for v, repl in (("semicolon", ";"), ("none", ""))]))
    ops.append(("insert-piece", sites(lambda k: True),
                lambda k, r: [("", edit(text, tokens[k].start, tokens[k].start, (lambda p: p if p.endswith((" ", "\n")) else p + " ")(r.choice(PIECES))))]))
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
    for name, suffix in (("unicode-latin", "é"), ("unicode-cjk", "名")):
        mutated = rename(text, tokens, suffix)
        if mutated != text:
            out.append((name, 0, mutated))
    return out


def trim(text):
    return text.strip("".join(chr(c) for c in range(33)))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("out")
    parser.add_argument("--goldens", default=os.path.join(os.path.dirname(__file__), "..", "..", "testdata", "java"))
    parser.add_argument("--per-file", type=int, default=2)
    parser.add_argument("--seed", default="java-mut")
    options = parser.parse_args()
    lines, origins = [], {}
    inputs = skipped = 0

    def write(rel, text):
        dest = os.path.join(options.out, rel)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        with open(dest, "w", encoding="utf-8", newline="") as f:
            f.write(text)

    for row in open(os.path.join(options.goldens, "manifest.tsv"), encoding="utf-8"):
        path, level, entry, trimmed, _ = row.rstrip("\n").split("\t")
        golden = open(os.path.join(options.goldens, path[:-len(".java")] + ".txt"), encoding="utf-8").read()
        if "PsiErrorElement" in golden:
            skipped += 1
            continue
        inputs += 1
        raw = open(os.path.join(options.goldens, path), encoding="utf-8", newline="").read()
        text = raw.replace("\r\n", "\n").replace("\r", "\n")
        if trimmed == "1":
            text = trim(text)
        original = os.path.join("original", path)
        write(original, text)
        lines.append(f"{original}\t{level}\t{entry}\t0")
        for directory, index, mutated in mutants_of(text, options.per_file, options.seed, path):
            mutant = os.path.join(directory, f"{path[:-len('.java')]}~{index}.java")
            write(mutant, mutated)
            origins[mutant] = original
            lines.append(f"{mutant}\t{level}\t{entry}\t0")
    with open(os.path.join(options.out, "origins.json"), "w", encoding="utf-8") as f:
        json.dump(origins, f, sort_keys=True)
        f.write("\n")
    with open(os.path.join(options.out, "list"), "w", encoding="utf-8") as f:
        f.write("".join(line + "\n" for line in lines))
    print(f"mutate: {inputs} PSI-clean inputs ({skipped} with a PSI error left out), "
          f"{len(lines) - inputs} mutants, {len(lines)} files in {options.out}/list")


if __name__ == "__main__":
    main()
