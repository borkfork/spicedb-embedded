#!/usr/bin/env bash
set -e
cd "$(git rev-parse --show-toplevel)/shared/c"
out=$(../../scripts/lib-path.sh)
CGO_ENABLED=1 go mod tidy
CGO_ENABLED=1 go build -buildmode=c-shared -o "$out" .
