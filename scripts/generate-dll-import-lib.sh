#!/usr/bin/env bash
# Generate spicedb.lib from spicedb.dll for MSVC linking on Windows.
# Go's c-shared builds produce .dll but not .lib; MSVC linker needs the import library.
set -e
cd "$(git rev-parse --show-toplevel)/shared/c"
[ -f spicedb.dll ] || { echo "spicedb.dll not found; run shared-c-build first" >&2; exit 1; }
[ -f spicedb.def ] || { echo "spicedb.def not found" >&2; exit 1; }
# Find lib.exe via vswhere (Visual Studio)
vswhere_exe="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
if [ -x "$vswhere_exe" ]; then
  vs_path=$("$vswhere_exe" -latest -property installationPath 2>/dev/null || true)
fi
if [ -z "$vs_path" ]; then
  vs_path="/c/Program Files/Microsoft Visual Studio/2022/Enterprise"
fi
lib_exe=$(find "$vs_path/VC/Tools/MSVC" -name "lib.exe" -path "*/Hostx64/x64/*" 2>/dev/null | head -1)
if [ -z "$lib_exe" ] || [ ! -x "$lib_exe" ]; then
  echo "lib.exe not found; install Visual Studio Build Tools" >&2
  exit 1
fi
"$lib_exe" /def:spicedb.def /out:spicedb.lib /machine:x64
echo "Generated spicedb.lib"
