# Performance baselines

`BASELINE.json` records the reference measurements taken immediately before
optimization work begins. It is a comparison anchor, not a universal promise:
hardware, filesystem, and build mode materially affect timings.

## Reproduce

```sh
# Core microbenchmarks and coarse regression budgets
cd sorrel-core
cargo bench --bench engine

# Release CLI: warm status on 10k files and log over 1k changes
cd ..
node scripts/benchmark-alpha.mjs > /tmp/sorrel-alpha-benchmark.json
```

Record CPU, memory, OS, filesystem, Rust, and Node versions alongside results.
Use the same machine for before/after optimization comparisons.

Correctness gates (`npm run validate:release`, conformance, module tests, root
E2E) must pass before accepting a faster result.

## Alpha baseline result

On the reference machine recorded in `BASELINE.json`:

- Core microbenchmarks are comfortably inside their coarse budgets.
- CLI `log` over 1,000 changes is at the roadmap target (49.403 ms vs 50 ms).
- Warm CLI `status` over 10,000 files is above the roadmap target
  (231.814 ms vs 100 ms) and is the first measured optimization priority.

## Foundation follow-up (2026-10-03)

A same-machine debug comparison on the laptop (Ryzen AI 7 350, Btrfs) used
five alternating warm-status samples per executable. The foundation binary
was compared with recorded-review, filesystem-safety, and tracked-path changes:

| Workload | Before median | After median | Blob opens before → after |
| --- | --- | --- | --- |
| 10,000 unique small files (298,890 payload bytes) | 1,863 ms | 1,814 ms | 20,000 → 10,000 |
| Same tree plus four 4 MiB files | 1,877 ms | 1,792 ms | 20,008 → 10,004 |

Blob bytes read halved in both workloads. Metadata-only tracked-path selection
and one verified cache read remove redundant I/O; cached corruption and symlink
checks remain enabled. New filesystem checks also add cost. Sample variation
was substantial, so these timings do not establish a large latency improvement
or attainment of the release-mode target. Do not compare these debug timings
with the release baseline above.
