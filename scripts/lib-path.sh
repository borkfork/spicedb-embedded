#!/usr/bin/env bash
# Outputs the full path to the dynamic library for the current OS.
# Assumes the script is run from within the git repository.
# Usage: ./scripts/lib-path.sh

root=$(git rev-parse --show-toplevel)
os=$(uname -s)
if [ "$os" = "Darwin" ]; then
  echo "${root}/shared/c/libspicedb.dylib"
elif [ "${os#MINGW}" != "$os" ] || [ "${os#MSYS}" != "$os" ]; then
  echo "${root}/shared/c/spicedb.dll"
else
  echo "${root}/shared/c/libspicedb.so"
fi
