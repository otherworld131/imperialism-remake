#!/bin/bash
# Kill existing dev server, rebuild WASM only when needed, and restart Vite.
# Usage: ./web/restart-web-server.sh [--force] [--opt|--no-opt]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WEB_DIR="$REPO_ROOT/web"
WASM_DIR="$REPO_ROOT/crates/wasm-bridge"
WASM_STAMP="$WASM_DIR/pkg/wasm_bridge_bg.wasm"
WASM_MODE_STAMP="$WASM_DIR/pkg/.restart-web-server-build-mode"
NPM_INSTALL_STAMP="$WEB_DIR/node_modules/.install-stamp"
FORCE_REBUILD=0
WASM_BUILD_MODE="dev-no-opt"

usage() {
  cat <<EOF
Usage: ./web/restart-web-server.sh [--force] [--opt|--no-opt]

Options:
  --force   Rebuild WASM even if sources look unchanged
  --opt     Build optimized WASM for runtime performance testing
  --no-opt  Build fast dev WASM without optimization (default)
  --help    Show this help text
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE_REBUILD=1
      ;;
    --opt)
      WASM_BUILD_MODE="opt"
      ;;
    --no-opt)
      WASM_BUILD_MODE="dev-no-opt"
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

needs_wasm_rebuild() {
  if [[ $FORCE_REBUILD -eq 1 || ! -f "$WASM_STAMP" ]]; then
    return 0
  fi

  if [[ ! -f "$WASM_MODE_STAMP" ]] || [[ "$(cat "$WASM_MODE_STAMP")" != "$WASM_BUILD_MODE" ]]; then
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
  echo ">> Building WASM ($WASM_BUILD_MODE)..."
  if [[ "$WASM_BUILD_MODE" == "opt" ]]; then
    (cd "$WASM_DIR" && wasm-pack build --target web)
  else
    (cd "$WASM_DIR" && wasm-pack build --target web --dev --no-opt)
  fi
  printf '%s\n' "$WASM_BUILD_MODE" > "$WASM_MODE_STAMP"
else
  echo ">> WASM build is up to date for mode '$WASM_BUILD_MODE'; skipping rebuild."
fi

echo ">> Starting dev server..."
(cd "$WEB_DIR" && npm run dev &)

echo ">> Done. Dev server starting on http://localhost:5173"
