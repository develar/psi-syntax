#!/bin/sh
# The correctness gate every change must pass, byte for byte against Kotlin 2.4.21's PSI and IntelliJ 271's Java PSI:
#   1. clippy over psi-syntax's crates (default and with ksdump's opt-in `counters,alloc-stats` features) and
#      KS_STRICT=1 cargo nextest run --release over them (goldens, lexer, kdoc, robustness), then their doc tests;
#   2. the 55 501 community files: our dumps against the cached oracle dumps, 0 differing;
#   3. cached fuzz corpora (parse seeds 11-12, lex seeds 1-2, 3000 files each, and the 16 724 one-place mutations of
#      psi-tools 4d21cba's tools/psi-gate/mutate.js over crates/kotlin-syntax/testdata/psi) against their cached oracle answers;
#   4. body-less parsing on the community files: `parse_body` of every unexpanded body dumps as the full parse's subtree;
#   5. the too-deep paths: the community files and the mutations parsed with nesting limit 3 (`--nesting-limit`), every
#      tree lossless and every body's `parse_body` its full parse's subtree;
#   6. Java's lexers (`ksdump --lex`) on the 68 263 community .java files, the Java golden mutations and the Java lexer
#      fuzz against the cached lines of tools/java-oracle/JavaLexOracle.java;
#   7. Java's parsers against IntelliJ 271's Java PSI (tools/java-oracle/JavaPsiOracle.java): the 1036 goldens of
#      testdata/java (step 1, strict), and both dumps (.psi.txt, .node.txt) of the 68 260 UTF-8 community .java files,
#      of the 19 116 one-place mutations of the goldens and of the 20 000 lexer fuzz inputs against the cached oracle
#      dumps, 0 differing;
#      The PSI getters of `java_syntax::psi` (`ksdump --getters`, every getter of every node) against the cached
#      lines of tools/java-oracle/JavaPsiOracle.java --getters (the real PSI's) on the goldens and the same three
#      corpora, 0 differing (an answer IntelliJ throws on, `!`, takes any of ours);
#   8. Java's bodies and too-deep paths: `parse_body` of every unexpanded code block of the community .java files
#      against the full parse, and the same with nesting limit 3 on the community .java files and the golden mutations,
#      every tree lossless;
#   9. C#'s lexer (`ksdump --lex --lang csharp --doc none`, Roslyn 5.9.0's `SyntaxFactory.ParseTokens` with
#      directives and diagnostics) on the four corpora of tools/csharp-oracle (dotnet/roslyn, dotnet/runtime,
#      dotnet/aspnetcore, Avalonia), its golden mutations and its lexer fuzz against the cached lines of
#      `tools/csharp-oracle/oracle.sh lex --doc none`, and the same with `--doc parse` (documentation comments
#      parsed into structured trivia by the documentation comment parser) and `--doc diagnose` (with its XML errors)
#      against `lex --doc parse`'s and `lex --doc diagnose`'s, 0 differing lines; and `Tree::value_text` (`ksdump
#      --values --check`) on the four corpora, the golden mutations at their list's columns and the lexer fuzz against
#      the cached lines of `tools/csharp-oracle/oracle.sh values --kept` (Roslyn's `ValueText` where the port keeps
#      it), 0 differing lines;
#  10. C#'s parser against Roslyn 5.9.0's trees (`tools/csharp-oracle/oracle.sh dump`): the 7064 goldens of
#      testdata/csharp and the reduced cases of crates/csharp-syntax/tests/oracle (step 1, strict), and our dumps
#      against the cached oracle dumps of the four corpora at Latest and at Preview, of the golden mutations at their
#      goldens' versions, at Latest and at Preview, and of the lexer fuzz at Latest and at Preview, 0 differing; the 6
#      runtime files whose dump is over 512 MB (no oracle dump) parse without a panic into a tree whose leaves are the
#      text (`ksdump --check-bodies --lang csharp`);
#  11. C# with preprocessor symbols (`CSharpParseOptions.WithPreprocessorSymbols`, the list's SYMBOLS column): the files
#      of the roslyn and Avalonia corpora whose #if and #elif conditions name a symbol, with every one of them defined
#      (tools/csharp-oracle/defines.py), our dumps and lexer lines (`--doc parse`) against the cached oracle's, 0
#      differing;
#  12. C#'s directives (`Tree::directives`, `ksdump --directives --check`): each directive's IsActive, BranchTaken and
#      ConditionValue on the four corpora, the -defines lists of step 11, the golden mutations at their list's columns
#      and the lexer fuzz against the cached lines of `tools/csharp-oracle/oracle.sh directives` (Roslyn's directive
#      trivia), 0 differing lines; the goldens' are step 1's (crates/csharp-syntax/tests/directives.rs).
#  13. the syntax getters of `csharp_syntax::syntax_getters` (`ksdump --lang csharp --getters --check`, every syntax
#      node property on every node) against the cached lines of `tools/csharp-oracle/oracle.sh getters` (Roslyn's own
#      properties): the goldens, the four corpora at Latest and at Preview (the 6 runtime files without a dump left
#      out), the roslyn corpus at `--doc diagnose` and `--doc none`, the -defines lists, the golden mutations at their
#      three versions, the lexer fuzz at Latest and at Preview and the getters fuzz, 0 differing lines.
# A dump comparison is `ksdump --check ORACLE`: every dump made in memory and compared byte for byte with the oracle's
# file, what `diff -rq -x index.jsonl ORACLE OURS` checked of written dumps (a dump that differs, one that either side
# lacks, an oracle directory without dumps; index.jsonl is not compared, so a file the oracle failed on must have no
# dump of ours either). A lexer comparison is `ksdump --lex --check LINES`, line by line against the cached lines (a
# Kotlin line's first two columns, as LexOracle.java prints them). Each runs on every core, one parser per thread; a new
# corpus is one line calling `dumps`, `lex`, `values`, `directives` or `bodies` in its language's section. The sections
# run in three lanes at once (see `lane` below).
# Prints PASS or the first failures and exits nonzero. The caches are made once (see the fuzz scripts' docstrings;
# the value text lines and the directive lines: tools/csharp-oracle/README.md, "Value texts", "Directives").
# Each run's logs and differing dumps go to a fresh directory under ~/.cache/psi-syntax/ours/ (concurrent sessions
# share the machine), removed on PASS and kept, with its path printed, on FAIL.
set -u
cd "$(dirname "$0")/.."
C=$HOME/.cache/psi-syntax
ROOT=/Users/develar/.cache/accio-bench/intellij-community
mkdir -p "$C/ours"
O=$(mktemp -d "$C/ours/gate.XXXXXX") || exit 1
# psi-syntax's crates of accio's workspace; ksdump built with the `psi` profile into accio's target directory.
P="-p psi-builder -p kotlin-syntax -p java-syntax -p csharp-syntax -p ksdump"
fail=0
# shellcheck disable=SC2086 # one argument per package
cargo clippy --release $P --all-targets -q -- -D warnings || fail=1
# shellcheck disable=SC2086
cargo clippy --release $P --all-targets --features ksdump/counters,ksdump/alloc-stats -q -- -D warnings || fail=1
# The tests stay on `release`, the profile their time limits were measured on; a passing test's output is printed at
# the end (`--success-output final`) for the goldens' counts. nextest skips doc tests: the libraries' run after it.
# shellcheck disable=SC2086
{ KS_STRICT=1 cargo nextest run --release $P --cargo-quiet --status-level fail --success-output final \
  && cargo test --release -p psi-builder -p kotlin-syntax -p java-syntax -p csharp-syntax --doc -q; } > "$O/tests.log" 2>&1 \
  || { tests=FAIL; fail=1; }
