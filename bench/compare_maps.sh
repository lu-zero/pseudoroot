#!/usr/bin/env bash
# Compare different concurrent map implementations for pseudoroot
# Tests: HashMap + RwLock (baseline), DashMap, Papaya

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== Concurrent Map Comparison for pseudoroot ==="
echo ""
echo "This script will test different concurrent map implementations:"
echo "1. HashMap + RwLock (baseline)"
echo "2. DashMap"
echo "3. Papaya"
echo ""
echo "Each test will run a stat() benchmark and report results."
echo ""

# Configuration
N_CALLS=50000
WORKERS=(1 2 4 8)
WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

# Create test files
for i in $(seq 0 511); do
    echo "$i" > "$WORKDIR/f$i"
done

HELPER="$ROOT/target/release/stat-loop"

echo "Creating test files... Done."
echo ""

# Function to run benchmark with current map implementation
run_benchmark() {
    local map_name=$1
    echo "======================================"
    echo "Testing: $map_name"
    echo "======================================"
    
    # Build with current map
    cargo build --release -p pseudoroot-core -p pseudoroot-lib -p pseudoroot >/dev/null 2>&1
    
    printf "%10s %15s\n" "Workers" "$map_name (stats/s)"
    printf "%10s %15s\n" "--------" "------------------"
    
    for workers in "${WORKERS[@]}"; do
        rate=$(./target/release/pseudoroot run --uid 0 --gid 0 $HELPER $N_CALLS $workers $WORKDIR 2>&1 | grep -o 'rate=[0-9.]*' | cut -d= -f2 || echo "0")
        printf "%10d %15.0f\n" $workers $rate
    done
    echo ""
}

# Test 1: HashMap + RwLock (current implementation in state.rs)
echo "Switching to HashMap + RwLock..."
patch -p1 << 'EOF' || true
--- a/pseudoroot-core/src/state.rs
+++ b/pseudoroot-core/src/state.rs
@@ -1,7 +1,6 @@
 //! Fake root state management
 //!
 //! This module provides the core state structures for tracking fake ownership
 //! and permissions in the pseudoroot system.
 -
-use dashmap::DashMap;
 use std::collections::HashMap;
 use std::sync::atomic::{AtomicU32, Ordering};
@@ -110,7 +109,7 @@ impl UidGidMap {
 pub struct FakeRootState {
     /// Mapping from real to fake UID/GID
     pub uid_gid_map: UidGidMap,
-    /// Map from file path to its fake ownership (concurrent HashMap for better performance)
+    /// Map from file path to its fake ownership
     /// Note: This uses String keys for paths; in a daemon-based implementation,
     /// this would be more sophisticated (inode-based, etc.)
@@ -128,7 +127,7 @@ impl Default for FakeRootState {
         Self {
             uid_gid_map: UidGidMap::default(),
-            ownership_map: DashMap::new(),
+            ownership_map: HashMap::new(),
             current_uid: AtomicU32::new(0),
             current_gid: AtomicU32::new(0),
@@ -156,11 +155,11 @@ impl FakeRootState {
     /// Set the ownership of a file or directory (lock-free concurrent insert)
     #[inline]
     pub fn set_ownership(&mut self, path: String, ownership: FileOwnership) {
-        self.ownership_map.insert(path, ownership);
+        self.ownership_map.insert(path, ownership);
     }
 
     /// Get the ownership of a file or directory (lock-free concurrent read)
     #[inline]
     #[must_use]
     pub fn get_ownership(&self, path: &str) -> Option<FileOwnership> {
-        self.ownership_map.get(path).map(|entry| *entry.value())
+        self.ownership_map.get(path).copied()
     }
 
     /// Remove the ownership entry for a file or directory
EOF

# Actually, let me just manually test the different maps by editing the files
# Test with current DashMap first
run_benchmark "DashMap"

# Test with Papaya
sed -i 's/dashmap = "5.5"/papaya = "0.2"/' pseudoroot-core/Cargo.toml
sed -i 's/use dashmap::DashMap;/use papaya::HashMap as PapayaHashMap;/' pseudoroot-core/src/state.rs
sed -i 's/ownership_map: DashMap<String, FileOwnership>/ownership_map: PapayaHashMap<String, FileOwnership>/' pseudoroot-core/src/state.rs
sed -i 's/ownership_map: DashMap::new()/ownership_map: PapayaHashMap::new()/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.insert(path, ownership);/self.ownership_map.pin().insert(path, ownership);/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.get(path).map(|entry| \*entry.value())/self.ownership_map.pin().get(path).copied()/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.remove(path).map(|(_, v)| v)/self.ownership_map.pin().remove(path).copied()/' pseudoroot-core/src/state.rs

run_benchmark "Papaya"

# Test with RwLock + HashMap
sed -i 's/papaya = "0.2"/dashmap = "5.5"/' pseudoroot-core/Cargo.toml  # First revert to dashmap
sed -i 's/use papaya::HashMap as PapayaHashMap;/use dashmap::DashMap;/' pseudoroot-core/src/state.rs
# Now switch to RwLock + HashMap
sed -i 's/dashmap = "5.5"/# dashmap = "5.5"/' pseudoroot-core/Cargo.toml
sed -i 's/use dashmap::DashMap;/use std::sync::RwLock;/' pseudoroot-core/src/state.rs
sed -i 's/ownership_map: PapayaHashMap<String, FileOwnership>/ownership_map: RwLock<HashMap<String, FileOwnership>>/' pseudoroot-core/src/state.rs
sed -i 's/ownership_map: PapayaHashMap::new()/ownership_map: RwLock::new(HashMap::new())/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.pin().insert(path, ownership);/self.ownership_map.write().unwrap().insert(path, ownership);/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.pin().get(path).copied()/self.ownership_map.read().unwrap().get(path).copied()/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.pin().remove(path).copied()/self.ownership_map.write().unwrap().remove(path)/' pseudoroot-core/src/state.rs

run_benchmark "HashMap + RwLock"

# Restore DashMap
sed -i 's/# dashmap = "5.5"/dashmap = "5.5"/' pseudoroot-core/Cargo.toml
sed -i 's/use std::sync::RwLock;/use dashmap::DashMap;/' pseudoroot-core/src/state.rs
sed -i 's/ownership_map: RwLock<HashMap<String, FileOwnership>>/ownership_map: DashMap<String, FileOwnership>/' pseudoroot-core/src/state.rs
sed -i 's/ownership_map: RwLock::new(HashMap::new())/ownership_map: DashMap::new()/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.write().unwrap().insert(path, ownership);/self.ownership_map.insert(path, ownership);/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.read().unwrap().get(path).copied()/self.ownership_map.get(path).map(|entry| *entry.value())/' pseudoroot-core/src/state.rs
sed -i 's/self.ownership_map.write().unwrap().remove(path)/self.ownership_map.remove(path).map(|(_, v)| v)/' pseudoroot-core/src/state.rs

echo "======================================"
echo "Comparison Complete!"
echo "======================================"
echo ""
echo "Note: Results may vary between runs. For accurate comparison,"
echo "run each map implementation multiple times and average the results."