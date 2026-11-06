#!/usr/bin/env python3
"""Mutated Kotlin files for the lexer oracle: `lex-fuzz.py LIST ROOT OUT_DIR [SEED] [COUNT]`.

Writes COUNT (3000) files cut and mutated from the listed ones (string, template, comment, backtick and number
fragments, odd code points) to OUT_DIR as f<N>.kt and OUT_DIR/list. Then compare the two lexers:

  java -cp "$HOME/.cache/psi-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java --list OUT/list --root OUT | cut -f1,2 > a
  ../target/psi/ksdump --lex --list OUT/list --root OUT | cut -f1,2 > b && diff a b
"""
import random

import fuzz_corpus

pieces = ['"', '"""', '$', '$$', '${', '{', '}', '`', '/*', '*/', '/**', "'", '\\', '\n', '!in', '!is', 'as?', '1.', '0x',
          '..', '#!', '@', '$$$"', '"$', '\\u12', 'e+', '`a b`', '\r', 'Ж', '€', '\t', ' ', '*', '/', '**/', '[a]',
          '@param ', '```\n', '~~~\n', '    ']


def mutate(t):
    chars = list(t[:random.randint(0, min(len(t), 4000))])
    for _ in range(random.randint(1, 12)):
        op, pos = random.random(), random.randint(0, len(chars))
        if op < 0.5:
            chars[pos:pos] = list(random.choice(pieces))
        elif op < 0.8 and chars:
            del chars[pos:pos + random.randint(1, 3)]
        else:
            chars[pos:pos] = [chr(random.choice([0x2028, 0x85, 0xA0, 0x663, 0x1F600, 0x300, 0xFEFF, 0x0B]))]
    return "".join(chars)


fuzz_corpus.write(mutate)
