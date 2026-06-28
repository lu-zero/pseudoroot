#!/usr/bin/env bash
# Comprehensive benchmark for pseudoroot performance
# Tests: native, pseudoroot (standalone), pseudoroot (daemon mode)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Configuration
N_NATIVE=100000      # stat calls per worker for native (baseline)
N_STANDALONE=50000   # stat calls per worker for standalone pseudoroot
N_DAEMON=50000       # stat calls per worker for daemon pseudoroot
WORKERS=(1 2 4 8)
HELPER="$ROOT/target/release/stat-loop"
PSEUDOROOT="$ROOT/target/release/pseudoroot"
PSEUDOROOT_LIB="$ROOT/target/release/libpseudoroot_lib.so"
SOCKET_PATH="/tmp/pseudoroot_bench.sock"

echo "=== Comprehensive pseudoroot Performance Benchmark ==="
echo "Testing stat() performance: Native vs Standalone vs Daemon mode"
echo "Native: $N_NATIVE calls/worker"
echo "Pseudoroot (standalone): $N_STANDALONE calls/worker"
echo "Pseudoroot (daemon): $N_DAEMON calls/worker"
echo ""

# Create test directory with many files
WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

echo "Creating test files in $WORKDIR..."
for i in $(seq 0 511); do
    echo "$i" > "$WORKDIR/f$i"
done
echo "Created 512 test files"
echo ""

# Start daemon for daemon mode testing
echo "Starting pseudoroot daemon..."
$PSEUDOROOT run --daemon --socket-path $SOCKET_PATH --uid 0 --gid 0 sleep infinity &
DAEMON_PID=$!
sleep 2  # Give daemon time to start

# Benchmark functions
run_native() {
    local workers=$1
    $HELPER $N_NATIVE $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

run_standalone() {
    local workers=$1
    $PSEUDOROOT run --uid 0 --gid 0 $HELPER $N_STANDALONE $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

run_daemon() {
    local workers=$1
    $PSEUDOROOT run --daemon --socket-path $SOCKET_PATH --uid 0 --gid 0 $HELPER $N_DAEMON $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

# Kill daemon
echo "Stopping pseudoroot daemon..."
kill $DAEMON_PID 2>/dev/null || true
sleep 1

echo "Benchmark Results:"
echo ""
printf "%10s %15s %15s %15s\n" "Workers" "Native (stats/s)" "Standalone (stats/s)" "Daemon (stats/s)"
printf "%10s %15s %15s %15s\n" "--------" "-------------" "---------------" "--------------"

for workers in "${WORKERS[@]}"; do
    native_rate=$(run_native $workers)
    standalone_rate=$(run_standalone $workers)
    
    # Start daemon for this test
    $PSEUDOROOT run --daemon --socket-path $SOCKET_PATH --uid 0 --gid 0 sleep infinity &
    DAEMON_PID=$!
    sleep 2
    
    daemon_rate=$(run_daemon $workers)
    
    # Stop daemon
    kill $DAEMON_PID 2>/dev/null || true
    sleep 1
    
    printf "%10d %15.0f %15.0f %15.0f\n" $workers $native_rate $standalone_rate $daemon_rate
done

# Clean up daemon
rm -f $SOCKET_PATH

echo ""
echo "=== Performance Analysis ==="
echo ""

# Single-threaded baselines
BASELINE_NATIVE=$(run_native 1)
BASELINE_STANDALONE=$(run_standalone 1)

# Start daemon for baseline
$PSEUDOROOT run --daemon --socket-path $SOCKET_PATH --uid 0 --gid 0 sleep infinity &
DAEMON_PID=$!
sleep 2
BASELINE_DAEMON=$(run_daemon 1)
kill $DAEMON_PID 2>/dev/null || true
sleep 1
rm -f $SOCKET_PATH

echo "Single-threaded baseline performance:"
printf "  Native:      %15.0f stats/s\n" $BASELINE_NATIVE
printf "  Standalone:  %15.0f stats/s\n" $BASELINE_STANDALONE
printf "  Daemon:      %15.0f stats/s\n" $BASELINE_DAEMON
echo ""

echo "Overhead analysis (compared to native):"
STANDALONE_OVERHEAD=$(echo "scale=1; ($BASELINE_NATIVE - $BASELINE_STANDALONE) * 100 / $BASELINE_NATIVE" | bc)
DAEMON_OVERHEAD=$(echo "scale=1; ($BASELINE_NATIVE - $BASELINE_DAEMON) * 100 / $BASELINE_NATIVE" | bc)
printf "  Standalone overhead: %15.1f%%\n" $STANDALONE_OVERHEAD
printf "  Daemon overhead:    %15.1f%%\n" $DAEMON_OVERHEAD
echo ""

echo "Benchmark complete!"

# Final cleanup
rm -rf "$WORKDIR"
rm -f $SOCKET_PATH