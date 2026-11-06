"""The corpus loop lex-fuzz.py and parse-fuzz.py share: `LIST ROOT OUT_DIR [SEED] [COUNT]` from argv, COUNT (3000)
of the listed files in a seeded random order, each read with normalized line ends, mutated and written to OUT_DIR as
f<N> with its suffix, and OUT_DIR/list naming them."""
import os
import random
import sys


def write(mutate, suffix=lambda path: ".kt"):
    """Write `mutate(text)` of each file; `suffix(path)` names the copy's extension."""
    list_path, root, out = sys.argv[1], sys.argv[2], sys.argv[3]
    random.seed(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
    count = int(sys.argv[5]) if len(sys.argv) > 5 else 3000
    paths = [l.strip() for l in open(list_path, encoding="utf-8") if l.strip()]
    random.shuffle(paths)
    os.makedirs(out, exist_ok=True)
    names = []
    for i, p in enumerate(paths[:count]):
        try:
            t = open(os.path.join(root, p), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        t = mutate(t.replace("\r\n", "\n").replace("\r", "\n"))
        name = f"f{i}{suffix(p)}"
        with open(os.path.join(out, name), "w", encoding="utf-8") as f:
            f.write(t)
        names.append(name)
    with open(os.path.join(out, "list"), "w", encoding="utf-8") as f:
        f.write("\n".join(names) + "\n")
    print(f"{len(names)} files in {out}")
