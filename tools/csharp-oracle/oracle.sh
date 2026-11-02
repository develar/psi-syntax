#!/bin/sh
# Builds the C# oracle when its sources changed and runs it: tools/csharp-oracle/oracle.sh MODE ARGS (README.md).
#
# The SDK is the one global.json pins, from $DOTNET_ROOT (default ~/.cache/psi-syntax/dotnet); packages restore
# in locked mode against packages.lock.json into ~/.cache/psi-syntax/csharp-oracle/nuget. Each distinct source
# tree builds once into ~/.cache/psi-syntax/csharp-oracle/build/<hash of the sources>, so checkouts at different
# revisions never share a build.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cache=$HOME/.cache/psi-syntax
export DOTNET_ROOT="${DOTNET_ROOT:-$cache/dotnet}" DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 \
  DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1 DOTNET_GENERATE_ASPNET_CERTIFICATE=0
dotnet="$DOTNET_ROOT/dotnet"
key=$(cd "$here" && cat global.json nuget.config Directory.Build.props CSharpOracle.csproj packages.lock.json ./*.cs |
  shasum -a 256 | cut -c1-16)
build="$cache/csharp-oracle/build/$key"
dll="$build/bin/Release/net10.0/CSharpOracle.dll"
if [ ! -f "$dll" ]; then
  mkdir -p "$build"
  (cd "$here" && CSHARP_ORACLE_BUILD="$build" "$dotnet" build -c Release --nologo -v quiet >&2)
fi
exec "$dotnet" "$dll" "$@"
