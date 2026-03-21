#!/bin/bash
set -e
echo "=== Smoke Test ==="
echo "Building..."
cargo build --release 2>&1 | tail -1

echo "Running game for 5 seconds..."
echo -e "turn\nstatus\nquit" | timeout 10 cargo run -- "smoke_test" 0 > /tmp/smoke_output.txt 2>&1
EXIT_CODE=$?

if [ $EXIT_CODE -eq 0 ]; then
    echo "Game ran and exited cleanly."
    # Check output contains expected strings
    grep -q "IMPERIALISM REMAKE" /tmp/smoke_output.txt && echo "  Title displayed: OK"
    grep -q "Playing as:" /tmp/smoke_output.txt && echo "  Game started: OK"
    grep -q "Farewell" /tmp/smoke_output.txt && echo "  Clean exit: OK"
    echo "=== Smoke Test PASSED ==="
else
    echo "=== Smoke Test FAILED (exit code $EXIT_CODE) ==="
    cat /tmp/smoke_output.txt
    exit 1
fi
