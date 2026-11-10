# Contributing

Keep the framework understandable and useful to language authors. Make each
change a small, buildable step, and include the tests and documentation that
belong to it.

## Development

The minimum supported Rust version is 1.88. `rust-toolchain.toml` selects the
current stable compiler for development. Release validation should also check
the minimum version. Before submitting Rust changes, run the relevant checks:

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run \
  --manifest-path tests/fixtures/downstream-consumer/Cargo.toml \
  --target-dir target \
  --locked
```

## Design changes

For substantial features or architecture changes, clarify the use case and
constraints, compare plausible approaches, and explain the chosen tradeoffs
before implementation. Discuss unresolved choices that could materially affect
public APIs, language behavior, architecture, or workflow before proceeding.
Consider developer experience for framework contributors and language authors,
and end-user experience for people using languages built with Langmeld. Prefer
the simplest design that meets the need without imposing unnecessary limits.

## Test organization

Put tests for behavior visible through a crate's public API in that crate's
`tests/` directory by default. Cargo compiles these as integration-test crates,
so they verify how consumers use the API.

Use source-local `#[cfg(test)]` unit tests when a test needs private functions,
private types, or internal invariants that cannot reasonably be checked through
the public API. Do not make an item public or `pub(crate)` only to reach it
from an integration test. Keep tests focused on one behavior; several
assertions are fine when they verify different parts of that same behavior.
Add a regression test when fixing a bug.

Use mocks, property-testing tools, snapshots, and coverage tools when they
solve a concrete problem. They are optional techniques, not required
dependencies or blanket coverage targets. Keep documentation accurate to
implemented and verified capabilities.

Benchmark only when a change has a concrete performance goal or a measured
performance concern. Use representative workloads and compare against a
baseline; do not add benchmark infrastructure speculatively.

## Changelog and releases

Record notable user-facing changes in `CHANGELOG.md` under `Unreleased`,
grouped as `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`
as needed. Curate entries for users; do not copy the commit log.

For a release, move entries to `## [X.Y.Z] - YYYY-MM-DD`, add a fresh
`Unreleased` section, and keep the Cargo workspace version, changelog, and Git
tag aligned. Regenerate `Cargo.lock` with Cargo, then run the development
checks above and `cargo package --workspace --locked`. Use
`chore(release): prepare vX.Y.Z` for the release-preparation commit.

## Commit conventions

Use `type(scope): message`, for example:

```text
feat(langmeld-core): add source spans
chore(langmeld-core/deps): add serde
```

Choose a type such as `feat`, `fix`, `refactor`, `test`, `docs`, `chore`,
`perf`, or `ci`. Use a crate or target as the scope, with at most two path
segments (`A/B`). Omit an unnecessary scope (`docs: explain setup`). Use
`chore(crate/deps): message` for dependency changes.

Add a brief body only when the subject cannot express useful context. A body
may have a paragraph, related bullets, a closing note, and a trailer; include
only what helps and put trailers last:

```text
fix(langmeld-core): reject empty source names

Reject empty names before assigning a source identity.

- Cover empty and whitespace-only names.
- Keep the existing diagnostic.

Verified with `cargo test -p langmeld-core`.
Refs: #123
```

Make each commit one coherent change; fold fixes into it instead of adding a
chain of generic `fix` commits.
