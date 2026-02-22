#!/usr/bin/env bash
# Stages the built shared library into python/src/spicedb_embedded/natives/<key>/ for local testing.
# Run from repo root after shared-c-build.
# Usage: ./scripts/stage-python-prebuild.sh

set -e
root=$(git rev-parse --show-toplevel)
cd "$root"

case "$(uname -s)" in
Darwin)
	arch=$(uname -m)
	key=$([ "$arch" = "x86_64" ] && echo "darwin-x64" || echo "darwin-arm64")
	libname="libspicedb.dylib"
	;;
Linux)
	arch=$(uname -m)
	key=$([ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ] && echo "linux-arm64" || echo "linux-x64")
	libname="libspicedb.so"
	;;
MINGW* | MSYS*)
	key="win32-x64"
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

mkdir -p "$root/python/src/spicedb_embedded/natives/$key"
cp "$lib_src" "$root/python/src/spicedb_embedded/natives/$key/"
echo "Staged $libname -> python/src/spicedb_embedded/natives/$key/"