# nextest's status lines (`FAIL [ 0.1s]`), not a passing test's own `FAIL path` lines (Kotlin's known stale golden).
grep -E "goldens:|^ *(FAIL|SIGSEGV|SIGABRT|TIMEOUT) +\[|^ *Summary|^test result: FAILED|FAILED|panicked|^error" "$O/tests.log"
[ "${tests:-}" = FAIL ] && echo "FAIL tests"
cargo build --profile psi -p ksdump -q || exit 1
B=${CARGO_TARGET_DIR:-../target}/psi/ksdump

# report NAME STATUS: the last line of $O/NAME.log after NAME, and the first lines before it when STATUS is not 0.
report() {
  echo "$1: $(tail -1 "$O/$1.log")"
  [ "$2" = 0 ] || { sed '$d' "$O/$1.log" | head -5; return 1; }
}
# dumps NAME ORACLE ARGS...: `ksdump --check ORACLE ARGS...`, the differing dumps of ours kept in $O/NAME.
dumps() {
  name=$1 oracle=$2; shift 2
  $B --check "$oracle" --out "$O/$name" "$@" > "$O/$name.log" 2>&1
  report "$name" $?
}
# lex NAME LINES ARGS...: `ksdump --lex --check LINES ARGS...`.
lex() {
  name=$1 lines=$2; shift 2
  $B --lex --check "$lines" "$@" > "$O/$name.log" 2>&1
  report "$name" $?
}
# values NAME LINES ARGS...: `ksdump --values --check LINES ARGS...`.
values() {
  name=$1 lines=$2; shift 2
  $B --values --check "$lines" "$@" > "$O/$name.log" 2>&1
  report "$name" $?
}
# directives NAME LINES ARGS...: `ksdump --directives --check LINES ARGS...`.
directives() {
  name=$1 lines=$2; shift 2
  $B --directives --check "$lines" "$@" > "$O/$name.log" 2>&1
  report "$name" $?
}
# getters NAME LINES ARGS...: `ksdump --lang csharp --getters --check LINES ARGS...`.
getters() {
  name=$1 lines=$2; shift 2
  $B --lang csharp --getters --check "$lines" "$@" > "$O/$name.log" 2>&1
  report "$name" $?
}
# bodies NAME ARGS...: `ksdump --check-bodies ARGS...`, nonzero on a differing body or a tree whose leaves are not the
# text.
bodies() {
  name=$1; shift
  $B --check-bodies "$@" > "$O/$name.log" 2>&1
  report "$name" $?
}

