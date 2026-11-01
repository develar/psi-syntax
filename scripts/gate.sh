#!/bin/sh
# The correctness gate every change must pass, byte for byte against Kotlin 2.4.21's PSI:
#   1. clippy (default and with the opt-in `counters,alloc-stats` features) and KS_STRICT=1 cargo test --release
#      (goldens, lexer, kdoc, robustness);
#   2. the 55 501 community files: `diff -rq` of our dumps against the cached oracle dumps, 0 lines;
#   3. cached fuzz corpora (parse seeds 11-12, lex seeds 1-2, 3000 files each) against their cached oracle answers;
#   4. body-less parsing on the community files: `parse_body` of every unexpanded body dumps as the full parse's subtree.
# Prints PASS or the first failures and exits nonzero. The caches are made once (see the fuzz scripts' docstrings).
# Our dumps go to a fresh directory per run under ~/.cache/kotlin-syntax/ours/ (concurrent sessions share the machine),
# removed on PASS and kept, with its path printed, on FAIL.
set -u
cd "$(dirname "$0")/.."
C=$HOME/.cache/kotlin-syntax
ROOT=/Users/develar/.cache/accio-bench/intellij-community
fail=0
cargo clippy --release --all-targets -q -- -D warnings || fail=1
cargo clippy --release --all-targets --features counters,alloc-stats -q -- -D warnings || fail=1
KS_STRICT=1 cargo test --release -q 2>&1 | grep -E "^test result|goldens:|FAILED|panicked" || true
KS_STRICT=1 cargo test --release -q > /dev/null 2>&1 || { echo "FAIL tests"; fail=1; }
cargo build --release -q || exit 1
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
for s in 1 2; do
  n=$($B --lex --list "$C/fuzz/lex-$s/list" --root "$C/fuzz/lex-$s" | cut -f1,2 | diff - "$C/fuzz/lex-$s.oracle" | grep -c '^[<>]')
  echo "lex-fuzz $s: $n differing lines"; [ "$n" = 0 ] || fail=1
done
bodies=$($B --check-bodies --list "$C/corpus/community-3caa0bf.list" --root "$ROOT" | tail -1)
echo "check-bodies: $bodies"; case "$bodies" in *"differing 0 "*) ;; *) fail=1 ;; esac
[ $fail = 0 ] && { rm -rf "$O"; echo PASS; } || { echo "FAIL (our dumps: $O)"; exit 1; }
