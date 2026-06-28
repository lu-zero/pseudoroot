#!/usr/bin/env bash
# Daemon mode benchmark for pseudoroot
# Compares standalone vs daemon mode performance

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Configuration
N_STANDALONE=50000   # stat calls per worker for standalone pseudoroot
N_DAEMON=50000       # stat calls per worker for daemon pseudoroot
WORKERS=(1 2 4 8)
HELPER="$ROOT/target/release/stat-loop"
PSEUDOROOT="$ROOT/target/release/pseudoroot"
PSEUDOROOT_DAEMON="$ROOT/target/release/pseudoroot-daemon"
SOCKET_PATH="/tmp/pseudoroot_daemon_bench.sock"

echo "=== pseudoroot Daemon Mode Performance Benchmark ==="
echo "Testing stat() performance: Standalone vs Daemon mode"
echo "Each: $N_STANDALONE calls/worker"
echo ""

# Create test directory with many files
WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"; rm -f "$SOCKET_PATH"' EXIT

echo "Creating test files in $WORKDIR..."
for i in $(seq 0 511); do
    echo "$i" > "$WORKDIR/f$i"
done
echo "Created 512 test files"
echo ""

# Benchmark functions
run_standalone() {
    local workers=$1
    $PSEUDOROOT run --uid 0 --gid 0 $HELPER $N_STANDALONE $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

run_daemon() {
    local workers=$1
    PSEUDOROOT_DAEMON_SOCKET=$SOCKET_PATH $PSEUDOROOT run --uid 0 --gid 0 $HELPER $N_DAEMON $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

echo "Starting pseudoroot daemon for benchmarking..."
$PSEUDOROOT_DAEMON --socket-path $SOCKET_PATH --uid 0 --gid 0 &
DAEMON_PID=$!
sleep 2

if ! [ -S $SOCKET_PATH ]; then
    echo "Error: Daemon socket not created at $SOCKET_PATH"
    kill $DAEMON_PID 2>/dev/null || true
    exit 1
fi

echo "Daemon started successfully (PID: $DAEMON_PID)"
echo ""

echo "Benchmark Results:"
echo ""
printf "%10s %15s %15s\n" "Workers" "Standalone (stats/s)" "Daemon (stats/s)"
printf "%10s %15s %15s\n" "--------" "---------------" "--------------"

for workers in "${WORKERS[@]}"; do
    standalone_rate=$(run_standalone $workers)
    daemon_rate=$(run_daemon $workers)
    printf "%10d %15.0f %15.0f\n" $workers $standalone_rate $daemon_rate
done

# Clean up daemon
kill $DAEMON_PID 2>/dev/null || true
sleep 1
rm -f $SOCKET_PATH

echo ""
echo "=== Performance Analysis ==="
echo ""

# Single-threaded baselines
BASELINE_STANDALONE=$(run_standalone 1)

# Restart daemon for baseline
$PSEUDOROOT_DAEMON --socket-path $SOCKET_PATH --uid 0 --gid 0 &
DAEMON_PID=$!
sleep 2
BASELINE_DAEMON=$(run_daemon 1)
kill $DAEMON_PID 2>/dev/null || true
sleep 1
rm -f $SOCKET_PATH

echo "Single-threaded baseline performance:"
printf "  Standalone:  %15.0f stats/s\n" $BASELINE_STANDALONE
printf "  Daemon:      %15.0f stats/s\n" $BASELINE_DAEMON
echo ""

if [ $BASELINE_DAEMON -gt $BASELINE_STANDALONE ]; then
    IMPROVEMENT=$(echo "scale=1; ($BASELINE_DAEMON - $BASELINE_STANDALONE) * 100 / $BASELINE_STANDALONE" | bc)
    printf "  Daemon is %15.1f%% FASTER than standalone\n" $IMPROVEMENT
else
    OVERHEAD=$(echo "scale=1; ($BASELINE_STANDALONE - $BASELINE_DAEMON) * 100 / $BASELINE_STANDALONE" | bc)
    printf "  Daemon overhead: %15.1f%%\n" $OVERHEAD
fi

echo ""
echo "Benchmark complete!"

# Final cleanup
rm -rf "$WORKDIR"
rm -f $SOCKET_PATH