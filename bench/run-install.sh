#!/usr/bin/env bash
# Benchmark offset install in the usual packaging pattern:
#
#   make -j N all                              # native, unprivileged build
#   sudo|fakeroot|pseudoroot|fakeroost -- \
#       make -j N package DESTDIR=… TARBALL=…  # install + tar archive
#
# Simulates `DESTDIR` staging where compile runs as the real user and only the
# install/package step needs root (chown, mknod, tar --numeric-owner stat walk).
# Directly exercises the SHM session inode map's chown/mknod/removal paths.
#
# Mode notes (package step = install + tar):
#   native (sudo)          — real root baseline (full install + archive)
#   pseudoroot              — LD_PRELOAD session (SHM map by default, one per invocation)
#   pseudoroot_daemon       — pseudoroot run --daemon with a shared pdrd
#   fakeroost (optional)    — supervisor/session (USER_NOTIF stat/chown + ptrace mknod),
#                             when built as a sibling checkout
#   fakeroot (optional)     — LD_PRELOAD + per-session faked daemon
#
# Job levels are powers of two up to nproc (32, 64, …) to explore contention.
#
#   bench/run-install.sh [n_files]
set -euo pipefail

WALL_LAST_EC=0
INSTALL_LAST_EC=0
INSTALL_LAST_FILES=0
INSTALL_LAST_TIME=0
INSTALL_LAST_TAR=0

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Portable tool selection. The workload needs GNU install/mknod/tar semantics
# (`install -o`, `tar --numeric-owner`) and, on macOS, unsigned binaries: SIP
# strips DYLD_INSERT_LIBRARIES from the system `/usr/bin` tools so pseudoroot
# can't fake them. Point everything at Homebrew's g-prefixed GNU tools via a
# shim dir, use `gmake`, and force recipes to run under an unsigned shell so
# the insert survives into install/mknod/tar.
MAKE=make
pkg_extra_args=()
if [[ "$(uname -s)" == "Darwin" ]]; then
    MAKE=gmake
    shimdir="$(mktemp -d)"
    for pair in install:ginstall tar:gtar mknod:gmknod chown:gchown \
                seq:gseq stat:gstat date:gdate; do
        target="/opt/homebrew/bin/${pair#*:}"
        if [[ ! -x "$target" ]]; then
            echo "# macOS needs Homebrew GNU tools: brew install coreutils gnu-tar make" >&2
            exit 1
        fi
        ln -sf "$target" "$shimdir/${pair%:*}"
    done
    export PATH="$shimdir:$PATH"
    pkg_extra_args=(SHELL=/opt/homebrew/bin/bash)
fi

n_files="${1:-200}"
cores="$(nproc)"
installer_uid="$(id -u)"
installer_gid="$(id -g)"
install_mk="$root/bench/install.mk"
pseudoroot="$root/target/release/pseudoroot"
pdrd="$root/target/release/pdrd"
fakeroost="$root/../fakeroost/target/release/fakeroost"
pdrd_socket=""
pdrd_pid=""

jobs=()
j=1
while (( j <= cores )); do jobs+=("$j"); j=$(( j * 2 )); done
if [[ "${jobs[-1]}" != "$cores" ]]; then jobs+=("$cores"); fi

if [[ ! -x "$pseudoroot" ]]; then
    echo "# building pseudoroot..." >&2
    cargo build --release
fi

have_fakeroot=0
if command -v fakeroot >/dev/null 2>&1; then
    have_fakeroot=1
else
    echo "# (fakeroot not installed; skipping that column)" >&2
fi

have_fakeroost=0
if [[ -x "$fakeroost" ]]; then
    have_fakeroost=1
else
    echo "# (fakeroost not built at $fakeroost; skipping that column)" >&2
fi

have_pdrd=0
if [[ -x "$pdrd" ]]; then
    have_pdrd=1
elif [[ -x "$root/target/release/pseudoroot-daemon" ]]; then
    pdrd="$root/target/release/pseudoroot-daemon"
    have_pdrd=1
else
    echo "# (pdrd not built; skipping pseudoroot_daemon column)" >&2
fi

have_sudo=0
if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    have_sudo=1
else
    echo "# (passwordless sudo not available; native column uses unprivileged install)" >&2
fi

workdir="$(mktemp -d)"
destdir="$workdir/stage"
tarball="$workdir/stage.tar"

stop_pdrd() {
    if [[ -n "${pdrd_pid:-}" ]]; then
        kill "$pdrd_pid" 2>/dev/null || true
        wait "$pdrd_pid" 2>/dev/null || true
        pdrd_pid=""
    fi
    if [[ -n "${pdrd_socket:-}" ]]; then
        rm -f "$pdrd_socket"
        pdrd_socket=""
    fi
}

