#!/usr/bin/env bash
# Benchmark script for pseudoroot performance testing
# Compares native vs pseudoroot stat() performance

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Configuration
N_NATIVE=100000    # stat calls per worker for native (baseline)
N_PSEUDOROOT=50000 # stat calls per worker for pseudoroot
WORKERS=(1 2 4 8)
HELPER="$ROOT/target/release/stat-loop"
PSEUDOROOT="$ROOT/target/release/pseudoroot"

echo "=== pseudoroot Performance Benchmark ==="
echo "Testing stat() performance with different worker counts"
echo "Native: $N_NATIVE calls/worker, pseudoroot: $N_PSEUDOROOT calls/worker"
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

# Benchmark functions
run_native() {
    local workers=$1
    $HELPER $N_NATIVE $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

run_pseudoroot() {
    local workers=$1
    $PSEUDOROOT run --uid 0 --gid 0 $HELPER $N_PSEUDOROOT $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

echo "Benchmark Results:"
echo ""
printf "%10s %15s %15s\n" "Workers" "Native (stats/s)" "Pseudoroot (stats/s)"
printf "%10s %15s %15s\n" "--------" "-------------" "--------------"

for workers in "${WORKERS[@]}"; do
    native_rate=$(run_native $workers)
    pseudo_rate=$(run_pseudoroot $workers)
    printf "%10d %15.0f %15.0f\n" $workers $native_rate $pseudo_rate
done

echo ""
echo "=== Parallelism Analysis ==="
echo "Calculating speedup vs single-threaded..."
echo ""

# Get baseline rates
BASELINE_NATIVE=$(run_native 1)
BASELINE_PSEUDO=$(run_pseudoroot 1)

printf "%10s %15s %15s\n" "Workers" "Native x1" "Pseudoroot x1"
printf "%10s %15s %15s\n" "--------" "----------" "------------"

for workers in "${WORKERS[@]}"; do
    native_rate=$(run_native $workers)
    pseudo_rate=$(run_pseudoroot $workers)
    
    native_speedup=$(echo "scale=2; $native_rate / $BASELINE_NATIVE" | bc)
    pseudo_speedup=$(echo "scale=2; $pseudo_rate / $BASELINE_PSEUDO" | bc)
    
    printf "%10d %15.2fx %15.2fx\n" $workers $native_speedup $pseudo_speedup
done

echo ""
echo "Benchmark complete!"

# Cleanup
rm -rf "$WORKDIR"