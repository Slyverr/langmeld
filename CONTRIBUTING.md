# Contributing

Langmeld is a Rust framework for creating programming languages and domain
specific languages. Contributions should keep its APIs understandable,
composable, and reliable for language authors.

## Development workflow

1. Identify the capability and inspect its callers, tests, and documented
   limits.
2. Choose one cohesive slice with a clear dependency boundary. Keep the project
   buildable at the end of the change.
3. Add or update focused tests for behavior changes, plus user documentation
   when commands or workflows change.
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
   cargo package --workspace --locked
   ```
5. Review the diff for generated files, unrelated changes, inaccurate claims,
   and a clean build from the checked-in sources.

Avoid speculative abstractions and dependencies. Keep parser, analysis, runtime,
and backend boundaries explicit so users can combine Langmeld's generic
capabilities with their own Rust implementations.

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
