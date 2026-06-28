# pseudoroot

[![LICENSE](https://img.shields.io/badge/license-MIT%20OR%20Apache-2.0-blue.svg)](LICENSE-MIT)
[![Build Status](https://github.com/lu-zero/pseudoroot/workflows/CI/badge.svg)](https://github.com/lu-zero/pseudoroot/actions?query=workflow:CI)
[![dependency status](https://deps.rs/repo/github/lu-zero/pseudoroot/status.svg)](https://deps.rs/repo/github/lu-zero/pseudoroot)

A Rust implementation of the fakeroot functionality using library interposition. This allows running commands that think they have root privileges without actually requiring root access.

> **Note**: This is a work-in-progress implementation. The library interposition is currently a stub and needs to be fully implemented to provide actual fake root functionality.

## The `pseudoroot` binary

`pseudoroot` is a command-line tool that runs a specified command with fake root privileges by preloading the pseudoroot library.

### Usage

```bash
pseudoroot <command> [args...]
```

This sets the appropriate environment variable (`LD_PRELOAD` on Linux, `DYLD_INSERT_LIBRARIES` on macOS) and executes the given command with the pseudoroot library preloaded.

### Examples

```bash
# Run a simple command with fake root
pseudoroot id

# Build a package with fake root
pseudoroot emerge --ask package

# Print the library path
pseudoroot --print-library-path
```

### Options

| Option | Description |
|--------|-------------|
| `--print-library-path` | Print the path to the pseudoroot library and exit |
| `--help` | Show help message |
| `--version` | Show version information |

---

## Architecture

See [`docs/architecture.md`](./docs/architecture.md) for the full design reference (not yet created).

### Workspace Structure

The project uses a Cargo workspace with the following crates:

| Crate | Purpose | Status |
|-------|---------|--------|
| `pseudoroot-core` | Shared types, state management, and IPC protocol | Implemented |
| `pseudoroot-lib` | The interposed shared library (cdylib) | Stub implementation |
| `pseudoroot-daemon` | Optional daemon for persistent state | Stub |
| `pseudoroot` | CLI binary | Working |

### Key Components

- **pseudoroot-core**: Provides the `FakeRootState`, `FileOwnership`, and `UidGidMap` types for managing fake ownership information.
- **pseudoroot-lib**: A shared library that intercepts system calls like `getuid()`, `geteuid()`, `chown()`, `stat()`, etc., to return fake values.
- **pseudoroot-daemon**: (Future) A long-running process that maintains persistent fake state across multiple processes.
- **pseudoroot**: The CLI that sets up the environment and executes commands with the library preloaded.

### Implementation Approach

The library uses **library interposition** via:
- `LD_PRELOAD` on Linux
- `DYLD_INSERT_LIBRARIES` on macOS

It intercepts system calls and returns fake values while maintaining the real state in memory. The implementation uses:
- `ctor` crate for automatic library initialization
- `rustix` for safe syscall access
- `libc` for `dlsym(RTLD_NEXT)` to call the real implementations

---

## Build Commands

```bash
cargo build                        # Build all crates
cargo test                         # Run all tests
cargo clippy -- -D warnings        # Lint — must be warning-free
cargo fmt --check                  # Format check — must pass
```

### Building the Shared Library

The shared library needs to be built with `cargo-c`:

```bash
# Install cargo-c
cargo install cargo-c

# Build the library in release mode
cargo cbuild -p pseudoroot-lib --release
```

The library will be created at `target/cbuild/release/libpseudoroot_lib.so` (Linux) or `target/cbuild/release/libpseudoroot_lib.dylib` (macOS).

### Manual Build (without cargo-c)

Alternatively, you can build the cdylib directly:

```bash
cargo build -p pseudoroot-lib --release
```

The library will be at `target/release/libpseudoroot_lib.so`.

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
