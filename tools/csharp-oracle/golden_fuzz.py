"""The loop lex_fuzz.py and getters_fuzz.py share: `GOLDENS OUT_DIR [COUNT] [SEED]` from argv, one seeded generator,
the vendored golden inputs read in path order, and COUNT files OUT_DIR/NNNNN.cs with OUT_DIR/list naming them."""

import random
import sys
from pathlib import Path


def write(count: int, seed: str, file) -> None:
    """`file(i, texts, rng)` returns the text of input `i` and the rest of its list line after the name. COUNT and
    SEED default to `count` and `seed`. A deletion can split a surrogate pair of a non-BMP character: never in Python's
    str, so every file is valid UTF-8."""
    goldens = Path(sys.argv[1])
    out = Path(sys.argv[2])
    count = int(sys.argv[3]) if len(sys.argv) > 3 else count
    rng = random.Random(sys.argv[4] if len(sys.argv) > 4 else seed)
    texts = [p.read_text(encoding="utf-8") for p in sorted(goldens.rglob("*.cs"))]
    out.mkdir(parents=True, exist_ok=True)
    lines = []
    for i in range(count):
        name = f"{i:05d}.cs"
        text, rest = file(i, texts, rng)
        (out / name).write_bytes(text.encode("utf-8"))
        lines.append(f"{name}{rest}\n")
    (out / "list").write_text("".join(lines))
