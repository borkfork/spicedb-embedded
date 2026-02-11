#!/usr/bin/env bash
# Stages the built shared library into runtimes/<rid>/native/ for packaging.
# Run from repo root after shared-c-build. Creates staging dir and copies the lib.
# Usage: ./scripts/stage-shared-lib.sh <staging-dir>
#   e.g. ./scripts/stage-shared-lib.sh staging
# Output: <staging-dir>/<rid>/native/<libname>

set -e
root=$(git rev-parse --show-toplevel)
staging="${1:?usage: stage-shared-lib.sh <staging-dir>}"
cd "$root"

case "$(uname -s)" in
  Darwin)
    rid="osx-arm64"
    libname="libspicedb.dylib"
    # macOS runner can be x64 or arm64
    if [ "$(uname -m)" = "x86_64" ]; then rid="osx-x64"; fi
    ;;
  Linux)
    arch=$(uname -m)
    if [ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ]; then
      rid="linux-arm64"
    else
      rid="linux-x64"
    fi
    libname="libspicedb.so"
    ;;
  MINGW*|MSYS*)
    rid="win-x64"
    libname="spicedb.dll"
    ;;
  *)
    echo "Unsupported OS: $(uname -s)" >&2
    exit 1
    ;;
esac

lib_src="$root/shared/c/$libname"
if [ ! -f "$lib_src" ]; then
  echo "Run shared-c-build first. Not found: $lib_src" >&2
  exit 1
fi

mkdir -p "$staging/$rid/native"
cp "$lib_src" "$staging/$rid/native/$libname"
echo "$rid"
