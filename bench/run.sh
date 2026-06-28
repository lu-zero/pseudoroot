#!/usr/bin/env bash
# Benchmark script for pseudoroot
#
# Runs the stat-loop helper native and under pseudoroot, sweeping the worker
# count up to the core count, and prints a rate table + speedup curve.
#
#   bench/run.sh [n_calls_native] [n_calls_pseudoroot]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

n_stat_native="${1:-500000}"  # per-worker, native: large for stable timing
n_stat_fake="${2:-20000}"     # per-worker, pseudoroot: small so the matrix finishes
cores="$(nproc)"
helper_manifest="$root/bench/stat-loop/Cargo.toml"
helper="$root/target/release/stat-loop"
target="$root/target/release/pseudoroot"

workers=()
w=1
while (( w <= cores )); do workers+=("$w"); w=$(( w * 2 )); done
if [[ "${workers[-1]}" != "$cores" ]]; then workers+=("$cores"); fi

if [[ ! -x "$helper" ]] || [[ "$helper_manifest" -nt "$helper" ]]; then
    echo "# building stat-loop helper..." >&2
    cargo build --release --manifest-path "$helper_manifest"
fi
if [[ ! -x "$target" ]]; then
    echo "# building pseudoroot..." >&2
    cargo build --release
fi

# A directory of distinct files so a native run actually parallelizes.
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
for ((i = 0; i < 512; i++)); do echo "$i" > "$workdir/f$i"; done

rate() { # <label> <workers>  -> prints rate
    case "$1" in
        native)    "$helper"            "$n_stat_native" "$2" "$workdir" 2>&1 >/dev/null ;;
        pseudoroot) "$target"            "$helper" "$n_stat_fake" "$2" "$workdir" 2>&1 >/dev/null ;;
    esac | sed -n 's/.*rate=\([0-9.]*\).*/\1/p'
}

declare -a Rn Rf
base_n="" base_f=""
for i in "${!workers[@]}"; do
    nw="${workers[$i]}"
    Rn[$i]="$(rate native "$nw")"
    Rf[$i]="$(rate pseudoroot "$nw")"
    [[ -z "$base_n" ]] && base_n="${Rn[$i]}"
    [[ -z "$base_f" ]] && base_f="${Rf[$i]}"
done

echo "# pseudoroot benchmark"
echo "# n_calls_per_worker: native=$n_stat_native pseudoroot=$n_stat_fake  cores=$cores"
echo "#"
printf '%9s %16s %18s\n' 'workers' 'rate_native/s' 'rate_pseudoroot/s'
for i in "${!workers[@]}"; do
    printf '%9s %16s %18s\n' "${workers[$i]}" "${Rn[$i]}" "${Rf[$i]}"
done

echo "#"
echo "# effective parallelism (rate_w / rate_w1):"
printf '%9s %16s %18s\n' 'workers' 'native_x' 'pseudoroot_x'
for i in "${!workers[@]}"; do
    sn="$(awk -v a="${Rn[$i]}" -v b="$base_n" 'BEGIN{printf "%.1f", a/b}')"
    sf="$(awk -v a="${Rf[$i]}" -v b="$base_f" 'BEGIN{printf "%.2f", a/b}')"
    printf '%9s %16s %18s\n' "${workers[$i]}" "$sn" "$sf"
done
