#!/usr/bin/env python3
"""Mutated Kotlin files for the lexer oracle: `lex-fuzz.py LIST ROOT OUT_DIR [SEED] [COUNT]`.

Writes COUNT (3000) files cut and mutated from the listed ones (string, template, comment, backtick and number
fragments, odd code points) to OUT_DIR as f<N>.kt and OUT_DIR/list. Then compare the two lexers:

  java -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java --list OUT/list --root OUT | cut -f1,2 > a
  ../target/psi/ksdump --lex --list OUT/list --root OUT | cut -f1,2 > b && diff a b
"""
import os
import random
import sys

list_path, root, out = sys.argv[1], sys.argv[2], sys.argv[3]
random.seed(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
count = int(sys.argv[5]) if len(sys.argv) > 5 else 3000
paths = [l.strip() for l in open(list_path, encoding="utf-8") if l.strip()]
random.shuffle(paths)
pieces = ['"', '"""', '$', '$$', '${', '{', '}', '`', '/*', '*/', '/**', "'", '\\', '\n', '!in', '!is', 'as?', '1.', '0x',
          '..', '#!', '@', '$$$"', '"$', '\\u12', 'e+', '`a b`', '\r', 'Ж', '€', '\t', ' ', '*', '/', '**/', '[a]',
          '@param ', '```\n', '~~~\n', '    ']
os.makedirs(out, exist_ok=True)
names = []
for i, p in enumerate(paths[:count]):
    try:
        t = open(os.path.join(root, p), encoding="utf-8").read()
    except (OSError, UnicodeDecodeError):
        continue
    t = t.replace("\r\n", "\n").replace("\r", "\n")
    chars = list(t[:random.randint(0, min(len(t), 4000))])
    for _ in range(random.randint(1, 12)):
        op, pos = random.random(), random.randint(0, len(chars))
        if op < 0.5:
            chars[pos:pos] = list(random.choice(pieces))
        elif op < 0.8 and chars:
            del chars[pos:pos + random.randint(1, 3)]
        else:
            chars[pos:pos] = [chr(random.choice([0x2028, 0x85, 0xA0, 0x663, 0x1F600, 0x300, 0xFEFF, 0x0B]))]
    name = f"f{i}.kt"
    with open(os.path.join(out, name), "w", encoding="utf-8") as f:
        f.write("".join(chars))
    names.append(name)
with open(os.path.join(out, "list"), "w", encoding="utf-8") as f:
    f.write("\n".join(names) + "\n")
print(f"{len(names)} files in {out}")
