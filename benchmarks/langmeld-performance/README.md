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
