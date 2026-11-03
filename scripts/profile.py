#!/usr/bin/env python3
"""Profiles `ksdump --bench` with macOS `sample` (1 ms) and prints where the time goes, inline-aware.

    scripts/profile.py [--bin ../target/psi/ksdump] [--seconds 12] [--warmup 4] [--top 30] [--lines 40] [--inclusive 60]
                       [--sample FILE] [-- BENCH ARGS...]

Starts the bench over the community corpus (KS_LIST, KS_ROOT override it) with many rounds, waits `--warmup`
seconds for the sources to load, samples it for `--seconds`, stops it, and symbolizes every sampled address with
`atos -i`, so code that fat LTO inlined is charged to the function and line it came from. Prints:

1. self time per physical function (what `sample` sees: the function the inlined code landed in);
2. self time per source function (the innermost inlined frame) and per source line;
3. inclusive time per source function (any frame of the stack, inlined or not, counted once per sample).

`--sample FILE` reads an existing `sample` output instead (its binary must still be at the path it names).
Bench arguments after `--` go to `ksdump --bench` (`--threads 4`: the profile sums all threads).
"""

import argparse
import collections
import os
import re
import subprocess
import sys
import time

LIST = os.environ.get("KS_LIST", os.path.expanduser("~/.cache/psi-syntax/corpus/community-3caa0bf.list"))
ROOT = os.environ.get("KS_ROOT", "/Users/develar/.cache/accio-bench/intellij-community")

NODE = re.compile(r"^(?P<indent>[\s+!:|]*)(?P<count>\d+) (?P<sym>.+?)  \(in (?P<image>[^)]+)\)(?: \+ [\d,]+)?"
                  r"(?:  \[(?P<addrs>[0-9a-fx,]+)\])?")


_DEMANGLED = {}


def demangle_all(symbols):
    """Demangles Rust (v0 and legacy) symbols with `c++filt` in one batch into `_DEMANGLED`."""
    todo = sorted({x for x in symbols if x not in _DEMANGLED})
    if not todo:
        return
    for i in range(0, len(todo), 500):
        batch = todo[i:i + 500]
        out = subprocess.run(["c++filt"] + batch, capture_output=True, text=True, stdin=subprocess.DEVNULL)
        for raw, nice in zip(batch, out.stdout.splitlines()):
            _DEMANGLED[raw] = shorten(nice)


def shorten(name):
    """`<a::b::T as a::c::Trait>::m` to `T::m`, `<a::T>::m` to `T::m`, paths and hashes dropped."""
    name = re.sub(r"::h[0-9a-f]{16}$", "", name)
    name = re.sub(r"\b(?:[A-Za-z_][A-Za-z0-9_]*::)+(?=[A-Za-z_])", "", name)
    name = re.sub(r"\[[0-9a-f]+\]", "", name)
    for _ in range(4):
        name = re.sub(r"<([^<>]*?) as [^<>]*>::", r"\1::", name)
        name = re.sub(r"<([^<>]*)>::", r"\1::", name)
    return name


def demangle(sym):
    return _DEMANGLED.get(sym, sym)


def parse_sample(path):
    """The call graph nodes as (depth, count, symbol, image, addresses, parent index), and the load address."""
    load = None
    nodes = []
    stack = []
    in_graph = False
    with open(path) as f:
        for line in f:
            if line.startswith("Load Address:"):
                load = line.split()[-1]
            if line.startswith("Call graph:"):
                in_graph = True
                continue
            if not in_graph:
                continue
            if not line.strip() or line.startswith("Total number") or line.startswith("Sort by"):
                if nodes:
                    break
                continue
            m = NODE.match(line.rstrip("\n"))
            if not m:
                continue
            depth = len(m.group("indent"))
            while stack and nodes[stack[-1]][0] >= depth:
                stack.pop()
            parent = stack[-1] if stack else -1
            addrs = m.group("addrs").split(",") if m.group("addrs") else []
            nodes.append((depth, int(m.group("count")), m.group("sym"), m.group("image"), addrs, parent))
            stack.append(len(nodes) - 1)
    return nodes, load


def symbolize(binary, load, addrs):
    """`atos -i`: each address to its inline chain, innermost first, as (function, file:line)."""
    chains = {}
    addrs = sorted(addrs)
    raw = {}
    for i in range(0, len(addrs), 2000):
        batch = addrs[i:i + 2000]
        out = subprocess.run(["atos", "-o", binary, "-l", load, "-i"] + batch, capture_output=True, text=True).stdout
        for a, g in zip(batch, out.split("\n\n")):
            chain = []
            for l in g.strip().splitlines():
                m = re.match(r"(\S+) \(in [^)]+\) \(([^)]+)\)", l)
                chain.append((m.group(1), m.group(2)) if m else (l.split(" ")[0], "?"))
            raw[a] = chain
    demangle_all(fn for chain in raw.values() for fn, _ in chain)
    for a, chain in raw.items():
        chains[a] = [(demangle(fn), line) for fn, line in chain]
    return chains


