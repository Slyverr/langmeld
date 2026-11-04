# Contributing

Langmeld is a Rust framework for creating programming languages and domain
specific languages. Contributions should keep its APIs understandable,
composable, and reliable for language authors.

## Development workflow

1. Identify the capability and inspect its callers, tests, and documented
   limits.
2. Choose one cohesive slice with a clear dependency boundary. Keep the project
   buildable at the end of the change.
3. Add or update focused tests for behavior changes, user documentation when
   commands or workflows change, and `CHANGELOG.md` for notable user-facing
   changes. Keep changelog entries concise and describe the impact on language
   authors rather than listing internal commits.
4. Run the relevant checks before committing. For the current starter, use:

   ```sh
   cargo fmt --all -- --check
   cargo test --workspace --locked
   cargo clippy --all-targets --locked -- -D warnings
   RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
   ```

   CI also checks the supported Rust toolchain and operating-system matrix,
   generated artifacts, and the independent example projects. Run the matching
   example checks when changing shared APIs or generated output. Before
   release-related changes, verify package contents and packaged builds with:

   ```sh
   cargo package --workspace --locked \
     --config 'patch.crates-io.langmeld-core.path="crates/langmeld-core"' \
     --config 'patch.crates-io.langmeld-expression.path="crates/langmeld-expression"' \
     --config 'patch.crates-io.langmeld-language.path="crates/langmeld-language"' \
     --config 'patch.crates-io.langmeld-lsp.path="crates/langmeld-lsp"' \
     --config 'patch.crates-io.langmeld.path="crates/langmeld"'
   ```

   These local patches let Cargo verify dependent crate archives and test-only
   workspace dependencies before the crates have been published to crates.io.
   After packaging, check a generated project against the extracted archives:

   ```sh
   bash scripts/check-packaged-consumer.sh
   ```

   If Cargo uses a custom target directory, set `CARGO_TARGET_DIR` when
   packaging as well. Set `LANGMELD_PACKAGE_ROOT` separately only when the
   archives are stored outside its default `target/package` subdirectory.
   Locally, the packaged-consumer check creates or reuses
   `../langmeld-packaged-consumer`, a generated sibling of the checkout, and
   uses the repository's `target/` cache by default. CI points the same check
   at a fresh runner directory, so it does not depend on this local copy. Set
   `LANGMELD_PACKAGE_CONSUMER_PROJECT` to choose another dedicated path. The
   script refuses to take over a nonempty unmarked path.
5. Review the diff for generated files, unrelated changes, inaccurate claims,
   and a clean build from the checked-in sources.

Avoid speculative abstractions and dependencies. Keep parser, analysis, runtime,
and backend boundaries explicit so users can combine Langmeld's generic
capabilities with their own Rust implementations.

## Versioning and releases

- Workspace crates share the version declared in the root `Cargo.toml`. Keep
  internal crate requirements and any versioned package constraints aligned
  with it.
- Follow [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) for
  `CHANGELOG.md`: keep an `Unreleased` section first, use its `Added`, `Changed`,
  `Deprecated`, `Removed`, `Fixed`, and `Security` categories as applicable,
  and describe user-visible effects rather than commit history. Group related
  changes into a concise entry; omit empty categories.
- Do not bump the version for each feature commit. Before the first release,
  target `0.1.0` and accumulate its changes under `Unreleased` until the gates
  pass. After `0.1.0` ships, accumulate later pre-1.0 work toward `0.2.0`.
- Before release, confirm the workspace package versions and internal
  dependency requirements agree, then refresh the root and affected consumer
  lockfiles. Run workspace tests, formatting, strict Clippy, warnings-denied
  docs, the documented package verification command, and
  `bash scripts/check-packaged-consumer.sh`. Also run
  `python3 -m unittest discover -s scripts/tests -p 'test_*.py'` and
  `python3 scripts/check_version_sync.py`. The version guard requires Python
  3.11+ and checks workspace crates, internal dependencies, example manifests,
  and their lockfiles. Require the hosted toolchain/platform matrix,
  generated-artifact checks, and a fresh-project workflow review to pass too.
