#!/usr/bin/env python3
"""The corpus list of a checkout: its tracked `.cs` files that are valid UTF-8, one path per line, sorted.

    tools/csharp-oracle/corpus.py CHECKOUT > ~/.cache/psi-syntax/corpus/csharp-REPO-SHA7.list

Prints to stderr how many tracked `.cs` files there are, how many are left out (not UTF-8, not a regular file) and
the total bytes of the list. The oracle dumps only valid UTF-8 (README.md, "Input"), so the list holds what the
gate compares; invalid bytes are the fuzz's business (no panic, lossless)."""
import os
import subprocess
import sys


def main():
    root = sys.argv[1]
    out = subprocess.run(["git", "-C", root, "ls-files", "-z", "--", "*.cs"], check=True, capture_output=True).stdout
    paths = sorted(p for p in out.decode("utf-8", "surrogateescape").split("\0") if p)
    kept, invalid, other, total = [], 0, 0, 0
    for p in paths:
        full = os.path.join(root, p)
        if os.path.islink(full) or not os.path.isfile(full):
            other += 1
            continue
        data = open(full, "rb").read()
        try:
            data.decode("utf-8", "strict")
            p.encode("utf-8", "strict")
        except UnicodeError:
            invalid += 1
            continue
        if "\t" in p or "\n" in p:
            other += 1
            continue
        kept.append(p)
        total += len(data)
    sys.stdout.write("".join(p + "\n" for p in kept))
    print(f"{len(paths)} tracked .cs files, {len(kept)} listed ({total} bytes), {invalid} not UTF-8, {other} other",
          file=sys.stderr)


main()
