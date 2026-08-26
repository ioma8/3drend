#!/bin/sh
# Build the wasm module into public/pkg (the static web root).
set -e
wasm-pack build --target web --release --out-dir public/pkg
