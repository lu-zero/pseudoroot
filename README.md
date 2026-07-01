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
| **Session** (default) | Auto-starts a private `pdrd` per `.fakeroot()` / `pdr` invocation; shared across `exec` within that session | Package builds (`make install` → `tar`), API usage |
| **External daemon** (`--daemon` / `PSEUDOROOT_DAEMON_SOCKET`) | Shared across separate top-level invocations via Unix socket IPC | Long-lived `pdrd`, multiple sequential `pdr` calls |
| **Standalone** (`PSEUDOROOT_STANDALONE=1`) | In-memory per process only; inherited across `fork()` | Single-process tools, debugging |

Environment variables:

- `PSEUDOROOT_UID` — fake UID (default: 0)
- `PSEUDOROOT_GID` — fake GID (default: 0)
- `PSEUDOROOT_DAEMON_SOCKET` — attach to an existing `pdrd` (skips session auto-start)
- `PSEUDOROOT_STANDALONE` — per-process state only (no session `pdrd`)
- `PSEUDOROOT_LIB` — override interposed library path
- `PSEUDOROOT_DAEMON_BIN` — override `pdrd` discovery for session mode

Nothing is written to disk for ownership: `chown` records fake uid/gid in the inode table; `stat`/`statx` overlay the result. The real filesystem uid/gid is unchanged.

## Architecture

| Crate | Purpose |
|-------|---------|
| `pseudoroot-core` | Shared types, inode-keyed state, daemon IPC protocol |
| `pseudoroot-lib` | Interposed cdylib (`LD_PRELOAD`) |
| `pseudoroot-daemon` | Optional persistent state daemon (`pdrd`) |
| `pseudoroot` | CLI (`pseudoroot` / `pdr`) and API crate |
| `pseudoroot-tests` | Integration, CLI, and interposition tests |

**Platform support:** Linux is fully supported. macOS is supported for credential and stat interposition; Linux-only syscalls (`statx`, `capset`, `*xattr`, `mknod`) are gated accordingly.

### Interposed syscall families

- **Credentials** — `getuid`, `setuid`, `setresuid`, `setfsuid`, `setgroups`, `capset`, …
- **Stat** — `stat`, `lstat`, `fstat`, `fstatat`, `statx` (uid/gid/mode/rdev overlay)
- **Ownership** — `chown`, `lchown`, `fchown`, `fchownat` (record only, skip real syscall)
- **Mode** — `chmod`, `fchmod`, `fchmodat` (record fake mode, real syscall with EPERM zeroed)
- **Inode lifecycle** — `unlink`, `rename`, … (drop stale inode entries)
- **Creation** — `mknod`, `mknodat` (placeholder file + faked device metadata)
- **xattr** — all 12 `*xattr` syscalls (fake `security.capability`, ACLs, etc.)

## Build and test

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

The shared library is at `target/{debug,release}/libpseudoroot_lib.so` (Linux) or `.dylib` (macOS).

## Performance

Benchmark results (stat loop, standalone mode):

```
workers    rate_native/s  rate_pseudoroot/s
   1         1471097          847049
   8         5081757         2930461
```

Daemon mode adds ~3–4% IPC overhead but shares state across processes.

Compared to classic fakeroot (~97% overhead, poor threading), pseudoroot's library interposition with `DashMap` and atomic UID/GID achieves substantially lower overhead and better parallelism. See `bench/stat-loop` for the harness.

## Comparison

| Feature | pseudoroot | fakeroot | fakeroost |
|---------|-----------|----------|-----------|
| Mechanism | `LD_PRELOAD` | `LD_PRELOAD` | ptrace supervisor |
| Platforms | Linux, macOS | Linux | Linux |
| xattr / setcap | Faked | Faked | Faked |
| mknod unprivileged | Placeholder + fake metadata | Yes | Yes |
| Multi-process state | Daemon (`pdrd`) | `faked` | N/A (single run) |
| Rust API | `FakerootCommandExt` | C only | `FakerootCommandExt` |

## License

Licensed under [MIT](LICENSE-MIT).

## Contributing

See [AGENTS.md](./AGENTS.md) for conventions (Conventional Commits, style, MSRV checks).

## Author

Luca Barbato <lu_zero@gentoo.org>