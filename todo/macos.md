# macOS follow-ups

Found during the 2026-07-01 unslop pass. Updated 2026-07-02 after the first
real run on Apple hardware (arm64).

**Done since:** interposition actually works on macOS now. The library no
longer relies on the `LD_PRELOAD` symbol-export trick (a no-op under dyld's
two-level namespace) — it ships a `__DATA,__interpose` table (`interpose!`
macro in `pseudoroot-lib/src/lib.rs`) pairing each wrapper with its libc
replacee, keeps the libc names unexported on macOS (`no_mangle` is Linux-only
now), and calls libc directly from `platform/macos.rs` (dyld never interposes
the interposer itself). A ctor-ordering abort — dyld applies interposition
before running initializers, so libSystem's own bootstrap re-enters our hooks
before TLS exists — is guarded by a `CTOR_DONE` flag in `ownership.rs`.
Session mode also works: `ShmInodeMap` is backed by `shm_open`+`shm_unlink`
on macOS and is the default there just like Linux. The whole suite passes
locally (`cargo test` / `clippy -D warnings` / `fmt`).

The `*at`/mknod/xattr surface (item 1 below) is now closed too:
`fstatat`, `fchmodat`, `fchownat`, `unlinkat`, `renameat`, `mknod`,
`mknodat`, and the full xattr family are interposed on macOS (the xattr
hooks carry Darwin's extra `position`/`options` args and live in
`macos_xattr`). This is what let GNU coreutils (`ginstall`/`gchown`/
`gmknod`, which use `fchownat`/`mknodat`) run faked under pseudoroot, so all
three benchmarks (`bench/run*.sh`) now run on macOS against Homebrew's
unsigned g-tools. `statx` and `renameat2` stay Linux-only — Darwin has no
equivalent.

One thing remains open:

## 1. No macOS CI job

`.github/workflows/ci.yml` only runs `ubuntu-latest`. That's *why* the 32
compile errors in `platform/macos.rs` went unnoticed for as long as they
did — nothing has ever actually built this crate for macOS in CI. Add a
`macos-latest` runner to at least the `test`/`clippy`/`fmt` jobs (or a
dedicated `cargo check --target x86_64-apple-darwin` job if full macOS
runners are too expensive to run on every push) so a regression here gets
caught automatically next time.
