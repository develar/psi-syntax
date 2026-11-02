#!/bin/sh
# The correctness gate every change must pass, byte for byte against Kotlin 2.4.21's PSI and IntelliJ 271's Java PSI:
#   1. clippy over the workspace (default and with ksdump's opt-in `counters,alloc-stats` features) and
#      KS_STRICT=1 cargo test --release over the workspace (goldens, lexer, kdoc, robustness);
#   2. the 55 501 community files: `diff -rq` of our dumps against the cached oracle dumps, 0 lines;
#   3. cached fuzz corpora (parse seeds 11-12, lex seeds 1-2, 3000 files each, and the 16 724 one-place mutations of
#      psi-tools 4d21cba's tools/psi-gate/mutate.js over crates/kotlin-syntax/testdata/psi) against their cached oracle answers;
#   4. body-less parsing on the community files: `parse_body` of every unexpanded body dumps as the full parse's subtree;
#   5. the too-deep paths: the community files and the mutations parsed with nesting limit 3 (`--nesting-limit`), every
#      tree lossless and every body's `parse_body` its full parse's subtree;
#   6. Java's lexers (`ksdump --lex`) on the 68 263 community .java files, the Java golden mutations and the Java lexer
#      fuzz against the cached lines of tools/java-oracle/JavaLexOracle.java;
#   7. Java's parsers against IntelliJ 271's Java PSI (tools/java-oracle/JavaPsiOracle.java): the 1036 goldens of
#      testdata/java (step 1, strict), and `diff -rq` of both dumps (.psi.txt, .node.txt) of the 68 260 UTF-8 community
#      .java files, of the 19 116 one-place mutations of the goldens and of the 20 000 lexer fuzz inputs against the
#      cached oracle dumps, 0 lines;
#   8. Java's bodies and too-deep paths: `parse_body` of every unexpanded code block of the community .java files
#      against the full parse, and the same with nesting limit 3 on the community .java files and the golden mutations,
#      every tree lossless;
#   9. C#'s lexer (`ksdump --lex --lang csharp --doc none`, Roslyn 5.9.0's `SyntaxFactory.ParseTokens` with
#      directives and diagnostics) on the four corpora of tools/csharp-oracle (dotnet/roslyn, dotnet/runtime,
#      dotnet/aspnetcore, Avalonia), its golden mutations and its lexer fuzz against the cached lines of
#      `tools/csharp-oracle/oracle.sh lex --doc none`, and the same with `--doc parse` (documentation comments
#      parsed into structured trivia by the documentation comment parser) and `--doc diagnose` (with its XML errors)
#      against `lex --doc parse`'s and `lex --doc diagnose`'s, 0 differing lines;
#  10. C#'s parser against Roslyn 5.9.0's trees (`tools/csharp-oracle/oracle.sh dump`): the 7051 goldens of
#      testdata/csharp and the reduced cases of crates/csharp-syntax/tests/oracle (step 1, strict), and `diff -rq` of
#      our dumps against the cached oracle dumps of the four corpora at Latest and at Preview, of the golden
#      mutations at their goldens' versions, at Latest and at Preview, and of the lexer fuzz at Latest and at Preview,
#      0 lines; the 6 runtime files whose dump is over 512 MB (no oracle dump) parse without a panic into a tree
#      whose leaves are the text (`ksdump --check-bodies --lang csharp`).
# Prints PASS or the first failures and exits nonzero. The caches are made once (see the fuzz scripts' docstrings).
# Our dumps go to a fresh directory per run under ~/.cache/psi-syntax/ours/ (concurrent sessions share the machine),
# removed on PASS and kept, with its path printed, on FAIL.
set -u
cd "$(dirname "$0")/.."
C=$HOME/.cache/psi-syntax
ROOT=/Users/develar/.cache/accio-bench/intellij-community
fail=0
cargo clippy --release --workspace --all-targets -q -- -D warnings || fail=1
cargo clippy --release --workspace --all-targets --features ksdump/counters,ksdump/alloc-stats -q -- -D warnings || fail=1
KS_STRICT=1 cargo test --release --workspace -q 2>&1 | grep -E "^test result|goldens:|FAILED|panicked" || true
KS_STRICT=1 cargo test --release --workspace -q > /dev/null 2>&1 || { echo "FAIL tests"; fail=1; }
cargo build --release --workspace -q || exit 1
B=target/release/ksdump
mkdir -p "$C/ours"
O=$(mktemp -d "$C/ours/gate.XXXXXX") || exit 1
$B --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" --out "$O/community" > /dev/null
n=$(diff -rq -x index.jsonl "$C/oracle/community-3caa0bf" "$O/community" | wc -l | tr -d ' ')
echo "community: $n differing of 55501"; [ "$n" = 0 ] || { diff -rq -x index.jsonl "$C/oracle/community-3caa0bf" "$O/community" | head -5; fail=1; }
for s in 11 12; do
  $B --list "$C/fuzz/parse-$s/list" --root "$C/fuzz/parse-$s" --out "$O/parse-$s" > /dev/null
  n=$(diff -rq -x index.jsonl "$C/fuzz/parse-$s.oracle" "$O/parse-$s" | wc -l | tr -d ' ')
  echo "parse-fuzz $s: $n differing of 3000"; [ "$n" = 0 ] || fail=1
