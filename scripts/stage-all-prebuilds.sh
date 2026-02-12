#!/usr/bin/env bash
# Stages the built shared library into every language's release layout for the current platform.
# Use for local build/test and CI so all loaders use the same paths as the release process.
# Run from repo root after shared-c-build (or after downloading the shared lib in CI).
# Usage: ./scripts/stage-all-prebuilds.sh

set -e
root=$(git rev-parse --show-toplevel)
cd "$root"

case "$(uname -s)" in
Darwin)
	arch=$(uname -m)
	key=$([ "$arch" = "x86_64" ] && echo "darwin-x64" || echo "darwin-arm64")
	rid=$([ "$arch" = "x86_64" ] && echo "osx-x64" || echo "osx-arm64")
	java_key=$([ "$arch" = "x86_64" ] && echo "osx-x86_64" || echo "osx-aarch_64")
	libname="libspicedb.dylib"
	;;
Linux)
	arch=$(uname -m)
	key=$([ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ] && echo "linux-arm64" || echo "linux-x64")
	rid=$key
	java_key=$([ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ] && echo "linux-aarch_64" || echo "linux-x86_64")
	libname="libspicedb.so"
	;;
MINGW* | MSYS*)
	key="win32-x64"
	rid="win-x64"
	java_key="windows-x86_64"
	libname="spicedb.dll"
	;;
*)
	echo "Unsupported OS: $(uname -s)" >&2
	exit 1
	;;
esac

lib_src="$root/shared/c/$libname"
if [ ! -f "$lib_src" ]; then
	echo "Run shared-c-build first (or download shared lib). Not found: $lib_src" >&2
	exit 1
fi

# Node: prebuilds/<key>/
mkdir -p "$root/node/prebuilds/$key"
cp "$lib_src" "$root/node/prebuilds/$key/"
echo "Staged -> node/prebuilds/$key/"

# C#: runtimes/<rid>/native/
mkdir -p "$root/csharp/runtimes/$rid/native"
cp "$lib_src" "$root/csharp/runtimes/$rid/native/"
echo "Staged -> csharp/runtimes/$rid/native/"

# Java: src/main/resources/natives/<java_key>/ (matches SpiceDB.platformKey / os-maven-plugin; CI + test classpath)
mkdir -p "$root/java/src/main/resources/natives/$java_key"
cp "$lib_src" "$root/java/src/main/resources/natives/$java_key/"
echo "Staged -> java/src/main/resources/natives/$java_key/"

# Python: src/spicedb_embedded/natives/<key>/
mkdir -p "$root/python/src/spicedb_embedded/natives/$key"
cp "$lib_src" "$root/python/src/spicedb_embedded/natives/$key/"
echo "Staged -> python/src/spicedb_embedded/natives/$key/"

# Rust: prebuilds/<rid>/ (same RID as C#); on Windows include .def for MSVC import lib
mkdir -p "$root/rust/prebuilds/$rid"
cp "$lib_src" "$root/rust/prebuilds/$rid/"
if [ -f "$root/shared/c/spicedb.def" ]; then
	cp "$root/shared/c/spicedb.def" "$root/rust/prebuilds/$rid/"
fi
echo "Staged -> rust/prebuilds/$rid/"
