#!/usr/bin/env bash
# Reproduces issue #7 (from fakeroost) at workload scale: a parallel `make` of
# many independent tiny compiles. Each compile forks/execs `cc` and stats the
# libc headers, so a ptrace-based supervisor hammers a single thread; under
# pseudoroot's LD_PRELOAD design each process resolves its own dlsym'd real
# syscalls independently, so this mostly measures per-call hook overhead.
#
#   bench/run-make.sh [n_files]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Portable tools: this workload's stat storm must be faked, so on macOS `make`
# has to be an unsigned binary (SIP strips DYLD_INSERT_LIBRARIES from the
# system `/usr/bin/make`) running its recipes under an unsigned shell, and the
# timing needs GNU `date +%N`. Point at Homebrew's GNU tools via a shim dir.
MAKE=make
make_extra_args=()
if [[ "$(uname -s)" == "Darwin" ]]; then
    MAKE=gmake
    shimdir="$(mktemp -d)"
    for pair in date:gdate stat:gstat seq:gseq; do
        target="/opt/homebrew/bin/${pair#*:}"
        if [[ ! -x "$target" ]]; then
            echo "# macOS needs Homebrew GNU tools: brew install coreutils make" >&2
            exit 1
        fi
        ln -sf "$target" "$shimdir/${pair%:*}"
    done
    export PATH="$shimdir:$PATH"
    make_extra_args=(SHELL=/opt/homebrew/bin/bash)
fi

pseudoroot="$root/target/release/pseudoroot"
fakeroost="$root/../fakeroost/target/release/fakeroost"
[[ -x "$pseudoroot" ]] || cargo build --release

have_fakeroost=0
if [[ -x "$fakeroost" ]]; then
    have_fakeroost=1
else
    echo "# (fakeroost not built at $fakeroost; skipping that column)" >&2
fi

n_files="${1:-400}"
cores="$(nproc)"

jobs=(1)
j=4
while (( j <= cores )); do jobs+=("$j"); j=$(( j * 4 )); done
if [[ "${jobs[-1]}" != "$cores" ]]; then jobs+=("$cores"); fi

# Self-contained scratch dir (cleaned on exit).
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir" "${shimdir:-}"' EXIT
mkdir -p "$workdir/src"

gen() {
    rm -f "$workdir"/src/*.c "$workdir"/src/*.o
    for ((i = 0; i < n_files; i++)); do
        # Several system headers each, so every compile triggers a real
        # header/stat storm (the workload that bites under fakeroot).
        cat > "$workdir/src/t$i.c" <<'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <fcntl.h>
#include <errno.h>
#include <time.h>
int work(void) {
    char b[64];
    struct stat st;
    fstat(0, &st);
    snprintf(b, sizeof b, "%d", (int)st.st_size);
    return strlen(b) + errno + (int)time(0);
}
EOF
    done
}

wall() { # <command...> -> elapsed seconds, robust to sub-shell stderr quirks
    local t0 t1
    t0=$(date +%s.%N)
    "$@" >/dev/null 2>&1
    t1=$(date +%s.%N)
    awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.2f", b-a}'
}

fmt() { printf "%9s %14s %18s" "$1" "$2" "$3"; shift 3; printf " %18s" "$@"; printf '\n'; }

echo "# pseudoroot parallel-build benchmark (fakeroost issue #7)"
echo "# n_files=$n_files  cores=$cores  cc=$(command -v cc)"
echo "#"
if [[ "$have_fakeroost" -eq 1 ]]; then
    fmt "jobs" "wall_native/s" "wall_pseudoroot/s" "wall_fakeroost/s"
else
    printf "%9s %14s %18s\n" "jobs" "wall_native/s" "wall_pseudoroot/s"
fi

for j in "${jobs[@]}"; do
    gen
    tn="$(wall "$MAKE" -j"$j" -f "$root/bench/Makefile" -C "$workdir" N="$n_files" "${make_extra_args[@]}" all)"
    gen
    tp="$(wall "$pseudoroot" run -- "$MAKE" -j"$j" -f "$root/bench/Makefile" -C "$workdir" N="$n_files" "${make_extra_args[@]}" all)"
    if [[ "$have_fakeroost" -eq 1 ]]; then
        gen
        tf="$(wall "$fakeroost" "$MAKE" -j"$j" -f "$root/bench/Makefile" -C "$workdir" N="$n_files" "${make_extra_args[@]}" all)"
        fmt "$j" "$tn" "$tp" "$tf"
    else
        printf "%9s %14s %18s\n" "$j" "$tn" "$tp"
    fi
done
