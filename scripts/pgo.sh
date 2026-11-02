#!/bin/sh
# A profile-guided ksdump: an instrumented build parses the training files (Kotlin and Java, a `--bench` round of
# each, the dump code never runs), llvm-profdata merges the profile, and a second build uses it. The profile changes
# only inlining, code layout and branch weights, never the output; the gate passes on the result.
#   scripts/pgo.sh OUT_DIR [KOTLIN_LIST KOTLIN_ROOT JAVA_LIST JAVA_ROOT]
# Prints the path of OUT_DIR/use/release/ksdump and leaves OUT_DIR/merged.profdata for a consumer's own build
# (`RUSTFLAGS=-Cprofile-use=.../merged.profdata`, the same rustc; a mismatched profile only warns and loses speed).
# Training defaults to the vendored goldens (crates/kotlin-syntax/testdata, testdata/java), which are not the benchmark
# corpus. llvm-profdata must be of rustc's LLVM major version (`rustc -vV`): rustup's llvm-tools component, else
# `llvm-profdata` of Homebrew's llvm@N for that N, else one on PATH.
set -eu
[ $# = 1 ] || [ $# = 5 ] || { sed -n 2,9p "$0"; exit 2; }
cd "$(dirname "$0")/.."
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
major=$(rustc -vV | sed -n 's/^LLVM version: \([0-9]*\).*/\1/p')
host=$(rustc -vV | sed -n 's/^host: //p')
P="$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata"
[ -x "$P" ] || P=$(brew --prefix "llvm@$major" 2>/dev/null || true)/bin/llvm-profdata
[ -x "$P" ] || P=$(command -v llvm-profdata || true)
[ -n "$P" ] && [ -x "$P" ] || { echo "no llvm-profdata: rustup component add llvm-tools, or brew install llvm@$major" >&2; exit 1; }
pmajor=$("$P" --version | sed -n 's/.*LLVM version \([0-9]*\).*/\1/p' | head -1)
[ "$pmajor" = "$major" ] || { echo "$P is LLVM $pmajor, rustc is LLVM $major" >&2; exit 1; }
if [ $# = 5 ]; then
  KL=$2 KR=$3 JL=$4 JR=$5
else
  KL=$OUT/kotlin.list KR=$PWD/crates/kotlin-syntax/testdata JL=$OUT/java.list JR=$PWD/testdata/java
  (cd "$KR" && find . -name '*.kt' -o -name '*.kts' | sed 's|^\./||' | sort) > "$KL"
  (cd "$JR" && find . -name '*.java' | sed 's|^\./||' | sort) > "$JL"
fi
rm -rf "$OUT/data" "$OUT/merged.profdata"
RUSTFLAGS="-Cprofile-generate=$OUT/data" CARGO_TARGET_DIR=$OUT/gen cargo build --release --locked --bin ksdump -q
# Several rounds, so a small training set still weighs the parse over reading the files.
"$OUT/gen/release/ksdump" --bench --list "$KL" --root "$KR" --repeat 5 > /dev/null
"$OUT/gen/release/ksdump" --bench --list "$JL" --root "$JR" --repeat 5 > /dev/null
"$P" merge -o "$OUT/merged.profdata" "$OUT/data"
RUSTFLAGS="-Cprofile-use=$OUT/merged.profdata" CARGO_TARGET_DIR=$OUT/use cargo build --release --locked --bin ksdump -q
echo "$OUT/use/release/ksdump"
