#!/bin/sh
# The correctness gate every change must pass, byte for byte against Kotlin 2.4.21's PSI:
#   1. clippy and KS_STRICT=1 cargo test --release (goldens, lexer, kdoc, robustness);
#   2. the 55 501 community files: `diff -rq` of our dumps against the cached oracle dumps, 0 lines;
#   3. cached fuzz corpora (parse seeds 11-12, lex seeds 1-2, 3000 files each) against their cached oracle answers.
# Prints PASS or the first failures and exits nonzero. The caches are made once (see the fuzz scripts' docstrings).
set -u
cd "$(dirname "$0")/.."
C=$HOME/.cache/kotlin-syntax
ROOT=/Users/develar/.cache/accio-bench/intellij-community
fail=0
cargo clippy --release --all-targets -q -- -D warnings || fail=1
KS_STRICT=1 cargo test --release -q 2>&1 | grep -E "^test result|goldens:|FAILED|panicked" || true
KS_STRICT=1 cargo test --release -q > /dev/null 2>&1 || { echo "FAIL tests"; fail=1; }
cargo build --release -q || exit 1
B=target/release/ksdump
rm -rf "$C/ours/gate"
$B --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" --out "$C/ours/gate" > /dev/null
n=$(diff -rq -x index.jsonl "$C/oracle/community-3caa0bf" "$C/ours/gate" | wc -l | tr -d ' ')
echo "community: $n differing of 55501"; [ "$n" = 0 ] || { diff -rq -x index.jsonl "$C/oracle/community-3caa0bf" "$C/ours/gate" | head -5; fail=1; }
for s in 11 12; do
  rm -rf "$C/ours/parse-$s"
  $B --list "$C/fuzz/parse-$s/list" --root "$C/fuzz/parse-$s" --out "$C/ours/parse-$s" > /dev/null
  n=$(diff -rq -x index.jsonl "$C/fuzz/parse-$s.oracle" "$C/ours/parse-$s" | wc -l | tr -d ' ')
  echo "parse-fuzz $s: $n differing of 3000"; [ "$n" = 0 ] || fail=1
done
for s in 1 2; do
  n=$($B --lex --list "$C/fuzz/lex-$s/list" --root "$C/fuzz/lex-$s" | cut -f1,2 | diff - "$C/fuzz/lex-$s.oracle" | grep -c '^[<>]')
  echo "lex-fuzz $s: $n differing lines"; [ "$n" = 0 ] || fail=1
done
[ $fail = 0 ] && echo PASS || { echo FAIL; exit 1; }
