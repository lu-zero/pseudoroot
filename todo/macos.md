# macOS follow-ups

Found during the 2026-07-01 unslop pass. `pseudoroot-lib` now compiles
cleanly for macOS (verified via `cargo check --target x86_64-apple-darwin`
cross type-check — there's no macOS runtime available in this environment,
so it still wants a real run on a Mac before fully trusting it), but two
things remain open:

## 1. macOS is missing a chunk of interposition surface, not just broken

`pseudoroot-lib/src/lib.rs` gates `fstatat`, `statx`, `fchmodat`, `fchownat`,
`unlinkat`, `renameat`, `renameat2`, `mknod`, `mknodat`, and the entire xattr
family (`setxattr`/`getxattr`/`listxattr`/`removexattr` and friends) behind
`#[cfg(target_os = "linux")]`. Darwin has all of these syscalls (with
different signatures, already handled in `platform/macos.rs`'s `real_*`
wrappers), but nothing in `lib.rs` exposes them as interposition symbols on
macOS. Net effect: on macOS, pseudoroot doesn't fake xattrs or mknod'd
device files at all, and `*at`-suffixed calls fall through to the real glibc
symbol unfaked.

Decide whether this is intentional (macOS support was only ever meant to
cover the credential/basic-stat/basic-chmod surface) or a gap to close. If
closing it: drop the `#[cfg(target_os = "linux")]` from the relevant
`lib.rs` exports, verify the corresponding `ownership.rs` fake_* functions
(currently dead weight on macOS per the same cfg gating) compile and behave
correctly there, and add test coverage.

## 2. No macOS CI job

`.github/workflows/ci.yml` only runs `ubuntu-latest`. That's *why* the 32
compile errors in `platform/macos.rs` went unnoticed for as long as they
did — nothing has ever actually built this crate for macOS in CI. Add a
`macos-latest` runner to at least the `test`/`clippy`/`fmt` jobs (or a
dedicated `cargo check --target x86_64-apple-darwin` job if full macOS
runners are too expensive to run on every push) so a regression here gets
caught automatically next time.
