#!/usr/bin/env bash
set -e
cd "$(dirname "$0")/.."

echo "Starting FaizDB multi-protocol server on test ports..."
./target/release/faizdb serve -w 37017 -g 35432 -r 30051 -p 37018 -H 127.0.0.1 >/tmp/faizdb_serve.log 2>&1 &
SERVER_PID=$!

sleep 2

if ps -p $SERVER_PID > /dev/null; then
    echo "Server running with PID: $SERVER_PID"
    echo "=== RESIDENT MEMORY PROFILE (/proc/$SERVER_PID/status) ==="
    grep -E "VmPeak|VmSize|VmHWM|VmRSS|VmData|Threads" /proc/$SERVER_PID/status
    kill $SERVER_PID 2>/dev/null || true
    echo "Server shutdown complete."
else
    echo "Server failed to start. Log output:"
    cat /tmp/faizdb_serve.log
    exit 1
fi
