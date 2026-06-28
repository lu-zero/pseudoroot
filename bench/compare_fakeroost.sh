#!/usr/bin/env bash
# Compare pseudoroot vs fakeroost performance

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Configuration
N_NATIVE=100000      # stat calls per worker for native (baseline)
N_FAKE=50000         # stat calls per worker for fake implementations
WORKERS=(1 2 4 8)
HELPER="$ROOT/target/release/stat-loop"
PSEUDOROOT="$ROOT/target/release/pseudoroot"

echo "=== pseudoroot vs fakeroost Performance Comparison ==="
echo "Testing stat() performance across different implementations"
echo "Native: $N_NATIVE calls/worker, Fake: $N_FAKE calls/worker"
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
    $PSEUDOROOT run --uid 0 --gid 0 $HELPER $N_FAKE $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

run_fakeroot() {
    local workers=$1
    fakeroot -- $HELPER $N_FAKE $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2
}

# Check if fakeroot is available
if ! command -v fakeroot >/dev/null 2>&1; then
    echo "Error: fakeroot is not installed. Cannot compare."
    exit 1
fi

echo "Benchmark Results:"
echo ""
printf "%10s %15s %15s %15s\n" "Workers" "Native (stats/s)" "pseudoroot (stats/s)" "fakeroot (stats/s)"
printf "%10s %15s %15s %15s\n" "--------" "-------------" "-----------------" "----------------"

for workers in "${WORKERS[@]}"; do
    native_rate=$(run_native $workers)
    pseudo_rate=$(run_pseudoroot $workers)
    fake_rate=$(run_fakeroot $workers)
    printf "%10d %15.0f %15.0f %15.0f\n" $workers $native_rate $pseudo_rate $fake_rate
done

echo ""
echo "=== Performance Analysis ==="
echo ""

# Single-threaded baselines
BASELINE_NATIVE=$(run_native 1)
BASELINE_PSEUDO=$(run_pseudoroot 1)
BASELINE_FAKE=$(run_fakeroot 1)

echo "Single-threaded baseline performance:"
printf "  Native:      %15.0f stats/s\n" $BASELINE_NATIVE
printf "  pseudoroot:  %15.0f stats/s\n" $BASELINE_PSEUDO
printf "  fakeroot:    %15.0f stats/s\n" $BASELINE_FAKE
echo ""

# Overhead calculations
PSEUDO_OVERHEAD=$(echo "scale=1; ($BASELINE_NATIVE - $BASELINE_PSEUDO) * 100 / $BASELINE_NATIVE" | bc)
FAKE_OVERHEAD=$(echo "scale=1; ($BASELINE_NATIVE - $BASELINE_FAKE) * 100 / $BASELINE_NATIVE" | bc)

echo "Overhead compared to native:"
printf "  pseudoroot:  %15.1f%%\n" $PSEUDO_OVERHEAD
printf "  fakeroot:    %15.1f%%\n" $FAKE_OVERHEAD
echo ""

# Compare overheads using bc for floating point comparison
if (( $(echo "$PSEUDO_OVERHEAD < $FAKE_OVERHEAD" | bc -l) )); then
    IMPROVEMENT=$(echo "scale=1; ($FAKE_OVERHEAD - $PSEUDO_OVERHEAD)" | bc)
    echo "✅ pseudoroot is $IMPROVEMENT% MORE EFFICIENT than fakeroot"
else
    SLOWER=$(echo "scale=1; ($PSEUDO_OVERHEAD - $FAKE_OVERHEAD)" | bc)
    echo "❌ pseudoroot is $SLOWER% SLOWER than fakeroot"
fi

echo ""
echo "=== Parallelism Comparison ==="
echo ""
printf "%10s %15s %15s %15s\n" "Workers" "Native x1" "pseudoroot x1" "fakeroot x1"
printf "%10s %15s %15s %15s\n" "--------" "----------" "--------------" "------------"

for workers in "${WORKERS[@]}"; do
    native_rate=$(run_native $workers)
    pseudo_rate=$(run_pseudoroot $workers)
    fake_rate=$(run_fakeroot $workers)
    
    native_speedup=$(echo "scale=2; $native_rate / $BASELINE_NATIVE" | bc)
    pseudo_speedup=$(echo "scale=2; $pseudo_rate / $BASELINE_PSEUDO" | bc)
    fake_speedup=$(echo "scale=2; $fake_rate / $BASELINE_FAKE" | bc)
    
    printf "%10d %15.2fx %15.2fx %15.2fx\n" $workers $native_speedup $pseudo_speedup $fake_speedup
done

echo ""
echo "Benchmark complete!"

# Cleanup
rm -rf "$WORKDIR"