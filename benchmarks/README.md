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