done
$B --list "$C/fuzz/mut330.list" --root "$C/fuzz/mut330" --out "$O/mut330" > /dev/null
n=$(diff -rq -x index.jsonl "$C/fuzz/mut330.oracle" "$O/mut330" | wc -l | tr -d ' ')
echo "mutations (psi-tools 4d21cba mutate.js over crates/kotlin-syntax/testdata/psi): $n differing of 16724"; [ "$n" = 0 ] || fail=1
for s in 1 2; do
  n=$($B --lex --list "$C/fuzz/lex-$s/list" --root "$C/fuzz/lex-$s" | cut -f1,2 | diff - "$C/fuzz/lex-$s.oracle" | grep -c '^[<>]')
  echo "lex-fuzz $s: $n differing lines"; [ "$n" = 0 ] || fail=1
done
# Java: JavaLexer and JavaDocLexer against tools/java-oracle/JavaLexOracle.java's cached lines (the community .java
# files, the one-place mutations of the Java goldens, the lexer fuzz of tools/java-oracle/lex_fuzz.py).
for corpus in "corpus/java-community-3caa0bf.list $ROOT java-community-3caa0bf" "fuzz/java-mut.list $C/fuzz/java-mut java-mut" \
  "fuzz/java-lex/list $C/fuzz/java-lex java-lex"; do
  set -- $corpus
  n=$($B --lex --list "$C/$1" --root "$2" | diff - "$C/lex/$3.oracle.txt" | grep -c '^[<>]')
  echo "java lex $3: $n differing lines of $(wc -l < "$C/lex/$3.oracle.txt" | tr -d ' ')"; [ "$n" = 0 ] || fail=1
done
# Java: both dumps (`--node`) of the community .java files, the golden mutations and the lexer fuzz against
# JavaPsiOracle.java's cached ones (the 3 community files that are not UTF-8 are failures on both sides: no dump).
for corpus in "corpus/java-community-3caa0bf.list $ROOT oracle/java-community-3caa0bf 68260" \
  "fuzz/java-mut.list $C/fuzz/java-mut fuzz/java-mut.oracle 19116" \
  "fuzz/java-lex/list $C/fuzz/java-lex fuzz/java-lex.oracle 20000"; do
  set -- $corpus
  $B --node --list "$C/$1" --root "$2" --out "$O/$(basename "$3")" > /dev/null 2>&1
  n=$(diff -rq -x index.jsonl "$C/$3" "$O/$(basename "$3")" | wc -l | tr -d ' ')
  echo "java psi $(basename "$3"): $n differing of $4"; [ "$n" = 0 ] || { diff -rq -x index.jsonl "$C/$3" "$O/$(basename "$3")" | head -5; fail=1; }
done
bodies=$($B --check-bodies --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" | tail -1)
echo "check-bodies: $bodies"; case "$bodies" in *"differing 0 "*) ;; *) fail=1 ;; esac
for corpus in "$C/corpus/community-3caa0bf.list $ROOT" "$C/fuzz/mut330.list $C/fuzz/mut330"; do
  set -- $corpus
  deep=$($B --check-bodies --nesting-limit 3 --list "$1" --root "$2" | tail -1)
  echo "nesting limit 3, $(basename "$1"): $deep"; case "$deep" in *"differing 0 "*) ;; *) fail=1 ;; esac
done
bodies=$($B --check-bodies --list "$C/corpus/java-community-3caa0bf.list" --root "$ROOT" | tail -1)
echo "java check-bodies: $bodies"; case "$bodies" in *"differing 0 "*) ;; *) fail=1 ;; esac
for corpus in "$C/corpus/java-community-3caa0bf.list $ROOT" "$C/fuzz/java-mut.list $C/fuzz/java-mut"; do
  set -- $corpus
  deep=$($B --check-bodies --nesting-limit 3 --list "$1" --root "$2" | tail -1)
  echo "java nesting limit 3, $(basename "$1"): $deep"; case "$deep" in *"differing 0 "*) ;; *) fail=1 ;; esac
