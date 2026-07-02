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

Two things remain open:

## 1. macOS is missing a chunk of interposition surface

`pseudoroot-lib/src/lib.rs` gates `fstatat`, `statx`, `fchmodat`, `fchownat`,
`unlinkat`, `renameat`, `renameat2`, `mknod`, `mknodat`, and the entire xattr
family (`setxattr`/`getxattr`/`listxattr`/`removexattr` and friends) behind
`#[cfg(target_os = "linux")]`. Darwin has all of these syscalls (with
different signatures, already handled in `platform/macos.rs`'s `real_*`
wrappers), but nothing in `lib.rs` exposes them as interposition entries on
macOS. Net effect: on macOS, pseudoroot doesn't fake xattrs or mknod'd
device files at all, and `*at`-suffixed calls fall through to the real libc
symbol unfaked.

Decide whether this is intentional (macOS support was only ever meant to
cover the credential/basic-stat/basic-chmod surface) or a gap to close. If
closing it: un-gate the relevant wrapper fns, add matching entries to the
`interpose!` table in `lib.rs` (minding the Darwin signatures — e.g. the
xattr functions take extra `options`/`position` args), verify the
corresponding `ownership.rs` fake_* functions (currently dead weight on macOS
per the same cfg gating, kept quiet with a module-level `allow(dead_code)`)
behave correctly there, and add test coverage via freshly built helpers (SIP
blocks faking Apple-signed system tools).

## 2. No macOS CI job

`.github/workflows/ci.yml` only runs `ubuntu-latest`. That's *why* the 32
compile errors in `platform/macos.rs` went unnoticed for as long as they
did — nothing has ever actually built this crate for macOS in CI. Add a
`macos-latest` runner to at least the `test`/`clippy`/`fmt` jobs (or a
dedicated `cargo check --target x86_64-apple-darwin` job if full macOS
runners are too expensive to run on every push) so a regression here gets
caught automatically next time.
