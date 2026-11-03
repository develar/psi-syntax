#!/bin/sh
# A/B throughput of two ksdump binaries on a shared machine: runs them alternately (A B A B ...) ROUNDS times each
# over the community corpus and prints every run with the 1-minute load average, then the median MB/s of each and
# the ratio B/A. Extra arguments go to `ksdump --bench` (e.g. `--threads 8`, `--walk`).
#   scripts/ab.sh OLD_KSDUMP NEW_KSDUMP [ROUNDS=5] [BENCH ARGS...]
# KS_INSTR=1 (macOS): each run is a `--repeat 1` and a `--repeat 3` run under `/usr/bin/time -l`, and it prints the
# instructions and cycles of one round (their difference halved, so reading the sources does not count) beside the
# median MB/s of the three rounds (with `--threads N`, the median of the rounds' per-thread medians); the bench
# arguments then take no `--repeat`. On a shared machine instructions move by about 1 %, MB/s and cycles by far more.
# A baseline binary of an accio commit: `git archive REV | tar -xm -C DIR && (cd DIR && CARGO_TARGET_DIR=DIR/target
# cargo build --profile psi --bin ksdump)`, the binary DIR/target/psi/ksdump (`-m`: fresh mtimes, so cargo does not
# reuse a stale build).
# Peak RSS of one run: `/usr/bin/time -l ksdump --bench ...` ("maximum resident set size"; the loaded sources count).
set -u
[ $# -ge 2 ] || { sed -n 2,12p "$0"; exit 2; }
A=$1 B=$2; shift 2
ROUNDS=${1:-5}; [ $# -ge 1 ] && shift
LIST=${KS_LIST:-$HOME/.cache/psi-syntax/corpus/community-3caa0bf.list}
ROOT=${KS_ROOT:-/Users/develar/.cache/accio-bench/intellij-community}
INSTR=${KS_INSTR:-0}
load() { sysctl -n vm.loadavg | awk '{print $2}'; }
run() { bin=$1; shift; "$bin" --bench --list "$LIST" --root "$ROOT" "$@" 2>/dev/null | awk '/MB\/s/ && /^(round [0-9]+  )?files/ {for (i=1;i<=NF;i++) if ($i=="MB/s") print $(i+1); exit}'; }
med() { echo "$@" | tr ' ' '\n' | grep . | sort -g | awk '{v[NR]=$1} END {print (NR%2 ? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2)}'; }
counter() { awk -v k="$1" '$0 ~ k {print $1; exit}'; }
# run_instr BIN ARGS...: "MB/s M-instructions M-cycles" of one round
run_instr() {
  bin=$1; shift
  o1=$( { /usr/bin/time -l "$bin" --bench --list "$LIST" --root "$ROOT" "$@" --repeat 1 > /dev/null; } 2>&1 )
  o3=$( { /usr/bin/time -l "$bin" --bench --list "$LIST" --root "$ROOT" "$@" --repeat 3; } 2>&1 )
  i1=$(echo "$o1" | counter 'instructions retired') i3=$(echo "$o3" | counter 'instructions retired')
  c1=$(echo "$o1" | counter 'cycles elapsed') c3=$(echo "$o3" | counter 'cycles elapsed')
  m=$(echo "$o3" | awk '/per-thread parse MB\/s/ {for (i=1;i<=NF;i++) if ($i=="median") print $(i+1)}')
  if [ -n "$m" ]; then m=$(med $m); else m=$(echo "$o3" | awk '/^median MB\/s/ {print $3}'); fi
  echo "$m $(( (i3 - i1) / 2000000 )) $(( (c3 - c1) / 2000000 ))"
}
field() { echo "$1" | cut -d' ' -f"$2"; }
ra="" rb="" ia="" ib="" ca="" cb=""
i=1
while [ $i -le "$ROUNDS" ]; do
  la=$(load)
  if [ "$INSTR" = 1 ]; then
    oa=$(run_instr "$A" "$@"); lb=$(load); ob=$(run_instr "$B" "$@")
    a=$(field "$oa" 1) b=$(field "$ob" 1)
    echo "round $i  A $a MB/s $(field "$oa" 2) M instr $(field "$oa" 3) M cycles (load $la)  B $b MB/s $(field "$ob" 2) M instr $(field "$ob" 3) M cycles (load $lb)"
    ia="$ia $(field "$oa" 2)" ib="$ib $(field "$ob" 2)" ca="$ca $(field "$oa" 3)" cb="$cb $(field "$ob" 3)"
  else
    a=$(run "$A" "$@"); lb=$(load); b=$(run "$B" "$@")
    echo "round $i  A $a MB/s (load $la)  B $b MB/s (load $lb)"
  fi
  ra="$ra $a" rb="$rb $b"
  i=$((i + 1))
done
ma=$(med $ra) mb=$(med $rb)
echo "median A $ma MB/s  B $mb MB/s  B/A $(echo "$ma $mb" | awk '{printf "%.3f", $2/$1}')"
if [ "$INSTR" = 1 ]; then
  xa=$(med $ia) xb=$(med $ib)
  echo "median A $xa M instr $(med $ca) M cycles  B $xb M instr $(med $cb) M cycles  instructions B/A $(echo "$xa $xb" | awk '{printf "%.4f", $2/$1}')"
fi