start_pdrd() {
    stop_pdrd
    pdrd_socket="$workdir/pseudoroot.sock"
    "$pdrd" --socket-path "$pdrd_socket" --uid 0 --gid 0 >/dev/null 2>&1 &
    pdrd_pid=$!
    local i
    for ((i = 0; i < 50; i++)); do
        if [[ -S "$pdrd_socket" ]]; then
            return 0
        fi
        sleep 0.02
    done
    echo "# warning: pdrd socket not ready" >&2
    return 1
}

trap 'stop_pdrd; rm -rf "$workdir" "${shimdir:-}"' EXIT
mkdir -p "$workdir/build"

gen_sources() {
    local i
    for ((i = 0; i < n_files; i++)); do
        cat > "$workdir/build/app-$i.c" <<'EOF'
#include <stdio.h>
int app_main(void) { return printf("app\n"); }
int main(void) { return app_main(); }
EOF
        cat > "$workdir/build/lib-$i.c" <<'EOF'
int lib_fn(void) { return 42; }
EOF
    done
}

scrub_build() {
    find "$workdir/build" -maxdepth 1 \( -name 'app-*' ! -name '*.c' -o -name 'lib-*' ! -name '*.c' \) \
        -exec rm -rf {} + 2>/dev/null || true
}

scrub_install() {
    rm -f "$tarball" 2>/dev/null || true
    if [[ ! -d "$destdir" ]]; then
        return 0
    fi
    # sudo/native leaves real root-owned nodes; fake-root wrappers keep disk uid.
    if [[ "$have_sudo" -eq 1 ]]; then
        sudo rm -rf "$destdir" 2>/dev/null || true
    fi
    rm -rf "$destdir" 2>/dev/null || true
}

scrub_all() {
    scrub_build
    scrub_install
}

expected_nodes() {
    # 4 install targets + 1 mknod device per id.
    echo $(( n_files * 5 ))
}

wall() {
    local t0 t1 ec
    t0=$(date +%s.%N)
    set +e
    "$@" >/dev/null 2>&1
    ec=$?
    set -e
    t1=$(date +%s.%N)
    WALL_LAST_EC=$ec
    awk -v a="$t0" -v b="$t1" 'BEGIN{printf "%.2f", b-a}'
}

native_build() {
    local j=$1
    scrub_build
    wall "$MAKE" -f "$install_mk" -C "$workdir" N="$n_files" -j"$j" all
}

run_package() {
    local label=$1
    local j=$2
    local -a wrap=()
    case "$label" in
        native)
            if [[ "$have_sudo" -eq 1 ]]; then
                wrap=(sudo -E)
            fi
            ;;
        pseudoroot) wrap=("$pseudoroot" run --) ;;
        pseudoroot_daemon)
            start_pdrd || return 1
            wrap=("$pseudoroot" run --daemon --socket-path "$pdrd_socket" --)
            ;;
        fakeroost) wrap=("$fakeroost") ;;
        fakeroot) wrap=(fakeroot --) ;;
        *) echo "unknown label: $label" >&2; return 1 ;;
    esac
    scrub_install
    INSTALL_LAST_FILES=0
    INSTALL_LAST_TAR=0
    INSTALL_LAST_EC=0
    INSTALL_LAST_TIME=$(wall "${wrap[@]}" "$MAKE" -f "$install_mk" -C "$workdir" N="$n_files" \
        DESTDIR="$destdir" TARBALL="$tarball" prefix=/usr \
        INSTALLER_UID="$installer_uid" INSTALLER_GID="$installer_gid" \
        "${pkg_extra_args[@]}" -j"$j" package)
    INSTALL_LAST_EC=${WALL_LAST_EC:-0}
    INSTALL_LAST_FILES=$(find "$destdir" -mindepth 1 ! -type d 2>/dev/null | wc -l)
    if [[ -f "$tarball" ]]; then
        INSTALL_LAST_TAR=1
    fi
}

install_ok() {
    local expected=$1
    [[ "$INSTALL_LAST_EC" -eq 0 && "$INSTALL_LAST_FILES" -ge "$expected" && "$INSTALL_LAST_TAR" -eq 1 ]]
}

verify_tar_owners() {
    local -a wrap=("$@")
    local listing
    listing=$("${wrap[@]}" tar --numeric-owner -tvf "$tarball" 2>/dev/null)
    [[ "$listing" == *"0/0"* ]] && [[ "$listing" == *"${installer_uid}/${installer_gid}"* ]]
}

