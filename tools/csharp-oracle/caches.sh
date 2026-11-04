#!/bin/sh
# Rebuilds the C# caches under ~/.cache/psi-syntax (README.md, "Caches"):
#   tools/csharp-oracle/caches.sh [clones] [lists] [dumps] [lex] [fuzz] [defines] [directives] [getters]   (no step: all, in this
#   order)
# clones: the pinned corpora under csharp-src/ (shallow, checked out); lists: corpus/csharp-REPO-SHA7.list;
# dumps: oracle/csharp-REPO-SHA7/ (Latest, Parse) and oracle/csharp-REPO-SHA7-preview/ (Preview, Parse, files equal
# to Latest's hard-linked); lex: lex/csharp-REPO-SHA7.oracle.txt (--doc none), .doc.oracle.txt (--doc parse) and
# .diag.oracle.txt (--doc diagnose);
# fuzz: the golden mutations (their dumps at their goldens' versions, at Latest and at Preview) and the lexer fuzz
# (dumps at Latest and at Preview) with their lexer lines;
# defines: of the roslyn and Avalonia corpora, the files whose #if and #elif conditions name a symbol, with every one
# of those symbols defined (defines.py): corpus/csharp-REPO-SHA7-defines.list, its dumps oracle/csharp-REPO-SHA7-defines/
# (Latest, Parse) and lexer lines lex/csharp-REPO-SHA7-defines.doc.oracle.txt (--doc parse);
# directives: each directive's IsActive, BranchTaken and ConditionValue (`directives`) of the four corpora, the
# -defines lists, the golden mutations and the lexer fuzz, with the lists' own columns: directives/KEY.txt (after
# fuzz and defines);
# getters: the getter lines (`oracle.sh getters`) under getters/csharp/: of the goldens, of the four corpora at Latest
# and at Preview (runtime without the 6 files whose dump is over 512 MB, getters/csharp/csharp-runtime-SHA7.list), of
# the roslyn corpus at --doc diagnose and none, of the -defines lists, of the mutations at their three versions, of the
# lexer fuzz at Latest and Preview, and of the getters fuzz (getters_fuzz.py, fuzz/csharp-getters/, its list without
# the files the oracle has no lines for: those its entry cannot parse and one Roslyn throws on). Keeps at least 200 GB
# free.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
S=$HOME/.cache/psi-syntax
O=$here/oracle.sh
CORPORA="roslyn https://github.com/dotnet/roslyn 35d9211b841e7613c1d2f8f5af6d628ace696c4c
runtime https://github.com/dotnet/runtime v10.0.14
aspnetcore https://github.com/dotnet/aspnetcore v10.0.14
avalonia https://github.com/AvaloniaUI/Avalonia 12.1.4"
DEFINES="roslyn avalonia"
steps=${*:-clones lists dumps lex fuzz defines directives getters}

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
  rm -rf "$F/csharp-mut" "$F/csharp-mut.oracle" "$F/csharp-mut-latest.oracle" "$F/csharp-mut-preview.oracle" \
    "$F/csharp-lex" "$F/csharp-lex.oracle" "$F/csharp-lex-preview.oracle"
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
  # the same mutants at Latest and at Preview, each dump equal to its goldens' version's a hard link to it
  for v in latest preview; do
    awk -F '\t' -v v=$v 'BEGIN { OFS = "\t" } { $2 = v; print }' "$F/csharp-mut.list" > "$F/csharp-mut-$v.list"
    "$O" dump --list "$F/csharp-mut-$v.list" --root "$F/csharp-mut" --out "$F/csharp-mut-$v.oracle"
    "$here/link_same.py" "$F/csharp-mut.oracle" "$F/csharp-mut-$v.oracle"
  done
  "$here/lex_fuzz.py" "$repo/testdata/csharp" "$F/csharp-lex" 20000 csharp-lex
  "$O" dump --list "$F/csharp-lex/list" --root "$F/csharp-lex" --out "$F/csharp-lex.oracle"
  "$O" dump --list "$F/csharp-lex/list" --root "$F/csharp-lex" --out "$F/csharp-lex-preview.oracle" --version preview
  "$here/link_same.py" "$F/csharp-lex.oracle" "$F/csharp-lex-preview.oracle"
  cut -f1 "$F/csharp-mut.list" > "$F/csharp-mut.paths"
  for doc in none parse diagnose; do
    suffix=$(case $doc in (parse) echo .doc ;; (diagnose) echo .diag ;; esac)
    "$O" lex --list "$F/csharp-mut.paths" --root "$F/csharp-mut" --doc $doc > "$S/lex/csharp-mut$suffix.oracle.txt"
    "$O" lex --list "$F/csharp-lex/list" --root "$F/csharp-lex" --doc $doc > "$S/lex/csharp-lex$suffix.oracle.txt"
  done
  rm "$F/csharp-mut.paths"
}

