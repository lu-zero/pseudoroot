# pseudoroot

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/pseudoroot/workflows/CI/badge.svg)](https://github.com/lu-zero/pseudoroot/actions?query=workflow:CI)
[![codecov](https://codecov.io/gh/lu-zero/pseudoroot/graph/badge.svg)](https://codecov.io/gh/lu-zero/pseudoroot)
[![Crates.io](https://img.shields.io/crates/v/pseudoroot.svg)](https://crates.io/crates/pseudoroot)
[![dependency status](https://deps.rs/repo/github/lu-zero/pseudoroot/status.svg)](https://deps.rs/repo/github/lu-zero/pseudoroot)
[![docs.rs](https://docs.rs/pseudoroot/badge.svg)](https://docs.rs/pseudoroot)

A Rust implementation of fakeroot using library interposition
(`LD_PRELOAD` on Linux, `DYLD_INSERT_LIBRARIES` on macOS). Commands run as
if they had root privileges without requiring real root access.

## Quick start

```bash
cargo install --path pseudoroot

# Implicit run (fakeroot-style)
pdr -- id

# Explicit subcommand form (equivalent)
pdr run -- id
pdr --uid 1000 --gid 1000 -- id
```

`pdr` is a single self-contained binary — the interposed library is embedded
at build time and extracted to a cache directory on first use, so no
`libpseudoroot_lib.so`/`.dylib` needs to exist anywhere on disk beforehand.
`pdr start` runs the optional persistent daemon in-process, so no separate
`pdrd` binary is needed for that either; a standalone `pdrd` is still
available (see [Build and test](#build-and-test)) as a dedicated daemon
process for external orchestration.

## CLI options

| Option | Description | Default |
|--------|-------------|---------|
| `--uid <UID>` | Fake UID | 0 |
| `--gid <GID>` | Fake GID | 0 |
| `--daemon` | Attach to an existing `pdrd` instead of a per-invocation session | off |
| `--socket-path <PATH>` | Daemon socket path | `/tmp/pseudoroot.sock` |
| `start` / `stop` / `status` | Manage the daemon | — |
| `print-library-path` | Print the interposed library path | — |
| `start --verbose` | Enable verbose daemon logging | off |
| `start --cleanup` | Clean up socket file on daemon exit | off |

By default each invocation gets its own session, shared across `exec`
within it. To share state across separate invocations, use a daemon:

```bash
pdr start --socket-path /tmp/pseudoroot.sock   # daemon runs in-process
pdr --daemon --socket-path /tmp/pseudoroot.sock -- make install
pdr stop --socket-path /tmp/pseudoroot.sock
```

## Rust API (fakeroost-compatible)

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

## Environment variables

- `PSEUDOROOT_UID` / `PSEUDOROOT_GID` — fake uid/gid (default: 0)
- `PSEUDOROOT_DAEMON_SOCKET` — attach to an existing daemon (skips session auto-start)
- `PSEUDOROOT_STANDALONE` — per-process state only (no session)
- `PSEUDOROOT_SESSION_SHM` — set to `0` to use an in-process daemon thread instead of the shared-memory map for session mode
- `PSEUDOROOT_LIB` — override the interposed library path, bypassing the
  embedded copy entirely (tests, custom installs, debugging a locally built
  `pseudoroot-lib`)

## Documentation

- [Architecture](docs/architecture.md) — crate and module map, state backends
  (SHM session, daemon, standalone), session lifecycle, IPC protocol, SHM
  layout, platform differences, and a guide for extending interposition.
- [Benchmarks](docs/benchmarks.md) — comparison against native,
  `fakeroot`, and `fakeroost`; on a 128-core Linux machine pseudoroot sustains
  ~7.4M faked `stat()` calls/sec vs fakeroot's ~44.5K, and on a MacBook Pro
  with M2 Max (12-core ARM64) ~899K calls/sec vs fakeroot's ~43K.

## Comparison

| Feature | pseudoroot | fakeroot | fakeroost |
|---------|-----------|----------|-----------|
| Mechanism | `LD_PRELOAD` | `LD_PRELOAD` | ptrace supervisor |
| Platforms | Linux, macOS | Linux | Linux |
| xattr / setcap | Faked | Faked | Faked |
| mknod unprivileged | Placeholder + fake metadata | Yes | Yes |
| Multi-process state | SHM session or daemon (`pdrd`) | `faked` | N/A (single run) |
| Rust API | `FakerootCommandExt` | C only | `FakerootCommandExt` |

## Build and test

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

`pdr` embeds the interposed library at build time: `pseudoroot/build.rs`
compiles the cdylib from the source bundled at `pseudoroot/interpose/` and
`include_bytes!`es it into the binary — so `cargo install pseudoroot` from
crates.io is self-contained, with no separate `.so`/`.dylib` to install.
For a standalone `pdrd` daemon binary, build the `pseudoroot-daemon` package
directly: `cargo build -p pseudoroot-daemon`.

## License

Licensed under [MIT](LICENSE-MIT).

## Contributing

See [AGENTS.md](./AGENTS.md) for conventions (Conventional Commits, style,
MSRV checks).

## Author

Luca Barbato <lu_zero@gentoo.org>
