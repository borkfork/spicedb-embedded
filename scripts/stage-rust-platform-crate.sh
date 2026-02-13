#!/usr/bin/env bash
# Copies the shared library from rust/prebuilds/<rid>/ into the matching
# platform crate's prebuilds/<rid>/ so that crate can be published with the binary.
# Run from repo root after stage-all-prebuilds (or after downloading prebuilds in CI).
# Usage: ./scripts/stage-rust-platform-crate.sh

set -e
root=$(git rev-parse --show-toplevel)
cd "$root"

case "$(uname -s)" in
Darwin)
	rid=$([ "$(uname -m)" = "x86_64" ] && echo "osx-x64" || echo "osx-arm64")
	if [ "$rid" = "osx-x64" ]; then
		echo "osx-x64 platform crate is not shipped (no CI runner). Use Apple Silicon or build from source." >&2
		exit 1
	fi
	crate="spicedb-embedded-sys-osx-arm64"
	;;
Linux)
	rid=$([ "$(uname -m)" = "aarch64" ] || [ "$(uname -m)" = "arm64" ] && echo "linux-arm64" || echo "linux-x64")
	crate="spicedb-embedded-sys-linux-x64"
	[ "$rid" = "linux-arm64" ] && crate="spicedb-embedded-sys-linux-arm64"
	;;
MINGW* | MSYS*)
	rid="win-x64"
	crate="spicedb-embedded-sys-win-x64"
	;;
*)
	echo "Unsupported OS: $(uname -s)" >&2
	exit 1
	;;
esac

src="$root/rust/prebuilds/$rid"
dest="$root/rust/$crate/prebuilds/$rid"
if [ ! -d "$src" ]; then
	echo "Run stage-all-prebuilds first. Not found: $src" >&2
	exit 1
fi
mkdir -p "$dest"
cp -r "$src"/* "$dest/"
echo "Staged -> rust/$crate/prebuilds/$rid/"
echo "Crate $crate is ready to package (e.g. cargo publish -p $crate)."
