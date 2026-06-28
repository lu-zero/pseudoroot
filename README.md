# pseudoroot

[![LICENSE](https://img.shields.io/badge/license-MIT%20OR%20Apache-2.0-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/pseudoroot/workflows/CI/badge.svg)](https://github.com/lu-zero/pseudoroot/actions?query=workflow:CI)
[![dependency status](https://deps.rs/repo/github/lu-zero/pseudoroot/status.svg)](https://deps.rs/repo/github/lu-zero/pseudoroot)

A Rust implementation of the fakeroot functionality using library interposition. This allows running commands that think they have root privileges without actually requiring root access.

## The `pseudoroot` binary

`pseudoroot` is a command-line tool that runs a specified command with fake root privileges by preloading the pseudoroot library.

### Usage

```bash
pseudoroot [OPTIONS] -- <command> [args...]
```

This sets the appropriate environment variable (`LD_PRELOAD` on Linux, `DYLD_INSERT_LIBRARIES` on macOS) and executes the given command with the pseudoroot library preloaded.

### Examples

```bash
# Run a simple command with fake root (UID=0, GID=0)
pseudoroot -- id

# Run with a specific fake UID and GID
pseudoroot --uid 1000 --gid 1000 -- id

# Build a package with fake root
pseudoroot -- emerge --ask package

# Print the library path
pseudoroot --print-library-path
```

### Options

| Option | Description | Default |
|--------|-------------|---------|
| `--print-library-path` | Print the path to the pseudoroot library and exit | |
| `--uid <UID>` | Fake UID to use | 0 (root) |
| `--gid <GID>` | Fake GID to use | 0 (root) |
| `--help` | Show help message | |
| `--version` | Show version information | |

---

## Architecture

The project uses a Cargo workspace with the following crates:

| Crate | Purpose | Status |
|-------|---------|--------|
| `pseudoroot-core` | Shared types, state management, and IPC protocol | Implemented |
| `pseudoroot-lib` | The interposed shared library (cdylib) | Implemented |
| `pseudoroot-daemon` | Optional daemon for persistent state | Stub |
| `pseudoroot` | CLI binary | Working |

### Implementation Details

The library intercepts the following system calls:

- **getuid()**, **geteuid()** - Return the fake UID
- **getgid()**, **getegid()** - Return the fake GID
- **stat()**, **fstat()**, **lstat()** - Return modified file info with fake ownership
- **chown()**, **lchown()**, **fchown()** - Record ownership changes and call real implementation
- **chmod()** - Pass through to real implementation

The fake state is configured via environment variables:
- `PSEUDOROOT_UID` - The fake UID to use (default: 0)
- `PSEUDOROOT_GID` - The fake GID to use (default: 0)

### Performance

Benchmark results (stat() loop across multiple threads):

```
workers    rate_native/s  rate_pseudoroot/s
128         48391305           15345986

effective parallelism (rate_w / rate_w1):
128             31.7              13.88
```

The library adds approximately 27% overhead per stat() call due to lock acquisition and ownership lookup. Parallelism is good because only the state access is serialized, not the actual I/O.

---

## Build Commands

```bash
cargo build                        # Build all crates
cargo test                         # Run all tests
cargo clippy -- -D warnings        # Lint — must be warning-free
cargo fmt --check                  # Format check — must pass
```

### Building the Shared Library

The shared library is built automatically with `cargo build`:

```bash
cargo build --release
```

The library will be created at `target/release/libpseudoroot_lib.so` (Linux) or `target/release/libpseudoroot_lib.dylib` (macOS).

---

## Installation

```bash
cargo install --path pseudoroot
```

## Local Development

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

## Contributing

See [AGENTS.md](./AGENTS.md) for project conventions (Conventional Commits, style, checks).

## Author

Luca Barbato <lu_zero@gentoo.org>
