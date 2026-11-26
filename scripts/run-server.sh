#!/usr/bin/env bash
# Runs the server and restarts it after a crash or a watchdog stop.
# A normal /stop (exit code 0) ends the loop. Too many crashes in a short time also
# stop it, so a broken world is not restarted forever.
#
# Usage: scripts/run-server.sh [path/to/pumpkin]   (run from the server folder)
set -u

BINARY="${1:-./pumpkin}"
RESTART_DELAY=10
MAX_CRASHES=5
CRASH_WINDOW=600

crash_times=()
while true; do
    echo "[run-server] Starting $BINARY at $(date '+%F %T')"
    "$BINARY"
    code=$?
    if [ "$code" -eq 0 ]; then
        echo "[run-server] Server stopped normally."
        exit 0
    fi

    now=$(date +%s)
    echo "[run-server] Server exited with code $code at $(date '+%F %T')" | tee -a crash-reports/restarts.log 2>/dev/null \
        || echo "[run-server] Server exited with code $code"
    recent=()
    for t in "${crash_times[@]}"; do
        if [ $((now - t)) -lt $CRASH_WINDOW ]; then recent+=("$t"); fi
    done
    recent+=("$now")
    crash_times=("${recent[@]}")
    if [ "${#crash_times[@]}" -ge "$MAX_CRASHES" ]; then
        echo "[run-server] $MAX_CRASHES crashes within $((CRASH_WINDOW / 60)) minutes, not restarting. Check crash-reports/."
        exit 1
    fi
    echo "[run-server] Restarting in ${RESTART_DELAY}s (Ctrl+C to cancel)..."
    sleep "$RESTART_DELAY"
done
