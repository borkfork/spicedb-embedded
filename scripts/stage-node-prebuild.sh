#!/usr/bin/env bash
# Stages the built shared library into node/prebuilds/<platform>-<arch>/ for local testing.
# Node expects prebuilds/darwin-arm64/, linux-x64/, win32-x64/ (no "native" subdir).
# Run from repo root after shared-c-build. Then: cd node && npm run build && npm test
# Usage: ./scripts/stage-node-prebuild.sh

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

mkdir -p "$root/node/prebuilds/$key"
cp "$lib_src" "$root/node/prebuilds/$key/"
echo "Staged $libname -> node/prebuilds/$key/"
