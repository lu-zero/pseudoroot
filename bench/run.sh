#!/usr/bin/env bash
# Reproduces issue #7 (from fakeroost, the ptrace-based sibling project): a
# single-threaded supervisor serializes stat() across the whole traced tree,
# so throughput hits a fixed ceiling and effective parallelism collapses no
# matter how many cores the workload is given. pseudoroot's LD_PRELOAD
# design has no such supervisor, so this is mostly a comparison baseline.
#
# Runs the stat-loop helper native and under pseudoroot, fakeroost (if built
# as a sibling checkout), and fakeroot, sweeping the worker count up to the
# core count, and prints a rate table + speedup curve.
#
#   bench/run.sh [n_calls_native] [n_calls_fake]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

n_stat_native="${1:-500000}"  # per-worker, native: large for stable timing
n_stat_fake="${2:-20000}"       # per-worker, wrapped tools: small so matrix finishes
cores="$(nproc)"
helper_manifest="$root/bench/stat-loop/Cargo.toml"
# bench/stat-loop is a workspace member here (unlike in fakeroost, where it's
# standalone), so it builds into the shared top-level target/, not its own.
helper="$root/target/release/stat-loop"
pseudoroot="$root/target/release/pseudoroot"
fakeroost="$root/../fakeroost/target/release/fakeroost"

workers=()
w=1
while (( w <= cores )); do workers+=("$w"); w=$(( w * 2 )); done
if [[ "${workers[-1]}" != "$cores" ]]; then workers+=("$cores"); fi

if [[ ! -x "$helper" ]] || [[ "$helper_manifest" -nt "$helper" ]]; then
    echo "# building stat-loop helper..." >&2
    cargo build --release --manifest-path "$helper_manifest"
fi
if [[ ! -x "$pseudoroot" ]]; then
    echo "# building pseudoroot..." >&2
    cargo build --release
fi

have_fakeroost=0
if [[ -x "$fakeroost" ]]; then
    have_fakeroost=1
else
    echo "# (fakeroost not built at $fakeroost; skipping that column)" >&2
fi

have_fakeroot=0
if command -v fakeroot >/dev/null 2>&1; then
    have_fakeroot=1
else
    echo "# (fakeroot not installed; skipping that column)" >&2
fi

# A directory of distinct files so a native run actually parallelizes.
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT
for ((i = 0; i < 512; i++)); do echo "$i" > "$workdir/f$i"; done

rate() { # <label> <workers>  -> prints rate
    case "$1" in
        native)     "$helper"                     "$n_stat_native" "$2" "$workdir" 2>&1 >/dev/null ;;
        pseudoroot) "$pseudoroot" run --           "$helper" "$n_stat_fake" "$2" "$workdir" 2>&1 >/dev/null ;;
        fakeroost)  "$fakeroost"                   "$helper" "$n_stat_fake" "$2" "$workdir" 2>&1 >/dev/null ;;
        fakeroot)   fakeroot --                    "$helper" "$n_stat_fake" "$2" "$workdir" 2>&1 >/dev/null ;;
    esac | sed -n 's/.*rate=\([0-9.]*\).*/\1/p'
}

fetch() { # <label> <workers>  -> rate (empty when tool unavailable)
    case "$1" in
        fakeroost) [[ "$have_fakeroost" -eq 1 ]] || { printf ''; return; } ;;
        fakeroot)  [[ "$have_fakeroot" -eq 1 ]] || { printf ''; return; } ;;
    esac
    rate "$1" "$2"
}

declare -a Rn Rp Rf Rk
base_n="" base_p="" base_f="" base_k=""
for i in "${!workers[@]}"; do
    nw="${workers[$i]}"
    Rn[$i]="$(rate native "$nw")"
    Rp[$i]="$(rate pseudoroot "$nw")"
    Rf[$i]="$(fetch fakeroost "$nw")"
    Rk[$i]="$(fetch fakeroot "$nw")"
    [[ -z "$base_n" ]] && base_n="${Rn[$i]}"
    [[ -z "$base_p" ]] && base_p="${Rp[$i]}"
    [[ -z "$base_f" && -n "${Rf[$i]}" ]] && base_f="${Rf[$i]}"
    [[ -z "$base_k" && -n "${Rk[$i]}" ]] && base_k="${Rk[$i]}"
done

echo "# pseudoroot serialization benchmark (fakeroost issue #7)"
echo "# n_calls_per_worker: native=$n_stat_native fake=$n_stat_fake  cores=$cores"
echo "#"
printf '%9s %16s %18s' 'workers' 'rate_native/s' 'rate_pseudoroot/s'
if [[ "$have_fakeroost" -eq 1 ]]; then printf ' %18s' 'rate_fakeroost/s'; fi
if [[ "$have_fakeroot" -eq 1 ]]; then printf ' %16s' 'rate_fakeroot/s'; fi
printf '\n'
for i in "${!workers[@]}"; do
    printf '%9s %16s %18s' "${workers[$i]}" "${Rn[$i]}" "${Rp[$i]}"
    if [[ "$have_fakeroost" -eq 1 ]]; then printf ' %18s' "${Rf[$i]}"; fi
    if [[ "$have_fakeroot" -eq 1 ]]; then printf ' %16s' "${Rk[$i]}"; fi
    printf '\n'
done

echo "#"
echo "# effective parallelism (rate_w / rate_w1):"
printf '%9s %16s %18s' 'workers' 'native_x' 'pseudoroot_x'
if [[ "$have_fakeroost" -eq 1 ]]; then printf ' %18s' 'fakeroost_x'; fi
if [[ "$have_fakeroot" -eq 1 ]]; then printf ' %16s' 'fakeroot_x'; fi
printf '\n'
for i in "${!workers[@]}"; do
    sn="$(awk -v a="${Rn[$i]}" -v b="$base_n" 'BEGIN{printf "%.1f", a/b}')"
    sp="$(awk -v a="${Rp[$i]}" -v b="$base_p" 'BEGIN{printf "%.2f", a/b}')"
    printf '%9s %16s %18s' "${workers[$i]}" "$sn" "$sp"
    if [[ "$have_fakeroost" -eq 1 ]]; then
        sf="$(awk -v a="${Rf[$i]}" -v b="$base_f" 'BEGIN{printf "%.2f", a/b}')"
        printf ' %18s' "$sf"
    fi
    if [[ "$have_fakeroot" -eq 1 ]]; then
        sk="$(awk -v a="${Rk[$i]}" -v b="$base_k" 'BEGIN{printf "%.2f", a/b}')"
        printf ' %16s' "$sk"
    fi
    printf '\n'
done
