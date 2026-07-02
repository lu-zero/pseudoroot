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
[`benchmark-results/`](benchmark-results). From the 2026-07-01 run
(128-core aarch64, default SHM session mode):

```
# bench/run.sh: stat() calls/sec, effective parallelism at 128 workers
native      42.5M/s   (26.5x)
pseudoroot   7.4M/s   ( 6.7x)
fakeroost  251.5K/s   ( 2.5x)  -- single-supervisor ceiling
fakeroot    44.5K/s   ( 0.7x)  -- gets slower under contention
```

The install workload completed correctly at every job level from 1 to 128,
with every `tar --numeric-owner` listing showing the expected mixed
ownership. On the compile workload pseudoroot tracks native wall time
almost exactly at every job level.
