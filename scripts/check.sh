#!/bin/bash
set -e
echo "=== Quick check (build + test + lint) ==="
cargo build 2>&1
cargo test --workspace 2>&1 | tail -20
cargo fmt --check 2>&1
cargo clippy --workspace 2>&1 | tail -5
echo "=== All checks passed ==="
