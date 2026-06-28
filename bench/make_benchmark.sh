#!/usr/bin/env bash
# Benchmark pseudoroot vs fakeroot vs fakeroost with real make -j N builds

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEST_PKG="$ROOT/bench/test-package"
PSEUDOROOT="$ROOT/target/release/pseudoroot"
FAKEROOST_BIN="$ROOT/../fakeroost/target/release/fakeroost"
J_LEVELS=(1 2 4 8)
N_RUNS=3

# Build pseudoroot
if [ ! -x "$PSEUDOROOT" ]; then
    echo "Building pseudoroot..."
    cd "$ROOT"
    cargo build --release 2>&1 | tail -1
fi

# Build fakeroost if available
if [ -f "$ROOT/../fakeroost/Cargo.toml" ] && [ ! -x "$FAKEROOST_BIN" ]; then
    echo "Building fakeroost..."
    cd "$ROOT/../fakeroost"
    cargo build --release 2>&1 | tail -1
fi

# Prepare test package - expand to 200 files for better measurement
cd "$TEST_PKG"
if [ ! -f Makefile ]; then
    cat > Makefile << 'EOF'
SRCS = $(wildcard file*.txt)
TARGETS = $(SRCS:.txt=.out)

all: $(TARGETS)

%.out: %.txt
	cp $< $@

clean:
	rm -f $(TARGETS)

.PHONY: all clean
EOF
    for i in $(seq 1 200); do echo "content $i" > "file$i.txt"; done
fi

clean_pkg() {
    cd "$TEST_PKG"
    make clean >/dev/null 2>&1 || true
}

# Simple timing function
run_test() {
    local wrapper="$1"
    local jobs="$2"
    
    clean_pkg >/dev/null 2>&1
    
    local cmd="make -j$jobs"
    local start end elapsed
    
    if [ "$wrapper" = "native" ]; then
        start=$(date +%s%N)
        cd "$TEST_PKG" && $cmd >/dev/null 2>&1
        end=$(date +%s%N)
    elif [ "$wrapper" = "pseudoroot" ]; then
        start=$(date +%s%N)
        cd "$TEST_PKG" && $PSEUDOROOT run -- $cmd >/dev/null 2>&1
        end=$(date +%s%N)
    elif [ "$wrapper" = "fakeroot" ]; then
        start=$(date +%s%N)
        cd "$TEST_PKG" && fakeroot -- $cmd >/dev/null 2>&1
        end=$(date +%s%N)
    elif [ "$wrapper" = "fakeroost" ] && [ -x "$FAKEROOST_BIN" ]; then
        start=$(date +%s%N)
        cd "$TEST_PKG" && $FAKEROOST_BIN $cmd >/dev/null 2>&1
        end=$(date +%s%N)
    fi
    
    elapsed=$(( (end - start) / 1000000 ))
    echo $elapsed
}

echo "=== make -j N Benchmark ==="
echo "Building 200 files with make -j N"
echo "Test package: $TEST_PKG"
echo ""

# Collect results
declare -A results

for wrapper in native pseudoroot fakeroot fakeroost; do
    if [ "$wrapper" = "fakeroost" ] && [ ! -x "$FAKEROOST_BIN" ]; then
        continue
    fi
    if [ "$wrapper" = "fakeroot" ] && ! command -v fakeroot >/dev/null 2>&1; then
        continue
    fi
    
    for jobs in "${J_LEVELS[@]}"; do
        echo -n "Testing $wrapper with -j$jobs: "
        total=0
        for ((run=1; run<=$N_RUNS; run++)); do
            elapsed=$(run_test "$wrapper" "$jobs")
            total=$((total + elapsed))
            echo -n "."
        done
        avg=$((total / N_RUNS))
        results["$wrapper,$jobs"]=$avg
        echo " $avg ms"
    done
done

echo ""
echo "=== Results (milliseconds, lower is better) ==="
echo ""

# Print header
headers="Jobs"
for wrapper in native pseudoroot fakeroot fakeroost; do
    if [ "$wrapper" = "fakeroost" ] && [ ! -x "$FAKEROOST_BIN" ]; then continue; fi
    if [ "$wrapper" = "fakeroot" ] && ! command -v fakeroot >/dev/null 2>&1; then continue; fi
    headers="$headers $wrapper"
done

echo "$headers"
echo "$(echo $headers | tr ' ' '-')"

for jobs in "${J_LEVELS[@]}"; do
    line="$jobs"
    for wrapper in native pseudoroot fakeroot fakeroost; do
        if [ "$wrapper" = "fakeroost" ] && [ ! -x "$FAKEROOST_BIN" ]; then continue; fi
        if [ "$wrapper" = "fakeroot" ] && ! command -v fakeroot >/dev/null 2>&1; then continue; fi
        line="$line ${results[$wrapper,$jobs]}"
    done
    echo "$line"
done

echo ""
echo "=== Speedup Analysis ==="
echo ""
echo "Speedup vs -j1 (higher is better):"
echo ""

# Print speedup header
speedup_headers="Jobs"
for wrapper in native pseudoroot fakeroot fakeroost; do
    if [ "$wrapper" = "fakeroost" ] && [ ! -x "$FAKEROOST_BIN" ]; then continue; fi
    if [ "$wrapper" = "fakeroot" ] && ! command -v fakeroot >/dev/null 2>&1; then continue; fi
    speedup_headers="$speedup_headers $wrapper"
done

echo "$speedup_headers"
echo "$(echo $speedup_headers | tr ' ' '-')"

for jobs in "${J_LEVELS[@]}"; do
    line="$jobs"
    for wrapper in native pseudoroot fakeroot fakeroost; do
        if [ "$wrapper" = "fakeroost" ] && [ ! -x "$FAKEROOST_BIN" ]; then continue; fi
        if [ "$wrapper" = "fakeroot" ] && ! command -v fakeroot >/dev/null 2>&1; then continue; fi
        
        base=${results[$wrapper,1]}
        current=${results[$wrapper,$jobs]}
        
        if [ "$base" = "0" ] || [ "$base" = "" ]; then
            speedup="0.00"
        else
            speedup=$(awk "BEGIN {printf \"%.2f\", $base / $current}")
        fi
        line="$line $speedup"
    done
    echo "$line"
done

echo ""
echo "Benchmark complete!"

# Cleanup
clean_pkg >/dev/null 2>&1