kotlin() {
  f=0
  dumps community "$C/oracle/community-3caa0bf" --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" || f=1
  for s in 11 12; do
    dumps parse-fuzz-$s "$C/fuzz/parse-$s.oracle" --list "$C/fuzz/parse-$s/list" --root "$C/fuzz/parse-$s" || f=1
  done
  # psi-tools 4d21cba mutate.js over crates/kotlin-syntax/testdata/psi
  dumps mutations "$C/fuzz/mut330.oracle" --list "$C/fuzz/mut330.list" --root "$C/fuzz/mut330" || f=1
  for s in 1 2; do
    lex lex-fuzz-$s "$C/fuzz/lex-$s.oracle" --list "$C/fuzz/lex-$s/list" --root "$C/fuzz/lex-$s" || f=1
  done
  bodies check-bodies --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" || f=1
  bodies nesting-limit-3-community --nesting-limit 3 --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" || f=1
  bodies nesting-limit-3-mut330 --nesting-limit 3 --list "$C/fuzz/mut330.list" --root "$C/fuzz/mut330" || f=1
  return $f
}

java() {
  f=0
  # JavaLexer and JavaDocLexer against tools/java-oracle/JavaLexOracle.java's cached lines (the community .java files,
  # the one-place mutations of the Java goldens, the lexer fuzz of `java-oracle lex-fuzz`, tools/java-oracle/README.md).
  lex java-lex-java-community-3caa0bf "$C/lex/java-community-3caa0bf.oracle.txt" \
    --list "$C/corpus/java-community-3caa0bf.list" --root "$ROOT" || f=1
  lex java-lex-java-mut "$C/lex/java-mut.oracle.txt" --list "$C/fuzz/java-mut.list" --root "$C/fuzz/java-mut" || f=1
  lex java-lex-java-lex "$C/lex/java-lex.oracle.txt" --list "$C/fuzz/java-lex/list" --root "$C/fuzz/java-lex" || f=1
  # Both dumps (`--node`) of the same corpora against JavaPsiOracle.java's cached ones (the 3 community files that are
  # not UTF-8 are failures on both sides: no dump).
  dumps java-psi-java-community-3caa0bf "$C/oracle/java-community-3caa0bf" --node \
    --list "$C/corpus/java-community-3caa0bf.list" --root "$ROOT" || f=1
  dumps java-psi-java-mut "$C/fuzz/java-mut.oracle" --node --list "$C/fuzz/java-mut.list" --root "$C/fuzz/java-mut" || f=1
  dumps java-psi-java-lex "$C/fuzz/java-lex.oracle" --node --list "$C/fuzz/java-lex/list" --root "$C/fuzz/java-lex" || f=1
  # The getters of java_syntax::psi against JavaPsiOracle.java --getters's cached lines (tools/java-oracle/README.md).
  dumps java-getters-goldens "$C/getters/2/java-goldens" --getters --list testdata/java/manifest.tsv \
    --root testdata/java || f=1
  dumps java-getters-java-community-3caa0bf "$C/getters/2/java-community-3caa0bf" --getters \
    --list "$C/corpus/java-community-3caa0bf.list" --root "$ROOT" || f=1
  dumps java-getters-java-mut "$C/getters/2/java-mut" --getters --list "$C/fuzz/java-mut.list" --root "$C/fuzz/java-mut" || f=1
  dumps java-getters-java-lex "$C/getters/2/java-lex" --getters --list "$C/fuzz/java-lex/list" --root "$C/fuzz/java-lex" || f=1
  bodies java-check-bodies --list "$C/corpus/java-community-3caa0bf.list" --root "$ROOT" || f=1
  bodies java-nesting-limit-3-community --nesting-limit 3 --list "$C/corpus/java-community-3caa0bf.list" --root "$ROOT" || f=1
  bodies java-nesting-limit-3-java-mut --nesting-limit 3 --list "$C/fuzz/java-mut.list" --root "$C/fuzz/java-mut" || f=1
  return $f
}

