# pseudoroot

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/pseudoroot/workflows/CI/badge.svg)](https://github.com/lu-zero/pseudoroot/actions?query=workflow:CI)
[![dependency status](https://deps.rs/repo/github/lu-zero/pseudoroot/status.svg)](https://deps.rs/repo/github/lu-zero/pseudoroot)

A Rust implementation of fakeroot using library interposition (`LD_PRELOAD`). Commands run as if they had root privileges without requiring real root access.

## Quick start

```bash
cargo build --release
cargo install --path pseudoroot

# Implicit run (fakeroot-style, via `pdr` short name)
pdr -- id

# Explicit subcommands (via `pseudoroot` main name)
pseudoroot run -- id
pseudoroot --uid 1000 --gid 1000 -- id
```

Short binary names: `pdr` (CLI) and `pdrd` (daemon), alongside `pseudoroot` and `pseudoroot-daemon`.

## API (fakeroost-compatible)

Swap backends by changing the import only:

```rust
use pseudoroot::FakerootCommandExt;

fn main() {
    pseudoroot::init(); // required: handles session re-exec (no-op otherwise)
    std::process::Command::new("make")
        .arg("install")
        .env("PSEUDOROOT_UID", "0")
        .env("PSEUDOROOT_GID", "0")
        .fakeroot()
        .status()
        .unwrap();
}
```

Set `PSEUDOROOT_LIB` to override library discovery (tests, custom installs).

## CLI options

| Option | Description | Default |
|--------|-------------|---------|
| `--uid <UID>` | Fake UID | 0 |
| `--gid <GID>` | Fake GID | 0 |
| `--daemon` | Attach to an existing `pdrd` instead of a per-invocation session | off |
| `--socket-path <PATH>` | Daemon socket path | `/tmp/pseudoroot.sock` |
| `start` / `stop` / `status` | Manage the daemon | — |
| `print-library-path` | Print the interposed library path | — |

Daemon management:

```bash
pdrd -s /tmp/pseudoroot.sock          # start daemon
pdr --daemon --socket-path /tmp/pseudoroot.sock -- make install
pseudoroot stop --socket-path /tmp/pseudoroot.sock
```

## State model

Fake metadata is keyed by `(dev, ino)` inode identity — not paths — so renames, hard links, and concurrent writers stay consistent.

| Mode | Scope | When to use |
|------|-------|-------------|
| **Session** (default) | Shared-memory inode map (Linux) or an in-process IPC server thread (other platforms, or `PSEUDOROOT_SESSION_SHM=0`) per `.fakeroot()` / `pdr` invocation; shared across `exec` within that session | Package builds (`make install` → `tar`), API usage |
| **External daemon** (`--daemon` / `PSEUDOROOT_DAEMON_SOCKET`) | Shared across separate top-level invocations via Unix socket IPC | Long-lived `pdrd`, multiple sequential `pdr` calls |
| **Standalone** (`PSEUDOROOT_STANDALONE=1`) | In-memory per process only; inherited across `fork()` | Single-process tools, debugging |

Environment variables:

- `PSEUDOROOT_UID` — fake UID (default: 0)
- `PSEUDOROOT_GID` — fake GID (default: 0)
- `PSEUDOROOT_DAEMON_SOCKET` — attach to an existing `pdrd` (skips session auto-start)
- `PSEUDOROOT_STANDALONE` — per-process state only (no session `pdrd`)
- `PSEUDOROOT_SESSION_SHM` — set to `0`/`false` to use the in-process daemon thread instead of the shared-memory map for session mode (Linux only; ignored elsewhere)
- `PSEUDOROOT_LIB` — override interposed library path


Nothing is written to disk for ownership: `chown` records fake uid/gid in the inode table; `stat`/`statx` overlay the result. The real filesystem uid/gid is unchanged.

## Architecture

| Crate | Purpose |
|-------|---------|
| `pseudoroot-core` | Shared types, inode-keyed state, daemon IPC protocol |
| `pseudoroot-lib` | Interposed cdylib (`LD_PRELOAD`) |
| `pseudoroot-daemon` | Optional persistent state daemon (`pdrd`) |
| `pseudoroot` | CLI (`pseudoroot` / `pdr`) and API crate |
| `pseudoroot-tests` | Integration, CLI, and interposition tests |

**Platform support:** Linux is fully supported (all syscall families below,
default session mode backed by a shared-memory inode map). macOS interposes
via `DYLD_INSERT_LIBRARIES` and a `__DATA,__interpose` table, and covers
credentials, `stat`/`lstat`/`fstat`, `chown`/`lchown`/`fchown`,
`chmod`/`fchmod`, and `unlink`/`rmdir`/`rename`, with the same shared-memory
session mode as Linux (backed by `shm_open` rather than `memfd_create`). The
`*at`-suffixed family (`fstatat`, `fchmodat`, `fchownat`, `unlinkat`,
`renameat`, `renameat2`), `statx`, `mknod`/`mknodat`, and the `*xattr` family
are currently Linux-only — see [`todo/macos.md`](todo/macos.md) for the plan
to close that gap.

macOS System Integrity Protection strips `DYLD_INSERT_LIBRARIES` from
Apple-signed binaries (`/bin/sh`, `/usr/bin/id`, the system coreutils …), so
those cannot be faked; interposition only applies to binaries you build or
install yourself. The test suite exercises macOS through freshly built
helpers for exactly this reason. CI runs on Linux today, so macOS is
type-checked in CI (`cargo check --target x86_64-apple-darwin`) and the full
suite passes on real Apple hardware locally.

### Interposed syscall families

- **Credentials** — `getuid`, `setuid`, `setresuid`, `setfsuid`, `setgroups`, … (Linux + macOS); `capset` (Linux only)
- **Stat** — `stat`, `lstat`, `fstat` (Linux + macOS); `fstatat`, `statx` (Linux only; uid/gid/mode/rdev overlay)
- **Ownership** — `chown`, `lchown`, `fchown` (Linux + macOS); `fchownat` (Linux only; record only, skip real syscall)
- **Mode** — `chmod`, `fchmod` (Linux + macOS); `fchmodat` (Linux only; record fake mode, real syscall with EPERM zeroed)
- **Inode lifecycle** — `unlink`, `rmdir`, `rename` (Linux + macOS); `unlinkat`, `renameat`, `renameat2` (Linux only; drop stale inode entries)
- **Creation** — `mknod`, `mknodat` (Linux only; placeholder file + faked device metadata)
- **xattr** — all 12 `*xattr` syscalls (Linux only; fake `security.capability`, ACLs, etc.)

## Build and test

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The shared library is at `target/{debug,release}/libpseudoroot_lib.so` (Linux) or `.dylib` (macOS).

## Performance

`bench/run.sh`, `bench/run-install.sh`, and `bench/run-make.sh` (synced from
the [fakeroost](https://github.com/lu-zero/fakeroost) sibling project, whose
ptrace-based supervisor design pseudoroot's `LD_PRELOAD` approach avoids the
serialization bottleneck of) compare pseudoroot against native, classic
`fakeroot`, and `fakeroost` across a `stat()` sweep, a realistic
build/install/tar packaging workload, and a parallel-compile workload. See
[`bench/results/`](bench/results) for full output and system details; latest
run (128-core aarch64, default SHM session mode):

```
# bench/run.sh: stat() calls/sec, effective parallelism at 128 workers
native      42.5M/s   (26.5x)
pseudoroot   7.4M/s   ( 6.7x)
fakeroost  251.5K/s   ( 2.5x)  -- single-supervisor ceiling (issue #7)
fakeroot    44.5K/s   ( 0.7x)  -- gets slower under contention
```

`bench/run-install.sh` (build → install with mixed root/installer ownership
+ `mknod` → `tar`) completed correctly at every job level from 1 to 128,
with every `tar --numeric-owner` listing showing the right mixed ownership —
the benchmark that actually exercises the session inode map's chown/mknod/
removal paths under real concurrent load, not just a micro-benchmark.

See `bench/stat-loop` for the raw stat-loop harness.

## Comparison

| Feature | pseudoroot | fakeroot | fakeroost |
|---------|-----------|----------|-----------|
| Mechanism | `LD_PRELOAD` | `LD_PRELOAD` | ptrace supervisor |
| Platforms | Linux, macOS | Linux | Linux |
| xattr / setcap | Faked | Faked | Faked |
| mknod unprivileged | Placeholder + fake metadata | Yes | Yes |
| Multi-process state | SHM session (Linux + macOS) or daemon (`pdrd`) | `faked` | N/A (single run) |
| Rust API | `FakerootCommandExt` | C only | `FakerootCommandExt` |

## License

Licensed under [MIT](LICENSE-MIT).

## Contributing

See [AGENTS.md](./AGENTS.md) for conventions (Conventional Commits, style, MSRV checks).

## Author

Luca Barbato <lu_zero@gentoo.org>