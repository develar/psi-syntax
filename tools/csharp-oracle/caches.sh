#!/bin/sh
# Rebuilds the C# caches under ~/.cache/psi-syntax (README.md, "Caches"):
#   tools/csharp-oracle/caches.sh [clones] [lists] [dumps] [lex] [fuzz]     (no step: all, in this order)
# clones: the pinned corpora under csharp-src/ (shallow, checked out); lists: corpus/csharp-REPO-SHA7.list;
# dumps: oracle/csharp-REPO-SHA7/ (Latest, Parse) and oracle/csharp-REPO-SHA7-preview/ (Preview, Parse, files equal
# to Latest's hard-linked); lex: lex/csharp-REPO-SHA7.oracle.txt (--doc none), .doc.oracle.txt (--doc parse) and
# .diag.oracle.txt (--doc diagnose);
# fuzz: the golden mutations and the lexer fuzz with their dumps and lexer lines. Keeps at least 200 GB free.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
S=$HOME/.cache/psi-syntax
O=$here/oracle.sh
CORPORA="roslyn https://github.com/dotnet/roslyn 35d9211b841e7613c1d2f8f5af6d628ace696c4c
runtime https://github.com/dotnet/runtime v10.0.14
aspnetcore https://github.com/dotnet/aspnetcore v10.0.14
avalonia https://github.com/AvaloniaUI/Avalonia 12.1.4"
steps=${*:-clones lists dumps lex fuzz}

need_space() {
  free=$(df -k "$S" | awk 'NR == 2 { print int($4 / 1024 / 1024) }')
  if [ "$free" -lt 220 ]; then
    echo "caches.sh: only $free GB free under $S, keeping 200 GB free" >&2
    exit 1
  fi
}

each() { # each FUNCTION: FUNCTION NAME URL REF for every corpus
  echo "$CORPORA" | while read -r name url ref; do "$1" "$name" "$url" "$ref"; done
}

key() { echo "csharp-$1-$(git -C "$S/csharp-src/$1" rev-parse --short=7 HEAD)"; }

clone() {
  dir=$S/csharp-src/$1
  [ -d "$dir/.git" ] && return
  case $3 in
    v* | [0-9]*.*) git clone -q --depth 1 --branch "$3" --single-branch "$2" "$dir" ;;
    *) git init -q "$dir" && git -C "$dir" remote add origin "$2" && git -C "$dir" fetch -q --depth 1 origin "$3" &&
      git -C "$dir" checkout -q FETCH_HEAD ;;
  esac
  echo "$1 $(git -C "$dir" rev-parse HEAD)"
}

list() { "$here/corpus.py" "$S/csharp-src/$1" > "$S/corpus/$(key "$1").list"; }

dump() {
  k=$(key "$1")
  need_space
  rm -rf "$S/oracle/$k" "$S/oracle/$k-preview"
  "$O" dump --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --out "$S/oracle/$k"
  "$O" dump --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --out "$S/oracle/$k-preview" --version preview
  "$here/link_same.py" "$S/oracle/$k" "$S/oracle/$k-preview"
}

lex() {
  k=$(key "$1")
  "$O" lex --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --doc none > "$S/lex/$k.oracle.txt"
  "$O" lex --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --doc parse > "$S/lex/$k.doc.oracle.txt"
  "$O" lex --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --doc diagnose > "$S/lex/$k.diag.oracle.txt"
}

fuzz() {
  need_space
  F=$S/fuzz
  rm -rf "$F/csharp-mut" "$F/csharp-mut.oracle" "$F/csharp-lex" "$F/csharp-lex.oracle"
  "$here/mutate.py" "$F/csharp-mut"
  "$O" dump --list "$F/csharp-mut/list" --root "$F/csharp-mut" --out "$F/csharp-mut.oracle"
  # A mutant its entry cannot parse (ParseMemberDeclaration returns null) has no dump: not in the oracle list.
  python3 - "$F/csharp-mut/list" "$F/csharp-mut.oracle/index.jsonl" > "$F/csharp-mut.list" <<'PY'
import json, sys
failed = {json.loads(l)["path"] for l in open(sys.argv[2], encoding="utf-8") if "failure" in json.loads(l)}
sys.stdout.write("".join(l for l in open(sys.argv[1], encoding="utf-8") if l.split("\t")[0] not in failed))
PY
  rm -rf "$F/csharp-mut.oracle"
  "$O" dump --list "$F/csharp-mut.list" --root "$F/csharp-mut" --out "$F/csharp-mut.oracle"
  "$here/lex_fuzz.py" "$repo/testdata/csharp" "$F/csharp-lex" 20000 csharp-lex
  "$O" dump --list "$F/csharp-lex/list" --root "$F/csharp-lex" --out "$F/csharp-lex.oracle"
  cut -f1 "$F/csharp-mut.list" > "$F/csharp-mut.paths"
  for doc in none parse diagnose; do
    suffix=$(case $doc in parse) echo .doc ;; diagnose) echo .diag ;; esac)
    "$O" lex --list "$F/csharp-mut.paths" --root "$F/csharp-mut" --doc $doc > "$S/lex/csharp-mut$suffix.oracle.txt"
    "$O" lex --list "$F/csharp-lex/list" --root "$F/csharp-lex" --doc $doc > "$S/lex/csharp-lex$suffix.oracle.txt"
  done
  rm "$F/csharp-mut.paths"
}

mkdir -p "$S/csharp-src" "$S/corpus" "$S/oracle" "$S/lex" "$S/fuzz"
for step in $steps; do
  case $step in
    clones) each clone ;;
    lists) each list ;;
    dumps) each dump ;;
    lex) each lex ;;
    fuzz) fuzz ;;
    *) echo "caches.sh: unknown step $step" >&2; exit 2 ;;
  esac
done
