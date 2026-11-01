#!/bin/sh
# A/B throughput of two ksdump binaries on a shared machine: runs them alternately (A B A B ...) ROUNDS times each
# over the community corpus and prints every run with the 1-minute load average, then the median MB/s of each and
# the ratio B/A. Extra arguments go to `ksdump --bench` (e.g. `--threads 8`, `--walk`).
#   scripts/ab.sh OLD_KSDUMP NEW_KSDUMP [ROUNDS=5] [BENCH ARGS...]
# A baseline binary of a commit: `git archive REV | tar -xm -C DIR && (cd DIR && CARGO_TARGET_DIR=DIR/target cargo
# build --release --bin ksdump)` (`-m`: fresh mtimes, so cargo does not reuse a stale build).
# Peak RSS of one run: `/usr/bin/time -l ksdump --bench ...` ("maximum resident set size"; the loaded sources count).
set -u
[ $# -ge 2 ] || { sed -n 2,8p "$0"; exit 2; }
A=$1 B=$2; shift 2
ROUNDS=${1:-5}; [ $# -ge 1 ] && shift
LIST=${KS_LIST:-$HOME/.cache/kotlin-syntax/corpus/community-3caa0bf.list}
ROOT=${KS_ROOT:-/Users/develar/.cache/accio-bench/intellij-community}
load() { sysctl -n vm.loadavg | awk '{print $2}'; }
run() { bin=$1; shift; "$bin" --bench --list "$LIST" --root "$ROOT" "$@" 2>/dev/null | awk '/MB\/s/ && /^(round [0-9]+  )?files/ {for (i=1;i<=NF;i++) if ($i=="MB/s") print $(i+1); exit}'; }
ra="" rb=""
i=1
while [ $i -le "$ROUNDS" ]; do
  la=$(load); a=$(run "$A" "$@"); lb=$(load); b=$(run "$B" "$@")
  echo "round $i  A $a MB/s (load $la)  B $b MB/s (load $lb)"
  ra="$ra $a" rb="$rb $b"
  i=$((i + 1))
done
med() { echo "$@" | tr ' ' '\n' | grep . | sort -g | awk '{v[NR]=$1} END {print (NR%2 ? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2)}'; }
ma=$(med $ra) mb=$(med $rb)
echo "median A $ma MB/s  B $mb MB/s  B/A $(echo "$ma $mb" | awk '{printf "%.3f", $2/$1}')"
