# Agent Instructions

Read [README.md](README.md) for the current project state and follow
[CONTRIBUTING.md](CONTRIBUTING.md) for development and commit conventions.

## Goal

- Build a general-purpose, Rust-first language framework. Do not claim
  capabilities until their implementation and verification exist here.
- Keep language-specific semantics, runtime behavior, and output generation
  customizable by Rust hosts.

## Design and Workflow

- Before editing, inspect the relevant code, callers, tests, and current Git
  changes. Preserve unrelated work and follow any more specific instructions
  in affected directories.
- Before substantial feature or architecture work, clarify the goal, users, and
  constraints; inspect the existing design; compare viable approaches and
  tradeoffs; then recommend the simplest design that meets the need. Ask the
  user before implementation when unresolved choices could materially change
  public APIs, language behavior, architecture, or workflow. Otherwise, state
  reasonable assumptions and proceed.
- Consider the experience of framework contributors and language authors
  (developer experience), as well as people using languages built with Langmeld
  (end-user experience). Identify which audiences a change affects and validate
  the relevant workflows.
- Keep changes small, cohesive, and buildable. Establish and test the
  architecture before adding broad tooling or generated outputs.
- Add dependencies and abstractions only for a concrete requirement.
- Follow the code-layout guidance in [CONTRIBUTING.md](CONTRIBUTING.md): group
  related declarations and statements into logical blocks and separate distinct
  blocks with blank lines. Apply this by meaning, not mechanically.
- Preserve existing user changes. Do not discard work or rewrite Git history
  without explicit authorization.

## Rust Implementation and Testing

- Put tests for public behavior in the crate's `tests/` directory by default.
  Keep source-local `#[cfg(test)]` tests for private implementation details or
  invariants that cannot reasonably be tested through the public API. Do not
  widen visibility just to move a test.
- Treat language source as untrusted input: expected parse and validation
  failures should produce diagnostics, not panics. Use `expect` only for a
  real invariant, with the reason made clear.
- Do not commit temporary `todo!()` or `unimplemented!()` placeholders as
  feature implementations. Keep each committed change buildable.

## Documentation and Verification

- Keep user documentation focused on behavior available in this project.
- Keep project docs self-contained: describe this project and its checked-in
  behavior without local machine paths or private context.
- Verify behavior changes with the focused checks in
  [CONTRIBUTING.md](CONTRIBUTING.md), and report exactly what ran and whether
  it passed.

## Local Skills

Use these project-local skills when relevant:

- Rust implementation and review:
  `.agents/skills/rust-best-practices/SKILL.md`
- Rust tests and benchmarks: `.agents/skills/rust-testing/SKILL.md`

The skill sources and content hashes are recorded in `skills-lock.json`. When
updating skills, use the Skills CLI and review the skill and lock changes
together. Treat skills as guidance, not rigid project rules. Use judgment
instead of mechanically applying advice such as mocking every dependency,
enforcing a generic coverage percentage, or requiring exactly one assertion
per test. Do not add test dependencies unless a concrete testing need
justifies them.
