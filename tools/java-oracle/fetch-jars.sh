#!/bin/sh
# Copies the jars the Java PSI oracle runs on from an IntelliJ installation into the cache and checks
# them against jars.sha256 (IntelliJ 2027.1, build 271.SNAPSHOT, built from the IntelliJ monorepo at
# 3d034e2526507767a19528888066e47ad2898bb1). A mismatch means the installation changed: the dumps
# would come from another parser, so the script refuses.
#
#   tools/java-oracle/fetch-jars.sh [IDEA_CONTENTS] [LIB_DIR]
#
# Defaults: /Applications/Idea.app/Contents and $HOME/.cache/psi-syntax/java-oracle/lib.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
contents=${1:-/Applications/Idea.app/Contents}
lib=${2:-$HOME/.cache/psi-syntax/java-oracle/lib}
mkdir -p "$lib"
while read -r sum path; do
  name=$(basename "$path")
  if [ ! -f "$lib/$name" ]; then
    cp -p "$contents/$path" "$lib/$name"
  fi
  actual=$(shasum -a 256 "$lib/$name" | cut -d' ' -f1)
  if [ "$actual" != "$sum" ]; then
    echo "fetch-jars: $name: sha256 $actual, expected $sum" >&2
    exit 1
  fi
done < "$here/jars.sha256"
echo "$lib"
