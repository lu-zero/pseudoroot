# Benchmarks

The scripts in [`bench/`](../bench) compare pseudoroot against native
execution, classic `fakeroot`, and
[fakeroost](https://github.com/lu-zero/fakeroost) (a ptrace-based sibling
project; the scripts are kept in sync with its copies):

- `bench/run.sh` — `stat()` throughput sweep across worker counts, using
  the `bench/stat-loop` harness.
- `bench/run-install.sh` — build → install → tar packaging workload with
  mixed root/installer ownership and `mknod`'d device nodes
  (`bench/install.mk`); exercises the inode map's chown/mknod/removal
  paths under concurrent load.
- `bench/run-make.sh` — parallel real `cc` compiles (`bench/Makefile`).

All three expect release builds (`cargo build --release --workspace`) and,
for the cross-tool columns, a fakeroost checkout at `../fakeroost` and a
system `fakeroot`.

## Latest results

Full output and system details are in
[`benchmark-results/`](benchmark-results/). Below are the headline numbers
from recent runs on different platforms.

### Linux (128-core aarch64)

From the [2026-07-01 run](benchmark-results/2026-07-01.md):

| Tool | stat() Throughput | Effective Parallelism | Notes |
|------|------------------|---------------------|-------|
| native | 42.5M/s | 26.5x | Baseline |
| pseudoroot | 7.4M/s | 6.7x | SHM session mode |
| fakeroost | 251.5K/s | 2.5x | Single-supervisor ceiling |
| fakeroot | 44.5K/s | 0.7x | Gets slower under contention |

The install workload completed correctly at every job level from 1 to 128,
with every `tar --numeric-owner` listing showing the expected mixed
ownership. On the compile workload pseudoroot tracks native wall time
almost exactly at every job level.

### macOS (12-core ARM64 Apple Silicon)

From the [2026-07-02 run](benchmark-results/2026-07-02.md) on a MacBook Pro with M2 Max:

| Tool | stat() Throughput | Effective Parallelism | Notes |
|------|------------------|---------------------|-------|
| native | 942K/s | 1.39x | Baseline |
| pseudoroot | 899K/s | 1.42x | SHM session mode |
| fakeroot | 43K/s | 0.51x | Regression under contention |

| Workload | Native | pseudoroot | Overhead |
|----------|--------|------------|----------|
| Parallel build (150 files, 12 jobs) | 1.10s | 1.14s | +4% |
| Install + tar (60 files, 8 jobs) | 1.52s | 1.51s | ~0% |

**Key insight:** pseudoroot is **~21x faster** than fakeroot for stat() operations
on macOS, with minimal overhead on real workloads.

### Cross-platform comparison

| Metric | Linux (128c) | macOS (12c) |
|--------|--------------|-------------|
| pseudoroot stat() peak | 7.4M/s | 899K/s |
| pseudoroot scaling | 6.7x | 1.42x |
| pseudoroot vs fakeroot | ~166x faster | ~21x faster |
| Build overhead | ~0% | +4% |

Performance scales with core count. The macOS numbers use Homebrew's GNU tools
to work around System Integrity Protection restrictions.
