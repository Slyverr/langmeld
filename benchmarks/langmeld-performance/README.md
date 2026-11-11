# Langmeld performance workloads

This standalone benchmark package measures the current examples without adding
benchmark dependencies to the framework crates or the main workspace lockfile.
It reuses the repository's root `target/` directory:

```sh
CRITERION_HOME=target/criterion cargo bench \
  --manifest-path benchmarks/langmeld-performance/Cargo.toml \
  --target-dir target --locked -- --noplot
```

The workloads cover distinct authoring paths:

- **Declarative parsing:** the WidgetScript grammar and Langmeld's built-in
  parser, with 16, 128, and 1,024 statements.
- **Rust-authored language analysis:** Aster parsing and ordered semantic
  analysis, with 16, 128, and 1,024 function declarations.
- **Workspace editor analysis:** Schema parsing, cross-file type analysis,
  context-aware completion, and the combined parse-and-analysis path, with 16,
  128, and 1,024 source files.

The harness checks that each fixture parses, analyzes, and produces the
expected semantic facts before measuring it. Parse-only and analysis-only
measurements reuse the already prepared language, source, and parse result, as
a long-running host normally would. The combined measurements include the
stages named by each benchmark. These are consumer workloads, not a ranking of
the language examples or a claim about parser throughput in isolation.

For comparable results, record the commit, Rust version, target triple, CPU,
operating system, power mode, and exact command. Criterion stores reports under
`target/criterion/`. Use `--save-baseline NAME` and `--baseline NAME` to compare
two revisions on the same machine. Run a second pass to check that results are
stable before drawing conclusions. This harness does not measure peak memory;
add that only when a representative workload gives a reason to investigate it.

## Recorded run: lossless declarative syntax tree

The 2026-11-11 run measured the working tree based on `5274b60`, including the
lossless syntax tree returned by the declarative parser. The harness's fixture
and semantic correctness checks passed. It used Rust 1.96.0 on
`x86_64-unknown-linux-gnu`, an AMD Ryzen 5 5500, and the `powersave` governor.
The command used 10 Criterion samples and a one-second measurement target per
workload:

```sh
CRITERION_HOME=target/criterion cargo bench \
  --manifest-path benchmarks/langmeld-performance/Cargo.toml \
  --target-dir target --locked --offline -- \
  --noplot --sample-size 10 --measurement-time 1
```

Median times for 1,024 items were:

| Workload | Median |
| --- | ---: |
| WidgetScript parse | 663 µs |
| Aster parse / analysis / combined | 682 / 647 / 1,504 µs |
| Schema parse / workspace analysis / completion / combined | 320 / 314 / 67 / 597 µs |

WidgetScript parsing now also builds source-preserving syntax nodes and token
trivia; its 1,024-statement parse remains below one millisecond on this host.
These short measurements are directional, not release thresholds. The earlier
0.6.0 baseline used 20 samples, so compare the figures cautiously; no profiling
or memory measurement was performed.
