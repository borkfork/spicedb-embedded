#!/usr/bin/env bash
# Stages the built shared library into every language's release layout for the current platform.
# Delegates to per-language scripts. Run from repo root after shared-c-build.
# Usage: ./scripts/stage-all-prebuilds.sh

set -e
root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

"$root/scripts/stage-node-prebuild.sh"
"$root/scripts/stage-csharp-prebuild.sh"
"$root/scripts/stage-java-prebuild.sh"
"$root/scripts/stage-python-prebuild.sh"
"$root/scripts/stage-rust-prebuild.sh"
