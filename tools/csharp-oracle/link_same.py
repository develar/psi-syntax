#!/usr/bin/env python3
"""Hard-link the files of one dump directory that equal another's: `link_same.py BASE OTHER`.

The Preview dumps of a corpus equal its Latest dumps but for the files a preview feature changes, so OTHER's
equal files become hard links to BASE's (same bytes, one copy on disk). Prints how many were linked and the bytes
saved. Readers see ordinary files. Writing through a link would change both; the oracle never does (it writes a
new file and renames it over the old one), and anything else that writes there must do the same."""
import filecmp
import os
import sys


def main():
    base, other = sys.argv[1], sys.argv[2]
    linked = saved = 0
    for d, _, files in os.walk(other):
        for f in files:
            o = os.path.join(d, f)
            b = os.path.join(base, os.path.relpath(o, other))
            if not os.path.isfile(b) or os.path.samefile(b, o):
                continue
            if os.path.getsize(b) == os.path.getsize(o) and filecmp.cmp(b, o, shallow=False):
                size = os.path.getsize(o)
                tmp = o + ".link"
                os.link(b, tmp)
                os.replace(tmp, o)
                linked += 1
                saved += size
    print(f"linked {linked} files, {saved / 1e9:.2f} GB", file=sys.stderr)


main()
