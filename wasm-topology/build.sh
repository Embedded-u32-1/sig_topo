#!/usr/bin/env bash
# M35: build the wasm-topology crate and emit the wasm-bindgen JS glue into
# `pkg/`, then (by default) symlink the demo in there so it can be served.
#
# Two paths, in order of preference:
#
#   PATH A  -- wasm-pack (recommended):
#     wasm-pack build --target web --out-dir pkg --release
#     Produces pkg/wasm_topology.js + pkg/wasm_topology_bg.wasm with the friendly
#     ES-module API. Bundler-friendly and tiny.
#
#   PATH B  -- cargo + wasm-bindgen-cli (fallback when wasm-pack is absent):
#     cargo build --target wasm32-unknown-unknown --release -p wasm-topology
#     wasm-bindgen --target web --out-dir pkg \
#       target/wasm32-unknown-unknown/release/wasm_topology.wasm
#
# Either way the demo (demo/index.html) loads `./pkg/wasm_topology.js` as an ES
# module, so it works once `pkg/` exists. The page lives in `demo/` and the glue
# in `pkg/`, so serve the REPO ROOT over http (the WebAssembly fetch is blocked
# on file://):
#
#     python3 -m http.server 8080 -d .
#     # open http://localhost:8080/demo/index.html

set -euo pipefail

cd "$(dirname "$0")"
PROFILE="release"

if command -v wasm-pack >/dev/null 2>&1; then
    echo "[build.sh] wasm-pack found -> PATH A"
    wasm-pack build --target web --out-dir pkg --release
else
    echo "[build.sh] wasm-pack not found -> PATH B (cargo + wasm-bindgen-cli)"
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
        echo "[build.sh] ERROR: neither wasm-pack nor wasm-bindgen-cli is on PATH." >&2
        echo "Install one of:" >&2
        echo "    cargo install wasm-pack          # recommended" >&2
        echo "    cargo install wasm-bindgen-cli   # lighter fallback" >&2
        exit 1
    fi
    cargo build --target wasm32-unknown-unknown --release -p wasm-topology
    mkdir -p pkg
    wasm-bindgen --target web --out-dir pkg \
        target/wasm32-unknown-unknown/release/wasm_topology.wasm
fi
echo "[build.sh] done -> pkg/ ; open demo/index.html via a static server (see header)"
