#!/usr/bin/env bash
# Regenerate Rust code from protos and write to rust/spicedb-grpc-tonic/src/generated/.
# Requires: buf, cargo (and the crate's build-deps). Run from repo root.
# After running, commit src/generated/ so the published crate does not require buf/protoc.

set -euo pipefail
cd "$(dirname "$0")/.."
CRATE=rust/spicedb-grpc-tonic
GEN_DIR=$CRATE/src/generated

# So that build.rs runs buf/tonic, ensure checked-in code is absent.
if [[ -d $GEN_DIR ]]; then
	rm -rf "$GEN_DIR"
fi

# Build; OUT_DIR will contain the generated .rs files.
cargo build -p spicedb-grpc-tonic
# Locate the build script's out dir (most recent that contains .rs files).
OUT=$(find target -type d -path "*/build/spicedb-grpc-tonic-*/out" 2>/dev/null | while read -r d; do
	[[ -n $d && -f "$d/authzed.api.v1.rs" ]] && echo "$d" && break
done)
if [[ -z $OUT || ! -d $OUT ]]; then
	echo "Could not find build output dir with generated .rs (build spicedb-grpc-tonic first)" >&2
	exit 1
fi

mkdir -p "$GEN_DIR"
for f in "$OUT"/*.rs; do
	[[ -e $f ]] && cp "$f" "$GEN_DIR/"
done
echo "Copied $(ls -1 "$GEN_DIR"/*.rs 2>/dev/null | wc -l) .rs files to $GEN_DIR"
echo "Verify with: cargo build -p spicedb-grpc-tonic"
