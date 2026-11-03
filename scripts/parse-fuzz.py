#!/usr/bin/env python3
"""Mutated Kotlin files for the PSI oracle: `parse-fuzz.py LIST ROOT OUT_DIR [SEED] [COUNT]`.

Like lex-fuzz.py, but the mutations aim at the parser's recovery: punctuation, keywords, soft keywords and
modifiers inserted at a token boundary, and words or punctuation deleted. Writes COUNT (3000) files to OUT_DIR as
f<N>.kt (or .kts) and OUT_DIR/list. Then compare the trees:

  java -Xss512m -Xmx8g -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" \\
    $HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/PsiOracle.java --list OUT/list --root OUT --out OUT.oracle
  ../target/psi/ksdump --list OUT/list --root OUT --out OUT.ours
  scripts/compare.py OUT.oracle OUT.ours
"""
import os
import random
import re
import sys

list_path, root, out = sys.argv[1], sys.argv[2], sys.argv[3]
random.seed(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
count = int(sys.argv[5]) if len(sys.argv) > 5 else 3000
paths = [l.strip() for l in open(list_path, encoding="utf-8") if l.strip()]
random.shuffle(paths)
pieces = ['(', ')', '{', '}', '[', ']', '<', '>', ',', ':', ';', '=', '->', '.', '?.', '::', '?', '!!', '?:', '..',
          '..<', '@', '@Ann', '@file:', '*', '&&', '||', '!', 'is ', '!is ', 'as ', 'as? ', 'in ', '!in ', '\n', '\n\n',
          'fun ', 'class ', 'interface ', 'object ', 'val ', 'var ', 'when ', 'if ', 'else ', 'for ', 'while ', 'do ',
          'try ', 'catch ', 'finally ', 'return ', 'throw ', 'break ', 'continue ', 'this', 'super', 'null',
          'typealias ', 'package ', 'import ', 'by ', 'where ', 'get() ', 'set(v) ', 'constructor', 'init ',
          'companion ', 'enum ', 'sealed ', 'data ', 'inline ', 'value ', 'private ', 'override ', 'suspend ',
          'vararg ', 'reified ', 'out ', 'crossinline ', 'context(', 'dynamic ', 'field', 'it', 'x', '1', '"', '"""',
          '${', 'fun interface ', 'expect ', 'actual ', 'external ', 'const ', 'lateinit ', 'operator ', 'infix ',
          '<T>', '<*>', '(a: Int)', '{ a -> }', '_', '`x`', '#']
boundary = re.compile(r'(?<=[\s(){}\[\],;.:=<>])|(?=[\s(){}\[\],;.:=<>])')
os.makedirs(out, exist_ok=True)
names = []
for i, p in enumerate(paths[:count]):
    try:
        t = open(os.path.join(root, p), encoding="utf-8").read()
    except (OSError, UnicodeDecodeError):
        continue
    t = t.replace("\r\n", "\n").replace("\r", "\n")
    t = t[:random.randint(0, min(len(t), 6000))]
    for _ in range(random.randint(1, 6)):
        cuts = [m.start() for m in boundary.finditer(t)] or [0]
        pos = random.choice(cuts)
        op = random.random()
        if op < 0.6:
            t = t[:pos] + random.choice(pieces) + t[pos:]
        else:
            m = re.match(r'\s*(\w+|[^\w\s]{1,2})', t[pos:])
            if m:
                t = t[:pos] + t[pos + m.end():]
    name = f"f{i}" + (".kts" if p.endswith(".kts") else ".kt")
    with open(os.path.join(out, name), "w", encoding="utf-8") as f:
        f.write(t)
    names.append(name)
with open(os.path.join(out, "list"), "w", encoding="utf-8") as f:
    f.write("\n".join(names) + "\n")
print(f"{len(names)} files in {out}")
