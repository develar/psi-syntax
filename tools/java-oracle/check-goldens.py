#!/usr/bin/env python3
"""Compares Java PSI oracle output with the vendored goldens, byte for byte.

    tools/java-oracle/check-goldens.py ORACLE_OUT [--goldens testdata/java]

ORACLE_OUT is a JavaPsiOracle `--node` run over testdata/java/manifest.tsv (root testdata/java):
each NAME.java.psi.txt must equal NAME.txt and each NAME.java.node.txt must equal NAME_node.txt.
Prints the equal counts and every differing path; exits 1 on a difference.
"""
import argparse
import os
import sys

parser = argparse.ArgumentParser()
parser.add_argument("out")
parser.add_argument("--goldens", default=os.path.join(os.path.dirname(__file__), "..", "..", "testdata", "java"))
options = parser.parse_args()
failed = False
for kind, golden_suffix, dump_suffix in (("psi", ".txt", ".psi.txt"), ("node", "_node.txt", ".node.txt")):
    equal, differ = 0, []
    for row in open(os.path.join(options.goldens, "manifest.tsv"), encoding="utf-8"):
        path = row.split("\t")[0]
        with open(os.path.join(options.goldens, path[:-len(".java")] + golden_suffix), "rb") as f:
            want = f.read()
        try:
            with open(os.path.join(options.out, path + dump_suffix), "rb") as f:
                got = f.read()
        except FileNotFoundError:
            got = None
        if got == want:
            equal += 1
        else:
            differ.append((path, "missing" if got is None else "differs"))
    print(f"{kind}: {equal} equal, {len(differ)} differ")
    for path, how in differ:
        print(f"  {how}: {path}")
    failed |= bool(differ)
sys.exit(1 if failed else 0)
