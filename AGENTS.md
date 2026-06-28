# Project Conventions

## Build Commands

```bash
cargo build                        # Build all crates
cargo test                         # Run all tests
cargo clippy -- -D warnings        # Lint — must be warning-free
cargo fmt --check                  # Format check — must pass

# MSRV verification (use cargo-msrv)
cargo install cargo-msrv
cargo msrv verify                  # Verifies the rust-version declared in Cargo.toml
```

## Architecture

Workspace crates live at the repository root:

- `pseudoroot-core` — shared types, state management, and IPC protocol
- `pseudoroot-lib` — cdylib that intercepts system calls via library interposition
- `pseudoroot-daemon` — daemon for persistent state across processes
- `pseudoroot` — CLI binary that preloads the library and executes commands
- `pseudoroot-tests` — integration and interposition tests

Read the [design document](https://github.com/lu-zero/pseudoroot) for architecture reference.

## Dependencies

Shared crate dependencies live in `[workspace.dependencies]` in the root `Cargo.toml`.
Member crates inherit package metadata via `field.workspace = true` and dependencies via
`dep.workspace = true`. Run `cargo autoinherit --prefer-simple-dotted` after adding deps.

- `ctor` — Library initialization for the cdylib
- `libc` — Used for `dlsym(RTLD_NEXT)` and libc type definitions (`pseudoroot-lib` only)
- `clap` — CLI argument parsing

## Coding Style

- `rustfmt` — all code must be formatted
- No dead code, no unused dependencies
- Doc comments on all public types and functions
- Tests live in a `#[cfg(test)] mod tests` block
- Use `#[must_use]` for functions that return values that should be used
- Use `#[inline]` for small, performance-critical functions

## Commits

[Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` — new functionality
- `fix:` — bug fix
- `refactor:` — code restructuring without behaviour change
- `docs:` — documentation only
- `test:` — adding or updating tests
- `ci:` — CI/CD changes
- `chore:` — maintenance (dependencies, tooling)

When a commit was significantly assisted by an AI tool, note it with an
`Assisted-by:` trailer rather than a `Co-Authored-By:` trailer. Use the kernel's
format (`AGENT_NAME:MODEL_VERSION`, colon-separated, e.g.
`Assisted-by: Mistral Vibe:mistral-medium-3.5`). Only list *specialized* analysis tools after the
model version if any were used; basic dev tools (git, cargo, editors) are not
listed. The agent never adds a `Signed-off-by` (DCO) — that is the human's.

## MSRV

The workspace tracks **latest stable** dependencies and bumps `rust-version` as needed.
Currently targeting Rust 2024 edition.

CI runs `stable` and the declared workspace minimum. After a release,
foundational crates may advertise a lower standalone MSRV; the workspace
floor follows whatever latest deps require.

When a dependency bump needs a newer compiler, raise `rust-version` in every
affected `Cargo.toml` and the CI matrix entry, then `cargo msrv verify`.

## Slop Warning

This codebase was largely AI-generated. Be skeptical of existing code — it may
contain bugs or surprising behaviour. Do not assume existing patterns are
correct.
