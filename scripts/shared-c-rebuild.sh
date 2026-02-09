#!/usr/bin/env bash
set -e
cd "$(git rev-parse --show-toplevel)/shared/c"
rm -f libspicedb.so libspicedb.dylib libspicedb.h spicedb.dll spicedb.h
out=$(../../scripts/lib-path.sh)
CGO_ENABLED=1 go mod tidy
CGO_ENABLED=1 go build -buildmode=c-shared -o "$out" .
