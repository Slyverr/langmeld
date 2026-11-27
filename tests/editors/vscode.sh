#!/usr/bin/env bash
# Writes the Visual Studio Code extensions for Calc and for `.langmeld` files with
# `langmeld package vscode`, compiles them, and runs each in the editor against a file with
# a mistake. It needs a display (`xvfb-run -a` makes one) and the programs built:
#   cargo build -p langmeld-cli -p calc --locked
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/editors"
# The extensions start `calc lsp` and `langmeld lsp` from the path.
export PATH="$root/target/debug:$PATH"

cd "$root/tests/editors"
npm ci --no-audit --no-fund
npm run compile

rm -rf "$out"
langmeld package vscode "$root/examples/calc" --out "$out/calc"
langmeld package vscode --langmeld --out "$out/grammar"
for name in calc grammar; do
  # The packages an extension needs are those pinned for the tests.
  ln -s "$root/tests/editors/node_modules" "$out/$name/node_modules"
  (cd "$out/$name" && "$root/tests/editors/node_modules/.bin/tsc" -p .)
done

SUITE=calc EXTENSION="$out/calc" WORKSPACE="$root/tests/editors/fixtures/calc" \
  node out/runTest.js
SUITE=grammar EXTENSION="$out/grammar" WORKSPACE="$root/tests/editors/fixtures/grammar" \
  node out/runTest.js
