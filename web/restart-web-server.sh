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
STATUS_FILE="$WEB_DIR/public/dev-server-status.json"
FORCE_REBUILD=0
WASM_BUILD_MODE="dev-no-opt"
DEV_SERVER_PORT=43173
DEV_SERVER_LOG="$WEB_DIR/.dev-server.log"

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
    "$REPO_ROOT/scripts" \
    -type f -newer "$WASM_STAMP" -print -quit | grep -q .
}

needs_npm_install() {
  [[ ! -d "$WEB_DIR/node_modules" || ! -f "$NPM_INSTALL_STAMP" ]] && return 0
  [[ "$WEB_DIR/package.json" -nt "$NPM_INSTALL_STAMP" ]] && return 0
  [[ "$WEB_DIR/package-lock.json" -nt "$NPM_INSTALL_STAMP" ]] && return 0
  return 1
}

write_status() {
  local phase="$1"
  local detail="$2"

  mkdir -p "$(dirname "$STATUS_FILE")"
  cat > "$STATUS_FILE" <<EOF
{
  "phase": "$phase",
  "detail": "$detail",
  "mode": "$WASM_BUILD_MODE",
  "updatedAt": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
}
EOF
}

wait_for_dev_server() {
  local attempts=60
  local url="http://localhost:$DEV_SERVER_PORT/dev-server-status.json"

  while (( attempts > 0 )); do
    if curl --silent --fail "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
    attempts=$((attempts - 1))
  done

  return 1
}

trap 'write_status "failed" "Restart failed"' ERR

write_status "restarting" "Stopping current dev server"
echo ">> Killing dev server..."
DEV_SERVER_PIDS="$(lsof -ti :"$DEV_SERVER_PORT" 2>/dev/null || true)"
if [[ -n "$DEV_SERVER_PIDS" ]]; then
  kill $DEV_SERVER_PIDS 2>/dev/null || true
fi

if needs_npm_install; then
  write_status "installing-deps" "Installing web dependencies"
  echo ">> Installing web dependencies..."
  (
    cd "$WEB_DIR"
    npm install
    touch "$NPM_INSTALL_STAMP"
  )
fi

if needs_wasm_rebuild || [[ "$REPO_ROOT/Cargo.lock" -nt "$WASM_STAMP" ]] || [[ "$REPO_ROOT/Cargo.toml" -nt "$WASM_STAMP" ]]; then
  write_status "building-wasm" "Compiling WASM bundle"
  echo ">> Building WASM ($WASM_BUILD_MODE)..."
  if [[ "$WASM_BUILD_MODE" == "opt" ]]; then
    (cd "$WASM_DIR" && wasm-pack build --target web)
  else
    (cd "$WASM_DIR" && wasm-pack build --target web --dev --no-opt)
  fi
  printf '%s\n' "$WASM_BUILD_MODE" > "$WASM_MODE_STAMP"
else
  write_status "wasm-ready" "WASM bundle already up to date"
  echo ">> WASM build is up to date for mode '$WASM_BUILD_MODE'; skipping rebuild."
fi

write_status "starting-dev-server" "Booting Vite dev server"
echo ">> Starting dev server..."
: > "$DEV_SERVER_LOG"
(
  cd "$WEB_DIR"
  nohup npm run dev -- --port "$DEV_SERVER_PORT" >>"$DEV_SERVER_LOG" 2>&1 < /dev/null &
)

if wait_for_dev_server; then
  write_status "ready" "Ready"
else
  write_status "failed" "Timed out waiting for dev server"
  echo ">> Timed out waiting for dev server on http://localhost:$DEV_SERVER_PORT" >&2
  echo ">> Dev server log: $DEV_SERVER_LOG" >&2
  exit 1
fi

echo ">> Done. Dev server starting on http://localhost:$DEV_SERVER_PORT"
echo ">> Dev server log: $DEV_SERVER_LOG"