csharp_lex() {
  f=0
  # The lexer against tools/csharp-oracle's cached `lex --doc none|parse|diagnose` lines (the four corpora, the golden
  # mutations, the lexer fuzz; the mutations' lines were made from the list's paths alone, the defaults for every file).
  cut -f1 "$C/fuzz/csharp-mut.list" > "$O/csharp-mut.paths"
  for corpus in "$C/corpus/csharp-roslyn-35d9211.list $C/csharp-src/roslyn csharp-roslyn-35d9211" \
    "$C/corpus/csharp-runtime-6aa72da.list $C/csharp-src/runtime csharp-runtime-6aa72da" \
    "$C/corpus/csharp-aspnetcore-0ef4bbf.list $C/csharp-src/aspnetcore csharp-aspnetcore-0ef4bbf" \
    "$C/corpus/csharp-avalonia-2d79ba5.list $C/csharp-src/avalonia csharp-avalonia-2d79ba5" \
    "$O/csharp-mut.paths $C/fuzz/csharp-mut csharp-mut" "$C/fuzz/csharp-lex/list $C/fuzz/csharp-lex csharp-lex"; do
    set -- $corpus
    lex csharp-lex-$3 "$C/lex/$3.oracle.txt" --lang csharp --doc none --list "$1" --root "$2" || f=1
    lex csharp-lex-doc-parse-$3 "$C/lex/$3.doc.oracle.txt" --lang csharp --doc parse --list "$1" --root "$2" || f=1
    lex csharp-lex-doc-diagnose-$3 "$C/lex/$3.diag.oracle.txt" --lang csharp --doc diagnose --list "$1" --root "$2" || f=1
  done
  # The value texts against the cached `values --kept` lines (made with the list's columns, the mutations' included).
  for corpus in "$C/corpus/csharp-roslyn-35d9211.list $C/csharp-src/roslyn csharp-roslyn-35d9211" \
    "$C/corpus/csharp-runtime-6aa72da.list $C/csharp-src/runtime csharp-runtime-6aa72da" \
    "$C/corpus/csharp-aspnetcore-0ef4bbf.list $C/csharp-src/aspnetcore csharp-aspnetcore-0ef4bbf" \
    "$C/corpus/csharp-avalonia-2d79ba5.list $C/csharp-src/avalonia csharp-avalonia-2d79ba5" \
    "$C/fuzz/csharp-mut.list $C/fuzz/csharp-mut csharp-mut" "$C/fuzz/csharp-lex/list $C/fuzz/csharp-lex csharp-lex"; do
    set -- $corpus
    values csharp-values-$3 "$C/values/$3.kept.txt" --list "$1" --root "$2" || f=1
  done
  return $f
}

