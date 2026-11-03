#!/usr/bin/env python3
"""Shrinks a file whose dump differs from the PSI oracle's: `psi-reduce.py FILE OUT`.

Deletes chunks of lines, then of characters (halving the chunk down to one), keeping a candidate while
`../target/psi/ksdump` and PsiOracle.java still print different trees for it; each round runs the oracle once
over every candidate. Writes the smallest input to OUT and prints it. The result shows one cause of the
difference, not necessarily the one of the original file: rerun the original after fixing it.
"""
import os
import shutil
import subprocess
import sys
import tempfile

here = os.path.dirname(os.path.abspath(__file__))
ksdump = os.path.join(here, "..", "..", "target", "psi", "ksdump")
tools = os.path.expanduser("~/.cache/psi-syntax/psi-tools/tools/psi-oracle")
source, out = sys.argv[1], sys.argv[2]
ext = os.path.splitext(source)[1]
work = tempfile.mkdtemp(prefix="psi-reduce-")


def differs(candidates):
    """Per candidate text, whether our dump differs from the oracle's."""
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(os.path.join(work, "in"))
    names = []
    for i, text in enumerate(candidates):
        names.append(f"c{i}{ext}")
        with open(os.path.join(work, "in", names[-1]), "w", encoding="utf-8") as f:
            f.write(text)
    listing = os.path.join(work, "in", "list")
    with open(listing, "w", encoding="utf-8") as f:
        f.write("\n".join(names) + "\n")
    subprocess.run(["java", "-Xss512m", "-Xmx8g", "-cp", os.path.join(tools, "lib", "*"),
                    os.path.join(tools, "PsiOracle.java"), "--list", listing, "--root", os.path.join(work, "in"),
                    "--out", os.path.join(work, "oracle")], capture_output=True)
    subprocess.run([ksdump, "--list", listing, "--root", os.path.join(work, "in"), "--out",
                    os.path.join(work, "ours")], capture_output=True)
    result = []
    for name in names:
        try:
            with open(os.path.join(work, "oracle", name + ".psi.txt"), encoding="utf-8") as f:
                want = f.read()
            with open(os.path.join(work, "ours", name + ".psi.txt"), encoding="utf-8") as f:
                got = f.read()
        except OSError:
            result.append(False)
            continue
        result.append(want != got)
    return result


with open(source, encoding="utf-8") as f:
    text = f.read()
if not differs([text])[0]:
    sys.exit(f"{source}: the dumps are equal")
for unit in ("line", "char"):
    chunk = None
    while True:
        parts = text.splitlines(keepends=True) if unit == "line" else list(text)
        if chunk is None:
            chunk = max(1, len(parts) // 2)
        candidates = ["".join(parts[:i] + parts[i + chunk:]) for i in range(0, len(parts), chunk)]
        found = differs(candidates)
        if any(found):
            text = candidates[found.index(True)]
            continue
        if chunk == 1:
            break
        chunk = max(1, chunk // 2)
shutil.rmtree(work, ignore_errors=True)
with open(out, "w", encoding="utf-8") as f:
    f.write(text)
print(repr(text))
