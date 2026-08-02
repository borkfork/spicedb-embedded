#!/usr/bin/env bash
# Stage a canonical CI artifact into one language's native-library layout.
# Usage: ./scripts/stage-downloaded-prebuild.sh <language> <artifact-root> <rid>

set -euo pipefail

root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
language="${1:?usage: stage-downloaded-prebuild.sh <language> <artifact-root> <rid>}"
artifact_root="${2:?usage: stage-downloaded-prebuild.sh <language> <artifact-root> <rid>}"
rid="${3:?usage: stage-downloaded-prebuild.sh <language> <artifact-root> <rid>}"
source_dir="$artifact_root/$rid/native"

case "$rid" in
linux-x64)
	libname="libspicedb.so"
	node_key="linux-x64"
	python_key="linux-x64"
	java_key="linux-x86_64"
	;;
linux-arm64)
	libname="libspicedb.so"
	node_key="linux-arm64"
	python_key="linux-arm64"
	java_key="linux-aarch_64"
	;;
osx-arm64)
	libname="libspicedb.dylib"
	node_key="darwin-arm64"
	python_key="darwin-arm64"
	java_key="osx-aarch_64"
	;;
win-x64)
	libname="spicedb.dll"
	node_key="win32-x64"
	python_key="win32-x64"
	java_key="windows-x86_64"
	;;
*)
	echo "Unsupported RID: $rid" >&2
	exit 1
	;;
esac

source_lib="$source_dir/$libname"
if [ ! -f "$source_lib" ]; then
	echo "Native library not found: $source_lib" >&2
	exit 1
fi

case "$language" in
rust)
	destination="$root/rust/spicedb-embedded-sys/prebuilds/$rid"
	;;
node)
	destination="$root/node/prebuilds/$node_key"
	;;
java)
	destination="$root/java/src/main/resources/natives/$java_key"
	;;
python)
	destination="$root/python/src/spicedb_embedded/natives/$python_key"
	;;
csharp)
	destination="$root/csharp/runtimes/$rid/native"
	;;
*)
	echo "Unsupported language: $language" >&2
	exit 1
	;;
esac

mkdir -p "$destination"
cp "$source_lib" "$destination/"
if [ "$language" = "rust" ] && [ -f "$source_dir/spicedb.def" ]; then
	cp "$source_dir/spicedb.def" "$destination/"
fi

echo "Staged $source_lib -> ${destination#"$root/"}/"
