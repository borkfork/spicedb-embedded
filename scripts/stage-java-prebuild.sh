#!/usr/bin/env bash
# Stages the built shared library into java/src/main/resources/natives/<java_key>/ for local testing.
# Run from repo root after shared-c-build.
# Usage: ./scripts/stage-java-prebuild.sh

set -e
root=$(git rev-parse --show-toplevel)
cd "$root"

case "$(uname -s)" in
Darwin)
	arch=$(uname -m)
	java_key=$([ "$arch" = "x86_64" ] && echo "osx-x86_64" || echo "osx-aarch_64")
	libname="libspicedb.dylib"
	;;
Linux)
	arch=$(uname -m)
	java_key=$([ "$arch" = "aarch64" ] || [ "$arch" = "arm64" ] && echo "linux-aarch_64" || echo "linux-x86_64")
	libname="libspicedb.so"
	;;
MINGW* | MSYS*)
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
	echo "Run shared-c-build first. Not found: $lib_src" >&2
	exit 1
fi

mkdir -p "$root/java/src/main/resources/natives/$java_key"
cp "$lib_src" "$root/java/src/main/resources/natives/$java_key/"
echo "Staged $libname -> java/src/main/resources/natives/$java_key/"
