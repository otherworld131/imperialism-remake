#!/bin/bash
set -e
echo "=== Running all tests ==="
cargo test --workspace
echo ""
echo "=== Checking formatting ==="
cargo fmt --check
echo ""
echo "=== Running clippy ==="
cargo clippy --workspace
echo ""
echo "=== All checks passed ==="
