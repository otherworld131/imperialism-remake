#!/usr/bin/env bash
# Build the flavor crate as a WASM module for the standalone re-roll demo.
#
# Usage:  ./build.sh             # builds pkg/ in this directory
#         ./build.sh --serve     # also starts a local HTTP server on :8787
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack not found. Install with: cargo install wasm-pack" >&2
  exit 1
fi

cd "$CRATE_DIR"
wasm-pack build \
  --target web \
  --out-dir web-demo/pkg \
  --no-typescript \
  -- \
  --features wasm

echo
echo "Built $SCRIPT_DIR/pkg/"
echo "Open:  cd $SCRIPT_DIR && python3 -m http.server 8787"
echo "Then:  http://localhost:8787/"

if [[ "${1:-}" == "--serve" ]]; then
  cd "$SCRIPT_DIR"
  exec python3 -m http.server 8787
fi
