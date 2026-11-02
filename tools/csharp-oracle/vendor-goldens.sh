#!/bin/sh
# Writes testdata/csharp from Roslyn's parser and lexer tests at the pin (README.md, "Goldens"):
#   tools/csharp-oracle/vendor-goldens.sh [ROSLYN_CLONE]
# The tests are read with `git archive` of the pinned commit (never a working tree), parsed by Roslyn itself, and
# every recognized input is written as NAME.cs with the oracle's NAME.tree.txt and a manifest.tsv line.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
clone=${1:-$HOME/.cache/psi-syntax/roslyn}
pin=35d9211b841e7613c1d2f8f5af6d628ace696c4c
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git -C "$clone" archive "$pin" src/Compilers/CSharp/Test/Syntax/Parsing src/Compilers/CSharp/Test/Syntax/LexicalAndXml |
  tar -x -C "$tmp"
"$here/oracle.sh" goldens --src "$tmp" --out "$repo/testdata/csharp" --skipped "$tmp/skipped.tsv"
