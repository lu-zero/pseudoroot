# pseudoroot

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/pseudoroot/workflows/CI/badge.svg)](https://github.com/lu-zero/pseudoroot/actions?query=workflow:CI)
[![dependency status](https://deps.rs/repo/github/lu-zero/pseudoroot/status.svg)](https://deps.rs/repo/github/lu-zero/pseudoroot)

A Rust implementation of fakeroot using library interposition
(`LD_PRELOAD` on Linux, `DYLD_INSERT_LIBRARIES` on macOS). Commands run as
if they had root privileges without requiring real root access.

## Quick start

```bash
cargo install --path pseudoroot

# Implicit run (fakeroot-style, via the `pdr` short name)
pdr -- id

# Explicit subcommands (via the `pseudoroot` main name)
pseudoroot run -- id
pseudoroot --uid 1000 --gid 1000 -- id
```

Short binary names: `pdr` (CLI) and `pdrd` (daemon), alongside
`pseudoroot` and `pseudoroot-daemon`.

## CLI options

| Option | Description | Default |
|--------|-------------|---------|
| `--uid <UID>` | Fake UID | 0 |
| `--gid <GID>` | Fake GID | 0 |
| `--daemon` | Attach to an existing `pdrd` instead of a per-invocation session | off |
| `--socket-path <PATH>` | Daemon socket path | `/tmp/pseudoroot.sock` |
| `start` / `stop` / `status` | Manage the daemon | — |
| `print-library-path` | Print the interposed library path | — |

By default each invocation gets its own session, shared across `exec`
within it. To share state across separate invocations, use a daemon:

```bash
pdrd -s /tmp/pseudoroot.sock          # start daemon
pdr --daemon --socket-path /tmp/pseudoroot.sock -- make install
pseudoroot stop --socket-path /tmp/pseudoroot.sock
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
- `PSEUDOROOT_DAEMON_SOCKET` — attach to an existing `pdrd` (skips session auto-start)
- `PSEUDOROOT_STANDALONE` — per-process state only (no session)
- `PSEUDOROOT_SESSION_SHM` — set to `0` to use an in-process daemon thread instead of the shared-memory map for session mode
- `PSEUDOROOT_LIB` — override interposed library path (tests, custom installs)

## Documentation

- [Architecture](docs/architecture.md) — crate layout, inode-keyed state
  model, platform support, interposed syscall families.
- [Benchmarks](docs/benchmarks.md) — comparison against native,
  `fakeroot`, and `fakeroost`; on a 128-core machine pseudoroot sustains
  ~7.4M faked `stat()` calls/sec vs fakeroot's ~44.5K.

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

The shared library is at `target/{debug,release}/libpseudoroot_lib.so`
(Linux) or `.dylib` (macOS).

## License

Licensed under [MIT](LICENSE-MIT).

## Contributing

See [AGENTS.md](./AGENTS.md) for conventions (Conventional Commits, style,
MSRV checks).

## Author

Luca Barbato <lu_zero@gentoo.org>
