#!/usr/bin/env bash
# Stages the built shared library into rust/spicedb-embedded-sys/prebuilds/<rid>/ for local testing.
# Run from repo root after shared-c-build.
# Usage: ./scripts/stage-rust-prebuild.sh

set -e
root=$(git rev-parse --show-toplevel)
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

mkdir -p "$root/rust/spicedb-embedded-sys/prebuilds/$rid"
cp "$lib_src" "$root/rust/spicedb-embedded-sys/prebuilds/$rid/"
if [ -f "$root/shared/c/spicedb.def" ]; then
	cp "$root/shared/c/spicedb.def" "$root/rust/spicedb-embedded-sys/prebuilds/$rid/"
fi
echo "Staged $libname -> rust/spicedb-embedded-sys/prebuilds/$rid/"
