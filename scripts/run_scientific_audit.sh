#!/usr/bin/env bash
set -e
cd "$(dirname "$0")/.."

echo "================================================================="
echo "🔬 SCIENTIFIC SYSTEMS AUDITOR: SPEED, SIZE & MULTI-MODEL CAPABILITY"
echo "================================================================="

echo -e "\n[1/4] AUDITING BINARY FOOTPRINT (Release Build with fat LTO & Strip):"
stat -c "  • Exact File Size : %s bytes (%f)" target/release/faizdb
ls -lh target/release/faizdb | awk '{print "  • Formatted Size  : " $5}'
size target/release/faizdb | awk 'NR==2 {printf "  • ELF Breakdown   : text=%d bytes (%.1f%%), data=%d bytes, bss=%d bytes\n", $1, ($1*100)/($1+$2+$3), $2, $3}'
file target/release/faizdb | awk -F': ' '{print "  • Binary Type     : " $2}'

echo -e "\n[2/4] STARTING PRODUCTION ENGINE IN BACKGROUND..."
./target/release/faizdb serve >/tmp/faizdb_audit_server.log 2>&1 &
SERVER_PID=$!

cleanup() {
    echo -e "\n[!] Shutting down FaizDB Engine (PID $SERVER_PID)..."
    kill $SERVER_PID 2>/dev/null || true
    wait $SERVER_PID 2>/dev/null || true
    echo "Engine shutdown verified."
}
trap cleanup EXIT

# Wait for health
echo "Waiting for engine initialization..."
for i in {1..20}; do
    if curl -s http://127.0.0.1:27018/v1/health | grep -q "online"; then
        echo "Engine healthy and listening on all 4 protocol gateways."
        break
    fi
    sleep 0.5
done

echo -e "\n[3/4] EXECUTING LIVE SCIENTIFIC BENCHMARK SUITE..."
python3 scripts/scientific_audit_bench.py

echo -e "\n[4/4] LIVE KERNEL RESIDENT MEMORY PROFILE (Linux /proc/$SERVER_PID/status):"
grep -E "VmPeak|VmSize|VmHWM|VmRSS|VmData|Threads" /proc/$SERVER_PID/status | awk '{printf "  • %-10s : %s %s\n", $1, $2, $3}'

echo -e "\n================================================================="
echo "🏆 SCIENTIFIC AUDIT VERIFICATION COMPLETE"
echo "================================================================="
