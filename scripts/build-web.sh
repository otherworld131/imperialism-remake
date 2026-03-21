#!/bin/bash
set -e

echo "=== Building Imperialism Web Frontend ==="

echo "1. Building WASM bridge..."
cd "$(dirname "$0")/.."
cd crates/wasm-bridge
wasm-pack build --target web
echo "   WASM built: $(ls -lh pkg/*.wasm | awk '{print $5}')"

echo "2. Installing web dependencies..."
cd ../../web
npm install --silent

echo "3. Building web app..."
npm run build
echo "   Web app built: $(du -sh dist/ | awk '{print $1}')"

echo ""
echo "=== Done! ==="
echo "To run locally: cd web && npm run dev"
echo "To deploy: upload web/dist/ to any static host"
