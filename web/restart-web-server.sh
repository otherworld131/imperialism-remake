#!/bin/bash
# Kill existing dev server, rebuild WASM only when needed, and restart Vite.
# Usage: ./web/restart-web-server.sh [--force]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WEB_DIR="$REPO_ROOT/web"
WASM_DIR="$REPO_ROOT/crates/wasm-bridge"
WASM_STAMP="$WASM_DIR/pkg/wasm_bridge_bg.wasm"
NPM_INSTALL_STAMP="$WEB_DIR/node_modules/.install-stamp"
FORCE_REBUILD=0

if [[ "${1:-}" == "--force" ]]; then
  FORCE_REBUILD=1
fi

needs_wasm_rebuild() {
  if [[ $FORCE_REBUILD -eq 1 || ! -f "$WASM_STAMP" ]]; then
    return 0
  fi

  find \
    "$REPO_ROOT/crates/application/src" \
    "$REPO_ROOT/crates/domain/src" \
    "$REPO_ROOT/crates/domain-snapshot/src" \
    "$REPO_ROOT/crates/flavor/src" \
    "$REPO_ROOT/crates/infrastructure/src" \
    "$REPO_ROOT/crates/wasm-bridge/src" \
    "$REPO_ROOT/data" \
    -type f -newer "$WASM_STAMP" -print -quit | grep -q .
}

needs_npm_install() {
  [[ ! -d "$WEB_DIR/node_modules" || ! -f "$NPM_INSTALL_STAMP" ]] && return 0
  [[ "$WEB_DIR/package.json" -nt "$NPM_INSTALL_STAMP" ]] && return 0
  [[ "$WEB_DIR/package-lock.json" -nt "$NPM_INSTALL_STAMP" ]] && return 0
  return 1
}

echo ">> Killing dev server..."
lsof -ti :5173 | xargs kill 2>/dev/null || true

if needs_npm_install; then
  echo ">> Installing web dependencies..."
  (
    cd "$WEB_DIR"
    npm install
    touch "$NPM_INSTALL_STAMP"
  )
fi

if needs_wasm_rebuild || [[ "$REPO_ROOT/Cargo.lock" -nt "$WASM_STAMP" ]] || [[ "$REPO_ROOT/Cargo.toml" -nt "$WASM_STAMP" ]]; then
  echo ">> Building WASM..."
  (cd "$WASM_DIR" && wasm-pack build --target web --dev)
else
  echo ">> WASM build is up to date; skipping rebuild."
fi

echo ">> Starting dev server..."
(cd "$WEB_DIR" && npm run dev &)

echo ">> Done. Dev server starting on http://localhost:5173"
