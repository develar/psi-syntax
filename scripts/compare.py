#!/usr/bin/env python3
"""Compare our dumps with the PSI oracle's: `compare.py ORACLE_DIR OURS_DIR [--examples N] [--json OUT]`.

Both directories hold `<path>.psi.txt` per file (the oracle's also `index.jsonl`). Prints the equal count, the missing
ones, and the differences bucketed by the first differing line pair with its parent chain (the node kinds above it),
largest bucket first, each with its smallest example file. A bucket is one bug more often than not: fix the biggest."""
import json
import os
import sys


def lines_of(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().rstrip().split("\n")


def parents(lines, i):
    """The composite kinds above line i, nearest last (indentation decides the parent)."""
    depth = len(lines[i]) - len(lines[i].lstrip(" "))
    chain = []
    for j in range(i - 1, -1, -1):
        d = len(lines[j]) - len(lines[j].lstrip(" "))
        if d < depth:
            chain.append(lines[j].strip().split("(")[0])
            depth = d
            if len(chain) == 3:
                break
    return " > ".join(reversed(chain))


def first_diff(a, b):
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            return i
    return min(len(a), len(b)) if len(a) != len(b) else None


def main():
    oracle, ours = sys.argv[1], sys.argv[2]
    examples = int(sys.argv[sys.argv.index("--examples") + 1]) if "--examples" in sys.argv else 3
    out_json = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
    index = os.path.join(oracle, "index.jsonl")
    if os.path.exists(index):
        paths = [json.loads(l)["path"] for l in open(index, encoding="utf-8")]
    else:
        paths = [os.path.relpath(os.path.join(d, f), oracle)[: -len(".psi.txt")]
                 for d, _, fs in os.walk(oracle) for f in fs if f.endswith(".psi.txt")]
    equal, missing, buckets = 0, [], {}
    for p in paths:
        mine = os.path.join(ours, p + ".psi.txt")
        if not os.path.exists(mine):
            missing.append(p)
            continue
        a, b = lines_of(os.path.join(oracle, p + ".psi.txt")), lines_of(mine)
        i = first_diff(a, b)
        if i is None:
            equal += 1
            continue
        want = a[i].strip() if i < len(a) else "<end>"
        got = b[i].strip() if i < len(b) else "<end>"
        key = f"{parents(a, i) if i < len(a) else ''} | want {want[:60]} | got {got[:60]}"
        size = os.path.getsize(os.path.join(oracle, p + ".psi.txt"))
        buckets.setdefault(key, []).append((size, p, i + 1))
    total = len(paths)
    print(f"equal {equal} of {total} ({100.0 * equal / max(total, 1):.2f} %), differ {total - equal - len(missing)}, missing {len(missing)}")
    ranked = sorted(buckets.items(), key=lambda kv: -len(kv[1]))
    for key, items in ranked[:40]:
        items.sort()
        print(f"{len(items):6}  {key}")
        for size, p, line in items[:examples]:
            print(f"          {p}:{line} ({size} bytes of dump)")
    if out_json:
        json.dump({"equal": equal, "total": total, "missing": missing,
                   "buckets": [{"key": k, "count": len(v), "examples": [p for _, p, _ in sorted(v)[:examples]]} for k, v in ranked]},
                  open(out_json, "w"), indent=1)


main()
