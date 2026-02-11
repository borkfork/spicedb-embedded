#!/usr/bin/env bash
set -e
root=$(git rev-parse --show-toplevel)
cd "$root/shared/c"
rm -f libspicedb.so libspicedb.dylib libspicedb.h spicedb.dll spicedb.h
out=$("$root/scripts/lib-path.sh")
CGO_ENABLED=1 go mod tidy
CGO_ENABLED=1 go build -buildmode=c-shared -o "$out" .
