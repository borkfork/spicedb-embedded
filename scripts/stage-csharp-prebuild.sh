#!/usr/bin/env bash
# Stages the built shared library into csharp/runtimes/<rid>/native/ for local testing.
# Run from repo root after shared-c-build.
# Usage: ./scripts/stage-csharp-prebuild.sh

set -e
root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$root"

case "$(uname -s)" in
Darwin)
	arch=$(uname -m)
	rid=$([ "$arch" = "x86_64" ] && echo "osx-x64" || echo "osx-arm64")
	libname="libspicedb.dylib"
	;;
Linux)
	arch=$(uname -m)
	rid=$([ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ] && echo "linux-arm64" || echo "linux-x64")
	libname="libspicedb.so"
	;;
MINGW* | MSYS*)
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

mkdir -p "$root/csharp/runtimes/$rid/native"
cp "$lib_src" "$root/csharp/runtimes/$rid/native/"
echo "Staged $libname -> csharp/runtimes/$rid/native/"
