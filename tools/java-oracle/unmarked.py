#!/usr/bin/env python3
"""The tracked .java files of a checkout that carry no test markup, one path per line.

    tools/java-oracle/unmarked.py CHECKOUT > LIST

The Java analogue of psi-tools' tools/psi-gate/unmarked.py: IDE markup (<caret>, <selection>,
highlighting tags such as <error descr="...">, <expr>), diagnostics markup (<!NAME!>...<!>) and
sections of other languages in a multi-file test (// FILE: A.kt). The parser reads them as code, so
a verdict on such a file says nothing about the parser; the files without markup stay, broken or not.
"""
import re
import subprocess
import sys

root = sys.argv[1]
markup = re.compile(rb'<(?:/?caret\w*|/?before|/?after|/?change|rename\b[^>]*|/?selection|/?error|/?warning|/?weak_warning|/?info|/?lineMarker|/?EOLError|/?EOLWarning|/?spot|/?symbolName|/?fold|/?inlineHint|/?hint|/?ref|/?cursor|/?block|/?html|/?inject|/?rainbow|/?TYPO|/?text_attr|/?caretMarker|/?expr)\b'
                    rb'|<!\w*(?:[:(][^>]*)?!>|<!>')
foreign = re.compile(rb'^// FILE: \S+\.(?!java\s*$)\w+\s*$', re.M)
files = subprocess.run(['git', '-C', root, 'ls-files', '-z', '--', '*.java'], capture_output=True, check=True).stdout.split(b'\0')
for f in files:
    if not f:
        continue
    try:
        data = open(root.encode() + b'/' + f, 'rb').read()
    except OSError:
        continue
    if not markup.search(data) and not foreign.search(data):
        print(f.decode())