package_wrapper_for_label() {
    local label=$1
    case "$label" in
        native)
            if [[ "$have_sudo" -eq 1 ]]; then
                printf '%s\0' sudo -E
            fi
            ;;
        pseudoroot) printf '%s\0%s\0%s\0' "$pseudoroot" run -- ;;
        pseudoroot_daemon)
            if [[ ! -S "${pdrd_socket:-}" ]]; then
                return 1
            fi
            printf '%s\0%s\0%s\0%s\0%s\0%s\0' "$pseudoroot" run --daemon --socket-path "$pdrd_socket" --
            ;;
        fakeroost) printf '%s\0' "$fakeroost" ;;
        fakeroot) printf '%s\0%s\0' fakeroot -- ;;
        *) return 1 ;;
    esac
}

report_tar_owners() {
    local label=$1
    local package_label=$2
    run_package "$package_label" 1
    if ! install_ok "$expected"; then
        echo "# WARNING: tar listing ($label) package step failed" >&2
        return 1
    fi
    local -a wrap=()
    mapfile -d '' -t wrap < <(package_wrapper_for_label "$package_label") || true
    if [[ ${#wrap[@]} -eq 0 ]]; then
        echo "# WARNING: tar listing ($label) wrapper unavailable" >&2
        return 1
    fi
    if verify_tar_owners "${wrap[@]}"; then
        echo "# tar listing check ($label): root 0/0 + installer ${installer_uid}/${installer_gid} OK"
    else
        echo "# WARNING: tar listing ($label) missing expected mixed ownership" >&2
    fi
}

gen_sources

echo "# pseudoroot offset-install benchmark (build native, package wrapped)"
echo "# n_files=$n_files  DESTDIR=<tmpdir>/stage  TARBALL=<tmpdir>/stage.tar  prefix=/usr  cores=$cores"
echo "# build: always \`make -jJ all\` (unprivileged/native)"
echo "# package: \`make -jJ package\` = install (mixed root/installer ownership + mknod) + tar"
echo "# ownership: bin/sbin/dev → 0:0; lib/share → \$(id -u):\$(id -g)"
if [[ "$have_sudo" -eq 1 ]]; then
    echo "# native column: sudo (real root baseline, comparable file count)"
else
    echo "# native column: unprivileged (partial install — no sudo)"
fi
echo "#"
printf '%9s %14s' 'jobs' 'build_native'
printf ' %14s' 'native_sudo'
printf ' %14s' 'pseudo'
if [[ "$have_pdrd" -eq 1 ]]; then
    printf ' %14s' 'pdrd'
fi
if [[ "$have_fakeroost" -eq 1 ]]; then
    printf ' %14s' 'fakeroost'
fi
if [[ "$have_fakeroot" -eq 1 ]]; then
    printf ' %14s' 'fakeroot'
fi
printf '\n'

expected=$(expected_nodes)
last_count=0
for j in "${jobs[@]}"; do
    nb=$(native_build "$j")
    run_package native "$j"
    ni=$INSTALL_LAST_TIME
    printf '%9s %14s %14s' "$j" "$nb" "$ni"
    if ! install_ok "$expected"; then
        printf '*'
    fi
    run_package pseudoroot "$j"
    printf ' %14s' "$INSTALL_LAST_TIME"
    if ! install_ok "$expected"; then
        printf '!'
    fi
    if [[ "$have_pdrd" -eq 1 ]]; then
        run_package pseudoroot_daemon "$j"
        printf ' %14s' "$INSTALL_LAST_TIME"
        if ! install_ok "$expected"; then
            printf '!'
        fi
    fi
    if [[ "$have_fakeroost" -eq 1 ]]; then
        run_package fakeroost "$j"
        printf ' %14s' "$INSTALL_LAST_TIME"
        if ! install_ok "$expected"; then
            printf '!'
        fi
    fi
    if [[ "$have_fakeroot" -eq 1 ]]; then
        run_package fakeroot "$j"
        printf ' %14s' "$INSTALL_LAST_TIME"
        if ! install_ok "$expected"; then
            printf '!'
        fi
    fi
    last_count=$INSTALL_LAST_FILES
    printf '\n'
done

echo "#"
echo "# expected staged nodes: $expected (4 install + 1 mknod per id)"
echo "# last wrapped node count: $last_count  tarball: required"
echo "# suffix * / ! = package did not complete (nodes or tar missing)"
if [[ "$last_count" -ge "$expected" ]]; then
    echo "# verifying tar mixed ownership (one package run per wrapper, jobs=1)"
    if [[ "$have_sudo" -eq 1 ]]; then
        report_tar_owners native_sudo native
    fi
    report_tar_owners pseudo pseudoroot
    if [[ "$have_pdrd" -eq 1 ]]; then
        report_tar_owners pdrd pseudoroot_daemon
    fi
    if [[ "$have_fakeroost" -eq 1 ]]; then
        report_tar_owners fakeroost fakeroost
    fi
    if [[ "$have_fakeroot" -eq 1 ]]; then
        report_tar_owners fakeroot fakeroot
    fi
fi
