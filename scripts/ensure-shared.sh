#!/usr/bin/env bash
# Ensures top-level shared/ exists and points at rust/shared (symlink) or is a copy.
# Used so scripts and CI that expect shared/c can run; shared/ is .gitignored and mise-managed.
set -e
root=$(git rev-parse --show-toplevel)
cd "$root"
if [ -L shared ] && [ "$(readlink shared)" = "rust/shared" ]; then
  exit 0
fi
if [ -e shared ]; then
  rm -rf shared
fi
ln -s rust/shared shared