# csharp_parse VERSION: the parser's dumps against tools/csharp-oracle's cached `dump` trees at VERSION (latest,
# preview): the four corpora and the lexer fuzz.
csharp_parse() {
  f=0 v=$1
  sfx=$([ $v = preview ] && echo -preview)
  for corpus in "csharp-roslyn-35d9211 roslyn" "csharp-runtime-6aa72da runtime" "csharp-aspnetcore-0ef4bbf aspnetcore" \
    "csharp-avalonia-2d79ba5 avalonia"; do
    set -- $corpus
    dumps csharp-parse-$1$sfx "$C/oracle/$1$sfx" --lang csharp --version $v --list "$C/corpus/$1.list" \
      --root "$C/csharp-src/$2" || f=1
  done
  dumps csharp-parse-csharp-lex$sfx "$C/fuzz/csharp-lex$sfx.oracle" --lang csharp --version $v \
    --list "$C/fuzz/csharp-lex/list" --root "$C/fuzz/csharp-lex" || f=1
  return $f
}

csharp_latest() {
  f=0
  csharp_parse latest || f=1
  # The golden mutations at their goldens' versions (the list's column), at Latest and at Preview (the same mutants).
  for m in csharp-mut csharp-mut-latest csharp-mut-preview; do
    dumps csharp-parse-$m "$C/fuzz/$m.oracle" --lang csharp --list "$C/fuzz/$m.list" --root "$C/fuzz/csharp-mut" || f=1
  done
  return $f
}

csharp_preview() {
  f=0
  csharp_parse preview || f=1
  # The runtime files the oracle has no dump for (over 512 MB) are only lossless.
  grep '"failure":"dump over 512 MB"' "$C/oracle/csharp-runtime-6aa72da/index.jsonl" | sed 's/^{"path":"\([^"]*\)".*/\1/' \
    > "$O/csharp-huge.list"
  bodies csharp-lossless-without-a-dump --lang csharp --list "$O/csharp-huge.list" --root "$C/csharp-src/runtime" || f=1
  [ "$(tail -1 "$O/csharp-lossless-without-a-dump.log")" = "differing 0  files 6" ] || f=1
  return $f
}

# csharp_defines: step 11, the dumps and lexer lines with every symbol of a file's conditions defined.
csharp_defines() {
  f=0
  for corpus in "csharp-roslyn-35d9211 roslyn" "csharp-avalonia-2d79ba5 avalonia"; do
    set -- $corpus
    dumps csharp-parse-$1-defines "$C/oracle/$1-defines" --lang csharp --list "$C/corpus/$1-defines.list" \
      --root "$C/csharp-src/$2" || f=1
    lex csharp-lex-doc-parse-$1-defines "$C/lex/$1-defines.doc.oracle.txt" --lang csharp --doc parse \
      --list "$C/corpus/$1-defines.list" --root "$C/csharp-src/$2" || f=1
  done
  return $f
}

# csharp_directives: step 12, each directive's boolean fields against the cached `directives` lines (made with the
# lists' own columns).
csharp_directives() {
  f=0
  for corpus in "$C/corpus/csharp-roslyn-35d9211.list $C/csharp-src/roslyn csharp-roslyn-35d9211" \
    "$C/corpus/csharp-runtime-6aa72da.list $C/csharp-src/runtime csharp-runtime-6aa72da" \
    "$C/corpus/csharp-aspnetcore-0ef4bbf.list $C/csharp-src/aspnetcore csharp-aspnetcore-0ef4bbf" \
    "$C/corpus/csharp-avalonia-2d79ba5.list $C/csharp-src/avalonia csharp-avalonia-2d79ba5" \
    "$C/corpus/csharp-roslyn-35d9211-defines.list $C/csharp-src/roslyn csharp-roslyn-35d9211-defines" \
    "$C/corpus/csharp-avalonia-2d79ba5-defines.list $C/csharp-src/avalonia csharp-avalonia-2d79ba5-defines" \
    "$C/fuzz/csharp-mut.list $C/fuzz/csharp-mut csharp-mut" "$C/fuzz/csharp-lex/list $C/fuzz/csharp-lex csharp-lex"; do
    set -- $corpus
    directives csharp-directives-$3 "$C/directives/$3.txt" --lang csharp --list "$1" --root "$2" || f=1
  done
  return $f
}

