# Langmeld performance workloads

This standalone package measures representative language-host pipelines that
consume Langmeld. It is excluded from the main workspace and has its own lock
file, so Criterion and its support crates do not become runtime dependencies
or part of the published framework crates.

The workloads cover three distinct use cases:

- `ledger_interpreter`: a declarative language parsed and analyzed by Langmeld,
  then evaluated by a Rust host.
- `c_signatures_to_rust_ffi`: a host-registered parser for a small C signature
  subset, generic workspace analysis, then Rust FFI generation.
- `rust_function_source_generation`: a Rust-authored language parsed and
  analyzed by Langmeld, then lowered to Rust source by its host.

Each workload has 16, 128, and 1,024 records. The suite times compiled-language
loading and parser setup separately from parsing, workspace analysis, host
execution or generation, and the full per-input pipeline. It checks valid
parses, clean workspace analysis, and expected output counts before timing.
The full pipeline reuses the parser and language model, as an editor or
long-running host normally would. These workloads exercise different
semantics; their timings should not be used as a direct ranking of the language
designs.

Run the statistical benchmarks in release mode from the repository root:

```sh
cargo bench --manifest-path benchmarks/langmeld-performance/Cargo.toml --locked
```

Criterion stores reports under the package's `target/criterion` directory.
For comparable measurements, record the commit, Rust version, target triple,
CPU model, operating system, power mode, and benchmark command. Close other
CPU-intensive work and run a second pass to check that the estimates repeat.
Performance claims should identify the exact workload and Criterion estimate
and variance rather than extrapolating one language's result to another.

The benchmark consumer has a separate `Cargo.lock`; refresh it intentionally
with `cargo update --manifest-path benchmarks/langmeld-performance/Cargo.toml`
and commit the resulting lockfile when changing its dependency versions.
