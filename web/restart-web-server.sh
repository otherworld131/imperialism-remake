#!/bin/bash
# Kill existing dev server, rebuild WASM, and restart the web dev server.
# Usage: ./web/restart-web-server.sh
set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo ">> Killing dev server..."
lsof -ti :5173 | xargs kill 2>/dev/null || true

echo ">> Building WASM..."
cd "$REPO_ROOT/crates/wasm-bridge"
wasm-pack build --target web

echo ">> Starting dev server..."
cd "$REPO_ROOT/web"
npm run dev &

echo ">> Done. Dev server starting on http://localhost:5173"