# csharp_getters: step 13, the syntax getters against tools/csharp-oracle's cached `getters` lines
# (tools/csharp-oracle/caches.sh getters).
csharp_getters() {
  f=0 G=$C/getters/csharp
  awk -F '\t' 'BEGIN { OFS = "\t" } { print $1 ".cs", $2, $3, $4, $5 }' testdata/csharp/manifest.tsv > "$O/csharp-goldens.list"
  getters csharp-getters-goldens "$G/csharp-goldens.txt" --list "$O/csharp-goldens.list" --root testdata/csharp || f=1
  for corpus in "csharp-roslyn-35d9211 roslyn $C/corpus" "csharp-runtime-6aa72da runtime $G" \
    "csharp-aspnetcore-0ef4bbf aspnetcore $C/corpus" "csharp-avalonia-2d79ba5 avalonia $C/corpus"; do
    set -- $corpus
    getters csharp-getters-$1 "$G/$1.txt" --list "$3/$1.list" --root "$C/csharp-src/$2" || f=1
    getters csharp-getters-$1-preview "$G/$1-preview.txt" --version preview --list "$3/$1.list" \
      --root "$C/csharp-src/$2" || f=1
  done
  for doc in diagnose none; do
    sfx=$([ $doc = none ] && echo nodoc || echo $doc)
    getters csharp-getters-roslyn-$sfx "$G/csharp-roslyn-35d9211-$sfx.txt" --doc $doc \
      --list "$C/corpus/csharp-roslyn-35d9211.list" --root "$C/csharp-src/roslyn" || f=1
  done
  for corpus in "csharp-roslyn-35d9211 roslyn" "csharp-avalonia-2d79ba5 avalonia"; do
    set -- $corpus
    getters csharp-getters-$1-defines "$G/$1-defines.txt" --list "$C/corpus/$1-defines.list" \
      --root "$C/csharp-src/$2" || f=1
  done
  for m in csharp-mut csharp-mut-latest csharp-mut-preview; do
    getters csharp-getters-$m "$G/$m.txt" --list "$C/fuzz/$m.list" --root "$C/fuzz/csharp-mut" || f=1
  done
  getters csharp-getters-csharp-lex "$G/csharp-lex.txt" --list "$C/fuzz/csharp-lex/list" --root "$C/fuzz/csharp-lex" || f=1
  getters csharp-getters-csharp-lex-preview "$G/csharp-lex-preview.txt" --version preview \
    --list "$C/fuzz/csharp-lex/list" --root "$C/fuzz/csharp-lex" || f=1
  getters csharp-getters-fuzz "$G/csharp-getters.txt" --list "$C/fuzz/csharp-getters.list" \
    --root "$C/fuzz/csharp-getters" || f=1
  return $f
}

# Three lanes at once, each ksdump on every core: the work of a lane is mostly parallel, but each C# lane waits some
# 20 s per pass on the runtime's 24 MB hugeexpr1.cs alone, which the other lanes fill. A lane's sections write to
# $O/SECTION.out, printed in this order once all are done.
lane() {
  for s in "$@"; do "$s" > "$O/$s.out" 2>&1 || echo "$s" >> "$O/failed"; done
}
lane kotlin java csharp_lex &
lane csharp_latest &
lane csharp_preview csharp_defines csharp_directives csharp_getters &
wait
cat "$O/kotlin.out" "$O/java.out" "$O/csharp_lex.out" "$O/csharp_latest.out" "$O/csharp_preview.out" \
  "$O/csharp_defines.out" "$O/csharp_directives.out" "$O/csharp_getters.out"
[ -e "$O/failed" ] && fail=1
[ $fail = 0 ] && { rm -rf "$O"; echo PASS; } || { echo "FAIL (logs and our differing dumps: $O)"; exit 1; }
