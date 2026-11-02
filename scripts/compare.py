#!/usr/bin/env python3
"""Compare our dumps with an oracle's: `compare.py ORACLE_DIR OURS_DIR [--examples N] [--json OUT] [--suffix S]`.

Both directories hold `<path><suffix>` per file (the oracle's also `index.jsonl`): `.psi.txt` (the PSI oracles) or
`.tree.txt` (the C# oracle, tools/csharp-oracle), detected from the oracle directory unless `--suffix` names it.
Prints the equal count, the missing ones, and the differences bucketed by the first differing line pair with its
parent chain (the node kinds above it), largest bucket first, each with its smallest example file. A bucket is one bug
more often than not: fix the biggest. In a `.tree.txt` bucket key the `[start..end)` offsets are left out, so one bug
in files of different lengths is one bucket; an index line with a `failure` (no oracle dump) is counted apart."""
import json
import os
import re
import sys

SUFFIXES = (".psi.txt", ".tree.txt")
RANGE = re.compile(r" \[\d+\.\.\d+\)")


def lines_of(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().rstrip().split("\n")


def label(line, suffix):
    """A parent line as a bucket key names it: the PSI element type, or the C# kind without its offsets."""
    line = line.strip()
    return line.split("(")[0] if suffix == ".psi.txt" else RANGE.sub("", line)


def parents(lines, i, suffix=".psi.txt"):
    """The composite kinds above line i, nearest last (indentation decides the parent)."""
    depth = len(lines[i]) - len(lines[i].lstrip(" "))
    chain = []
    for j in range(i - 1, -1, -1):
        d = len(lines[j]) - len(lines[j].lstrip(" "))
        if d < depth:
            chain.append(label(lines[j], suffix))
            depth = d
            if len(chain) == 3:
                break
    return " > ".join(reversed(chain))


def first_diff(a, b):
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            return i
    return min(len(a), len(b)) if len(a) != len(b) else None


def detect_suffix(oracle, index):
    """The suffix the oracle's dumps carry: of the first indexed dump on disk, else of the first file found."""
    if os.path.exists(index):
        for l in open(index, encoding="utf-8"):
            p = json.loads(l)["path"]
            for suffix in SUFFIXES:
                if os.path.exists(os.path.join(oracle, p + suffix)):
                    return suffix
    for d, _, fs in os.walk(oracle):
        for f in fs:
            for suffix in SUFFIXES:
                if f.endswith(suffix):
                    return suffix
    return SUFFIXES[0]


def main():
    oracle, ours = sys.argv[1], sys.argv[2]
    examples = int(sys.argv[sys.argv.index("--examples") + 1]) if "--examples" in sys.argv else 3
    out_json = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
    index = os.path.join(oracle, "index.jsonl")
    suffix = sys.argv[sys.argv.index("--suffix") + 1] if "--suffix" in sys.argv else detect_suffix(oracle, index)
    tree = suffix != ".psi.txt"
    failed = []
    if os.path.exists(index):
        rows = [json.loads(l) for l in open(index, encoding="utf-8")]
        if tree:
            failed = [r["path"] for r in rows if "failure" in r]
            rows = [r for r in rows if "failure" not in r]
        paths = [r["path"] for r in rows]
    else:
        paths = [os.path.relpath(os.path.join(d, f), oracle)[: -len(suffix)]
                 for d, _, fs in os.walk(oracle) for f in fs if f.endswith(suffix)]
    equal, missing, buckets = 0, [], {}
    for p in paths:
        mine = os.path.join(ours, p + suffix)
        if not os.path.exists(mine):
            missing.append(p)
            continue
        a, b = lines_of(os.path.join(oracle, p + suffix)), lines_of(mine)
        i = first_diff(a, b)
        if i is None:
            equal += 1
            continue
        want = a[i].strip() if i < len(a) else "<end>"
        got = b[i].strip() if i < len(b) else "<end>"
        if tree:
            want, got = RANGE.sub("", want), RANGE.sub("", got)
        key = f"{parents(a, i, suffix) if i < len(a) else ''} | want {want[:60]} | got {got[:60]}"
        size = os.path.getsize(os.path.join(oracle, p + suffix))
        buckets.setdefault(key, []).append((size, p, i + 1))
    total = len(paths)
    print(f"equal {equal} of {total} ({100.0 * equal / max(total, 1):.2f} %), differ {total - equal - len(missing)}, missing {len(missing)}"
          + (f", {len(failed)} without an oracle dump" if failed else ""))
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