defines() {
  k=$(key "$1")-defines
  need_space
  python3 -I "$here/defines.py" "$S/corpus/$(key "$1").list" "$S/csharp-src/$1" > "$S/corpus/$k.list"
  rm -rf "$S/oracle/$k"
  "$O" dump --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --out "$S/oracle/$k"
  "$O" lex --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" --doc parse > "$S/lex/$k.doc.oracle.txt"
}

directives() {
  k=$(key "$1")
  "$O" directives --list "$S/corpus/$k.list" --root "$S/csharp-src/$1" > "$S/directives/$k.txt"
  case " $DEFINES " in
    *" $1 "*)
      "$O" directives --list "$S/corpus/$k-defines.list" --root "$S/csharp-src/$1" > "$S/directives/$k-defines.txt" ;;
  esac
}

getters() {
  G=$S/getters/csharp
  F=$S/fuzz
  mkdir -p "$G"
  awk -F '\t' 'BEGIN { OFS = "\t" } { print $1 ".cs", $2, $3, $4, $5 }' "$repo/testdata/csharp/manifest.tsv" > "$G/goldens.list"
  "$O" getters --list "$G/goldens.list" --root "$repo/testdata/csharp" > "$G/csharp-goldens.txt"
  echo "$CORPORA" | while read -r name url ref; do
    k=$(key "$name")
    list=$S/corpus/$k.list
    if [ "$name" = runtime ]; then
      # the files whose dump is over 512 MB have no getter lines either (only lossless, as in the gate's dumps)
      grep '"failure"' "$S/oracle/$k/index.jsonl" | sed 's/^{"path":"\([^"]*\)".*/\1/' | grep -vxFf - "$list" > "$G/$k.list"
      list=$G/$k.list
    fi
    "$O" getters --list "$list" --root "$S/csharp-src/$name" > "$G/$k.txt"
    "$O" getters --list "$list" --root "$S/csharp-src/$name" --version preview > "$G/$k-preview.txt"
  done
  for name in $DEFINES; do
    k=$(key "$name")-defines
    "$O" getters --list "$S/corpus/$k.list" --root "$S/csharp-src/$name" > "$G/$k.txt"
  done
  k=$(key roslyn)
  "$O" getters --list "$S/corpus/$k.list" --root "$S/csharp-src/roslyn" --doc diagnose > "$G/$k-diagnose.txt"
  "$O" getters --list "$S/corpus/$k.list" --root "$S/csharp-src/roslyn" --doc none > "$G/$k-nodoc.txt"
  for m in csharp-mut csharp-mut-latest csharp-mut-preview; do
    "$O" getters --list "$F/$m.list" --root "$F/csharp-mut" > "$G/$m.txt"
  done
  "$O" getters --list "$F/csharp-lex/list" --root "$F/csharp-lex" > "$G/csharp-lex.txt"
  "$O" getters --list "$F/csharp-lex/list" --root "$F/csharp-lex" --version preview > "$G/csharp-lex-preview.txt"
  rm -rf "$F/csharp-getters"
  python3 -I "$here/getters_fuzz.py" "$repo/testdata/csharp" "$F/csharp-getters" 20000 csharp-getters
  "$O" getters --list "$F/csharp-getters/list" --root "$F/csharp-getters" > "$G/csharp-getters.txt"
  # the files the oracle has no lines for: no member at the member entry, and Roslyn's own exception (1 file)
  awk -F '\t' 'NR == FNR { if ($2 == "failure") failed[$1] = 1; next } !($1 in failed)' \
    "$G/csharp-getters.txt" "$F/csharp-getters/list" > "$F/csharp-getters.list"
  awk -F '\t' '$2 != "failure"' "$G/csharp-getters.txt" > "$G/csharp-getters.ok.txt"
  mv "$G/csharp-getters.ok.txt" "$G/csharp-getters.txt"
}

mkdir -p "$S/csharp-src" "$S/corpus" "$S/oracle" "$S/lex" "$S/fuzz" "$S/directives"
for step in $steps; do
  case $step in
    clones) each clone ;;
    lists) each list ;;
    dumps) each dump ;;
    lex) each lex ;;
    fuzz) fuzz ;;
    defines) for corpus in $DEFINES; do defines "$corpus"; done ;;
    directives)
      each directives
      "$O" directives --list "$S/fuzz/csharp-mut.list" --root "$S/fuzz/csharp-mut" > "$S/directives/csharp-mut.txt"
      "$O" directives --list "$S/fuzz/csharp-lex/list" --root "$S/fuzz/csharp-lex" > "$S/directives/csharp-lex.txt" ;;
    getters) getters ;;
    *) echo "caches.sh: unknown step $step" >&2; exit 2 ;;
  esac
done
