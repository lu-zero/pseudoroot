#!/usr/bin/env bash
# Test Papaya vs DashMap performance

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

# Create test files
for i in $(seq 0 511); do
    echo "$i" > "$WORKDIR/f$i"
done

echo "=== Testing Papaya vs DashMap ==="
echo ""

# Test DashMap
echo "Testing with DashMap..."
sed -i 's/.*dashmap.*//' crates/pseudoroot-core/Cargo.toml
sed -i '/\[dependencies\]/a dashmap = "5.5"' crates/pseudoroot-core/Cargo.toml
sed -i 's/use .*//' crates/pseudoroot-core/src/state.rs
sed -i '/use std::collections::HashMap;/a use dashmap::DashMap;' crates/pseudoroot-core/src/state.rs
sed -i 's/ownership_map: .*/ownership_map: DashMap<String, FileOwnership>,' crates/pseudoroot-core/src/state.rs
sed -i 's/ownership_map: .*/ownership_map: DashMap::new(),/' crates/pseudoroot-core/src/state.rs

cargo build --release -q -p pseudoroot-core -p pseudoroot-lib -p pseudoroot

DASHMAP_RATE=$(./target/release/pseudoroot run --uid 0 --gid 0 ./target/release/stat-loop 50000 4 $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2)
echo "DashMap: $DASHMAP_RATE stats/s"

# Test Papaya
echo "Testing with Papaya..."
sed -i 's/dashmap = "5.5"/papaya = "0.2"/' crates/pseudoroot-core/Cargo.toml
sed -i 's/use dashmap::DashMap;/use papaya::HashMap as PapayaHashMap;/' crates/pseudoroot-core/src/state.rs
sed -i 's/ownership_map: DashMap<String, FileOwnership>/ownership_map: PapayaHashMap<String, FileOwnership>/' crates/pseudoroot-core/src/state.rs
sed -i 's/ownership_map: DashMap::new()/ownership_map: PapayaHashMap::new()/' crates/pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.insert(path, ownership);/self.ownership_map.pin().insert(path, ownership);/' crates/pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.get(path).map(|entry| \*entry.value())/self.ownership_map.pin().get(path).copied()/' crates/pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.remove(path).map(|(_, v)| v)/self.ownership_map.pin().remove(path).copied()/' crates/pseudoroot-core/src/state.rs

cargo build --release -q -p pseudoroot-core -p pseudoroot-lib -p pseudoroot

PAPAYA_RATE=$(./target/release/pseudoroot run --uid 0 --gid 0 ./target/release/stat-loop 50000 4 $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2)
echo "Papaya:  $PAPAYA_RATE stats/s"

# Restore DashMap
git checkout crates/pseudoroot-core/Cargo.toml crates/pseudoroot-core/src/state.rs
cargo build --release -q -p pseudoroot-core -p pseudoroot-lib -p pseudoroot

echo ""
echo "Results:"
echo "DashMap: $DASHMAP_RATE stats/s"
echo "Papaya:  $PAPAYA_RATE stats/s"

if [ $PAPAYA_RATE -gt $DASHMAP_RATE ]; then
    IMPROVEMENT=$(echo "scale=1; ($PAPAYA_RATE - $DASHMAP_RATE) * 100 / $DASHMAP_RATE" | bc)
    echo "Papaya is $IMPROVEMENT% FASTER"
else
    OVERHEAD=$(echo "scale=1; ($DASHMAP_RATE - $PAPAYA_RATE) * 100 / $DASHMAP_RATE" | bc)
    echo "Papaya is $OVERHEAD% SLOWER"
fi