- Prepare each release in one commit. For the first one, use
  `chore(release): finalize 0.1.0`; substitute the new version in that subject
  for later releases. Move shipped entries from `Unreleased` to a dated version
  heading, start a fresh `Unreleased` section, update the root workspace
  version, internal crate requirements, example-consumer requirements, and all
  affected lockfiles together. Refresh each affected lockfile before running
  locked checks, then review the lockfile diff for unrelated dependency
  updates. Review `cargo metadata --offline --locked --no-deps
  --format-version 1` and every `langmeld*` dependency in `crates/`, `examples/`,
  and `benchmarks/` to confirm versions and constraints agree. Run the full
  release checks after those edits; do not tag a failing or unverified state.
- After the release commit's full CI passes, create and push the matching
  `vMAJOR.MINOR.PATCH` Git tag. Wait for the tag-triggered validation to pass
  before publishing. Prerelease package versions and tags must use the same
  suffix, such as `0.2.0-alpha.1` and `v0.2.0-alpha.1`.
- Pushing a version tag currently runs CI only; it does not publish crates.
  Publish manually with Cargo credentials configured outside the repository,
  never by committing a registry token. Publish dependencies before their
  dependents, in this workspace order:

  ```sh
  cargo publish -p langmeld-core
  cargo publish -p langmeld-expression
  cargo publish -p langmeld-language
  cargo publish -p langmeld
  cargo publish -p langmeld-lsp
  cargo publish -p langmeld-cli
  ```

  Confirm each published version is available from crates.io before publishing
  the next crate. The package graph is checked from the workspace manifests;
  if those dependencies change, review this order.
- Follow [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) with
  this project's pre-1.0 policy: patch releases are for compatible fixes;
  minor releases add capabilities and may include breaking API changes while
  the project is still `0.y.z`. Document any break and migration path. Reserve
  `1.0.0` until the public Rust APIs, generated-project contract, supported
  targets, and compatibility policy are ready to be treated as stable.
- From `1.0.0` onward, increment MAJOR for incompatible public API changes,
  MINOR for backward-compatible functionality, and PATCH for backward-
  compatible bug fixes. Reset lower components when a higher component changes.

## Commit conventions

- The repository baseline is a single commit named exactly `Initial commit`.
- For later changes, use a concise Conventional Commit subject with the most
  useful crate and target scope:

  ```text
  <type>(<crate>/<target>): <short message>
  ```

  Keep scopes to at most two levels (`A/B`): a crate plus one target or folder.
  Do not add a third nested level. Use names that match the actual crate,
  directory, or target. For example:

  ```text
  feat(langmeld-core/spec): add source snapshots
  refactor(langmeld-language/parser): isolate pattern compilation
  test(langmeld-cli/commands): cover custom lexer recovery
  ```

  When there is no crate or target to name, use `type: short message` or a
  useful single scope such as `type(scope): short message` (for example,
  `docs: explain the init workflow`).
- Dependency changes use `chore(<crate>/deps): <short message>`, such as
  `chore(langmeld-language/deps): update serde`.
- A commit body is optional. Add one only when it provides useful context that
  does not fit in the subject. Keep it brief: short paragraphs and, when helpful,
  concise bullet lists. Do not repeat the subject in the body.
- Keep each commit to one coherent, buildable capability or infrastructure
  layer. Include its tests and docs with the change that owns them.
- Avoid a chain of generic `fix(...)` commits. Fold corrections into the feature
  or refactor that exposed them; use a standalone `fix(area): ...` only for a
  distinct defect that cannot sensibly ship with its owning work.
- Do not combine unrelated cleanup with a feature. Record meaningful redesigns
  in the commit description or relevant documentation.

## Local agent skills

The Rust guidance used for this repository is copied to `.agents/skills/` and
tracked in `skills-lock.json`. Read the relevant skill before Rust
implementation, review, test-design, or performance work. Keep updates scoped to
the skill itself and review upstream content before replacing a copy.