def main():
    argv = sys.argv[1:]
    bench_args = []
    if "--" in argv:
        i = argv.index("--")
        argv, bench_args = argv[:i], argv[i + 1:]
    p = argparse.ArgumentParser()
    p.add_argument("--bin", default="../target/psi/ksdump")
    p.add_argument("--seconds", type=int, default=12)
    p.add_argument("--warmup", type=float, default=4)
    p.add_argument("--top", type=int, default=30)
    p.add_argument("--lines", type=int, default=40)
    p.add_argument("--inclusive", type=int, default=60)
    p.add_argument("--sample")
    a = p.parse_args(argv)

    path = a.sample
    if not path:
        path = "/tmp/ks-profile-sample.txt"
        bench = subprocess.Popen([a.bin, "--bench", "--list", LIST, "--root", ROOT, "--repeat", "1000"] + bench_args,
                                 stdout=subprocess.DEVNULL)
        time.sleep(a.warmup)
        subprocess.run(["sample", str(bench.pid), str(a.seconds), "1", "-mayDie", "-file", path],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        bench.kill()
        bench.wait()
    nodes, load = parse_sample(path)
    binary = os.path.abspath(a.bin)
    with open(path) as f:
        for line in f:
            if line.startswith("Path:"):
                binary = line.split(None, 1)[1].strip()
                break

    children = collections.defaultdict(int)
    for depth, count, sym, image, addrs, parent in nodes:
        if parent >= 0:
            children[parent] += count
    # Self samples per node, the waiting main thread and the kernel left out.
    selfs = [max(0, n[1] - children[i]) for i, n in enumerate(nodes)]
    idle = {"__ulock_wait", "__psynch_cvwait", "__semwait_signal", "mach_msg2_trap", "__workq_kernreturn"}
    total = sum(s for s, n in zip(selfs, nodes) if n[2] not in idle)
    print(f"samples {total} (1 ms each, idle waits excluded), binary {binary}")

    demangle_all(n[2] for n in nodes)
    ours = {os.path.basename(binary)}
    # A frame with callees is sampled at its return address: symbolize the call instruction before it.
    def call_site(x):
        return hex(int(x, 16) - 1)

    addrs = {x for i, n in enumerate(nodes) if n[3] in ours for x in n[4]}
    addrs |= {call_site(x) for i, n in enumerate(nodes) if n[3] in ours and children[i] for x in n[4]}
    chains = symbolize(binary, load, addrs) if load and addrs else {}

    def node_chain(i):
        n = nodes[i]
        if n[3] in ours and n[4]:
            x = call_site(n[4][0]) if children[i] else n[4][0]
            return chains.get(x, [(demangle(n[2]), "?")])
        return [(demangle(n[2]) + f" ({n[3]})", "?")]

    physical = collections.Counter()
    source_fn = collections.Counter()
    source_line = collections.Counter()
    inclusive = collections.Counter()
    memo = {}

    def logical_stack(i):
        if i in memo:
            return memo[i]
        parent = nodes[i][5]
        above = logical_stack(parent) if parent >= 0 else frozenset()
        memo[i] = above | {fn for fn, _ in node_chain(i)}
        return memo[i]

    for i, n in enumerate(nodes):
        s = selfs[i]
        if not s or n[2] in idle:
            continue
        physical[demangle(n[2]) + ("" if n[3] in ours else f" ({n[3]})")] += s
        targets = n[4] if (n[3] in ours and n[4]) else [None]
        for x in targets:
            share = s / len(targets)
            chain = chains.get(x) if x else None
            if not chain:
                chain = [(demangle(n[2]) + ("" if n[3] in ours else f" ({n[3]})"), "?")]
            fn, line = chain[0]
            source_fn[fn] += share
            caller = chain[1][0] if len(chain) > 1 else (demangle(nodes[n[5]][2]) if n[5] >= 0 else "")
            source_line[(line, fn, caller)] += share
        for fn in logical_stack(i):
            inclusive[fn] += s

    def pct(v):
        return f"{100.0 * v / max(total, 1):5.1f}%"

    print(f"\n== self, physical function (top {a.top})")
    for k, v in physical.most_common(a.top):
        print(f"{pct(v)} {v:7.0f}  {k}")
    print(f"\n== self, source function: the innermost inlined frame (top {a.top})")
    for k, v in source_fn.most_common(a.top):
        print(f"{pct(v)} {v:7.0f}  {k}")
    print(f"\n== self, source line: line, its function, the frame it is inlined into or called from (top {a.lines})")
    for (line, fn, caller), v in source_line.most_common(a.lines):
        print(f"{pct(v)} {v:7.0f}  {line:28} {fn}  <- {caller}")
    harness = re.compile(r"spawn_unchecked|catch_unwind|thread_start|call_once|__rust_begin|do_call|bench|Tree::parse")
    print(f"\n== inclusive, source function, harness frames left out (top {a.inclusive})")
    shown = [(k, v) for k, v in inclusive.most_common() if not harness.search(k)]
    for k, v in shown[: a.inclusive]:
        print(f"{pct(v)} {v:7.0f}  {k}")


if __name__ == "__main__":
    main()
