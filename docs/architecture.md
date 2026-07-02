# Architecture

## Crates

| Crate | Purpose |
|-------|---------|
| `pseudoroot-core` | Shared types, inode-keyed state, daemon IPC protocol |
| `pseudoroot-lib` | Interposed cdylib (`LD_PRELOAD` / `DYLD_INSERT_LIBRARIES`) |
| `pseudoroot-daemon` | Optional persistent state daemon (`pdrd`) |
| `pseudoroot` | CLI (`pseudoroot` / `pdr`) and API crate |
| `pseudoroot-tests` | Integration, CLI, and interposition tests |

## State model

Fake metadata is keyed by `(dev, ino)` inode identity — not paths — so
renames, hard links, and concurrent writers stay consistent.

Nothing is written to disk for ownership: `chown` records fake uid/gid in
the inode table; `stat`/`statx` overlay the result. The real filesystem
uid/gid is unchanged.

| Mode | Scope | When to use |
|------|-------|-------------|
| **Session** (default) | Shared-memory inode map, or an in-process IPC server thread with `PSEUDOROOT_SESSION_SHM=0`, per `.fakeroot()` / `pdr` invocation; shared across `exec` within that session | Package builds (`make install` → `tar`), API usage |
| **External daemon** (`--daemon` / `PSEUDOROOT_DAEMON_SOCKET`) | Shared across separate top-level invocations via Unix socket IPC | Long-lived `pdrd`, multiple sequential `pdr` calls |
| **Standalone** (`PSEUDOROOT_STANDALONE=1`) | In-memory per process only; inherited across `fork()` | Single-process tools, debugging |

## Platform support

Linux and macOS are both fully supported, with the same shared-memory
session mode by default (backed by `memfd_create` on Linux and `shm_open`
on macOS). Linux interposes via `LD_PRELOAD`; macOS via
`DYLD_INSERT_LIBRARIES` and a `__DATA,__interpose` table. Every syscall
family below is faked on both, except a few with no Darwin equivalent
(`statx`, `renameat2`, `capset`) which stay Linux-only.

macOS System Integrity Protection strips `DYLD_INSERT_LIBRARIES` from
Apple-signed binaries (`/bin/sh`, `/usr/bin/id`, the system coreutils and
`tar` under `/usr/bin`, …), so those cannot be faked; interposition only
applies to binaries you build or install yourself, including Homebrew's
unsigned GNU tools (`ginstall`, `gmknod`, `gtar`, `/opt/homebrew/bin/bash`,
…). The test suite and the macOS benchmark paths use freshly built or
Homebrew binaries for exactly this reason.

CI runs the full test suite and clippy on both `ubuntu-latest` and
`macos-latest`.

## Interposed syscall families

- **Credentials** — `getuid`, `setuid`, `setresuid`, `setfsuid`, `setgroups`, … (Linux + macOS); `capset` (Linux only)
- **Stat** — `stat`, `lstat`, `fstat`, `fstatat` (Linux + macOS; uid/gid/mode/rdev overlay); `statx` (Linux only)
- **Ownership** — `chown`, `lchown`, `fchown`, `fchownat` (Linux + macOS)
- **Mode** — `chmod`, `fchmod`, `fchmodat` (Linux + macOS; record fake mode, real syscall with EPERM zeroed)
- **Inode lifecycle** — `unlink`, `rmdir`, `rename`, `unlinkat`, `renameat` (Linux + macOS; drop stale inode entries); `renameat2` (Linux only)
- **Creation** — `mknod`, `mknodat` (Linux + macOS; placeholder file + faked device metadata)
- **xattr** — the `*xattr` family (Linux + macOS; fake `security.capability`, ACLs, etc.). macOS carries Darwin's extra `position`/`options` args; Linux also has the `l*`-prefixed variants.