done
# C#: the lexer against tools/csharp-oracle's cached `lex --doc none` lines (the four corpora, the golden mutations,
# the lexer fuzz; the mutations' lines were made from the list's paths alone, the defaults for every file).
cut -f1 "$C/fuzz/csharp-mut.list" > "$O/csharp-mut.paths"
for corpus in "corpus/csharp-roslyn-35d9211.list $C/csharp-src/roslyn csharp-roslyn-35d9211" \
  "corpus/csharp-runtime-6aa72da.list $C/csharp-src/runtime csharp-runtime-6aa72da" \
  "corpus/csharp-aspnetcore-0ef4bbf.list $C/csharp-src/aspnetcore csharp-aspnetcore-0ef4bbf" \
  "corpus/csharp-avalonia-2d79ba5.list $C/csharp-src/avalonia csharp-avalonia-2d79ba5" \
  "$O/csharp-mut.paths $C/fuzz/csharp-mut csharp-mut" "fuzz/csharp-lex/list $C/fuzz/csharp-lex csharp-lex"; do
  set -- $corpus
  case "$1" in /*) list=$1 ;; *) list=$C/$1 ;; esac
  n=$($B --lex --lang csharp --doc none --list "$list" --root "$2" | diff - "$C/lex/$3.oracle.txt" | grep -c '^[<>]')
  echo "csharp lex $3: $n differing lines of $(wc -l < "$C/lex/$3.oracle.txt" | tr -d ' ')"; [ "$n" = 0 ] || fail=1
  for doc in parse diagnose; do
    lines=$C/lex/$3.$([ $doc = parse ] && echo doc || echo diag).oracle.txt
    n=$($B --lex --lang csharp --doc $doc --list "$list" --root "$2" | diff - "$lines" | grep -c '^[<>]')
    echo "csharp lex --doc $doc $3: $n differing lines of $(wc -l < "$lines" | tr -d ' ')"; [ "$n" = 0 ] || fail=1
  done
done
# C#: the parser's dumps against tools/csharp-oracle's cached `dump` trees. A corpus's dumps are removed once equal
# (runtime's are 6.9 GB).
for v in latest preview; do
  sfx=$([ $v = preview ] && echo -preview)
  for corpus in "csharp-roslyn-35d9211 roslyn" "csharp-runtime-6aa72da runtime" "csharp-aspnetcore-0ef4bbf aspnetcore" \
    "csharp-avalonia-2d79ba5 avalonia"; do
    set -- $corpus
    $B --lang csharp --version $v --list "$C/corpus/$1.list" --root "$C/csharp-src/$2" --out "$O/$1$sfx" > /dev/null 2>&1
    n=$(diff -rq -x index.jsonl "$C/oracle/$1$sfx" "$O/$1$sfx" | wc -l | tr -d ' ')
    echo "csharp parse $1$sfx: $n differing of $(wc -l < "$C/corpus/$1.list" | tr -d ' ')"
    if [ "$n" = 0 ]; then rm -rf "$O/$1$sfx"; else diff -rq -x index.jsonl "$C/oracle/$1$sfx" "$O/$1$sfx" | head -5; fail=1; fi
  done
done
# the golden mutations at their goldens' versions (the list's column), at Latest and at Preview (the same mutants)
for m in csharp-mut csharp-mut-latest csharp-mut-preview; do
  $B --lang csharp --list "$C/fuzz/$m.list" --root "$C/fuzz/csharp-mut" --out "$O/$m" > /dev/null 2>&1
  n=$(diff -rq -x index.jsonl "$C/fuzz/$m.oracle" "$O/$m" | wc -l | tr -d ' ')
  echo "csharp parse $m: $n differing of $(wc -l < "$C/fuzz/$m.list" | tr -d ' ')"
  [ "$n" = 0 ] || { diff -rq -x index.jsonl "$C/fuzz/$m.oracle" "$O/$m" | head -5; fail=1; }
done
for v in latest preview; do
  sfx=$([ $v = preview ] && echo -preview)
  $B --lang csharp --version $v --list "$C/fuzz/csharp-lex/list" --root "$C/fuzz/csharp-lex" --out "$O/csharp-lex$sfx" > /dev/null 2>&1
  n=$(diff -rq -x index.jsonl "$C/fuzz/csharp-lex$sfx.oracle" "$O/csharp-lex$sfx" | wc -l | tr -d ' ')
  echo "csharp parse csharp-lex$sfx: $n differing of 20000"
  [ "$n" = 0 ] || { diff -rq -x index.jsonl "$C/fuzz/csharp-lex$sfx.oracle" "$O/csharp-lex$sfx" | head -5; fail=1; }
done
grep '"failure":"dump over 512 MB"' "$C/oracle/csharp-runtime-6aa72da/index.jsonl" | sed 's/^{"path":"\([^"]*\)".*/\1/' \
  > "$O/csharp-huge.list"
huge=$($B --check-bodies --lang csharp --list "$O/csharp-huge.list" --root "$C/csharp-src/runtime" | tail -1)
echo "csharp lossless, the runtime files without a dump: $huge"; [ "$huge" = "differing 0  files 6" ] || fail=1
[ $fail = 0 ] && { rm -rf "$O"; echo PASS; } || { echo "FAIL (our dumps: $O)"; exit 1; }
