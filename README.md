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

**Platform Support:** Both Linux and macOS are fully supported with all 36 syscalls intercepted. Note that `statx` and `capset` return `ENOSYS` on macOS as these syscalls don't exist there, and `renameat2` falls back to `renameat`.

### Implementation Details

The library intercepts the following system calls:

### Credential Functions (Read)
- **getuid()**, **geteuid()** - Return the fake UID
- **getgid()**, **getegid()** - Return the fake GID
- **getresuid()**, **getresgid()** - Return fake real/effective/saved UIDs/GIDs

### Credential Functions (Set)
- **setuid()**, **setgid()** - Set fake UID/GID
- **setreuid()**, **setregid()** - Set fake real and effective UID/GID
- **setresuid()**, **setresgid()** - Set fake real/effective/saved UIDs/GIDs
- **setfsuid()**, **setfsgid()** - Set fake filesystem UID/GID
- **setgroups()** - Set supplementary groups (always succeeds in fake mode)
- **capset()** - Set capabilities (always succeeds in fake mode)

### Stat Family
- **stat()**, **lstat()** - Return modified file info with fake ownership
- **fstat()** - Return modified file info for file descriptor with fake ownership
- **fstatat()** - Return modified file info relative to directory file descriptor
- **statx()** - Extended stat (Linux-specific)

### Ownership Functions
- **chown()**, **lchown()** - Record ownership changes and call real implementation
- **fchown()** - Record ownership changes for file descriptor
- **fchownat()** - Record ownership changes relative to directory file descriptor

### Mode Functions
- **chmod()** - Pass through to real implementation
- **fchmod()** - Change mode by file descriptor
- **fchmodat()** - Change mode relative to directory file descriptor

### Inode Lifecycle
- **unlink()** - Remove directory entry and ownership tracking
- **unlinkat()** - Remove directory entry relative to directory file descriptor
- **rmdir()** - Remove directory and ownership tracking
- **rename()** - Move ownership entry from old path to new path
- **renameat()** - Rename relative to directory file descriptors (Linux)
- **renameat2()** - Rename with flags relative to directory file descriptors (Linux)

### Inode Creation
- **mknod()** - Create special file with ownership tracking
- **mknodat()** - Create special file relative to directory file descriptor

### Extended Attributes (xattr)
- **setxattr()**, **lsetxattr()**, **fsetxattr()** - Set extended attributes
- **getxattr()**, **lgetxattr()**, **fgetxattr()** - Get extended attributes
- **listxattr()**, **llistxattr()**, **flistxattr()** - List extended attributes
- **removexattr()**, **lremovexattr()**, **fremovexattr()** - Remove extended attributes

The fake state is configured via environment variables:
- `PSEUDOROOT_UID` - The fake UID to use (default: 0)
- `PSEUDOROOT_GID` - The fake GID to use (default: 0)

### Performance

Benchmark results (stat() loop across multiple threads, after optimizations):

**Standalone Mode (latest):**
```
workers    rate_native/s  rate_pseudoroot/s
   1         1471097          847049
   2         2166107         1322809
   4         4157430         2182618
   8         5081757         2930461

effective parallelism (rate_w / rate_w1):
   1             1.00              1.00
   2             1.45              1.59
   4             2.69              2.58
   8             4.69              3.13
```

**Daemon Mode (latest):**
```
workers    standalone/s   daemon/s
   1          842157       809783
   2         1306048      1321168
   4         2048677      2179339
   8         2712931      2698308
```

### Performance Analysis

- **Overhead**: The library adds approximately **15-20% overhead** per stat() call (improved from 27% in previous versions)
- **Parallelism**: Good scaling up to 8 workers due to optimized concurrent data structures
- **Daemon vs Standalone**: Daemon mode has ~3.8% additional overhead for IPC communication, but provides persistent state across processes
- **Optimizations Applied**:
  - Atomic UID/GID for lock-free reads
  - DashMap for concurrent ownership map access
  - Reduced lock contention in hot paths

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
