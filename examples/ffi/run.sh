#!/usr/bin/env bash
#
# M31: one-shot verification of the C-ABI.
#
#   1. cargo build            -> produce libsignal_topology.so / .a
#   2. compile + run test.c   -> gcc against the .so, assert shipped
#   3. run test.py            -> python3 ctypes against the .so, assert shipped
#
# The script exits non-zero if any step fails.

set -euo pipefail

# Resolve the repo root from this script's location (examples/ffi/run.sh).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
cd "${ROOT}"

echo "==> cargo build"
cargo build

echo
echo "==> C demo (test.c)"
 gcc -I include examples/ffi/test.c -L target/debug -lsignal_topology \
     -Wl,-rpath,target/debug -o /tmp/test_ffi_c
/tmp/test_ffi_c

echo
echo "==> Python demo (test.py)"
LD_LIBRARY_PATH=target/debug python3 examples/ffi/test.py

echo
echo "ALL FFI DEMOS PASSED"
