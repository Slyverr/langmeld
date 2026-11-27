#!/usr/bin/env bash
# Builds the language server for the browser into www/pkg. Run it from anywhere, in the
# development shell. The version of `wasm-bindgen` in Cargo.lock has to be the one of the
# `wasm-bindgen` command.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
cargo build -p calc-web --release --target wasm32-unknown-unknown --locked
wasm-bindgen --target web --out-dir examples/web/www/pkg \
  target/wasm32-unknown-unknown/release/calc_web.wasm
