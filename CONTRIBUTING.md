# Contributing to Langmeld

Thanks for your interest in Langmeld! Contributions of every size are welcome, from a typo
fix or a bug report to a new feature.

Langmeld is experimental and at an early stage, so expect things to change. For what the
project is, see [README.md](README.md).

## Ways to contribute

- **Report a bug:** see [Reporting issues](#reporting-issues).
- **Suggest an idea:** open an issue before building it, so the approach can be agreed.
- **Improve the docs:** fix whatever is unclear, missing or out of date.
- **Write code:** start with [Setup](#setup), then follow the [Workflow](#workflow).

## Contents

| To | Read |
|----|------|
| Get started | [Setup](#setup), [Commands](#commands), [Project structure](#project-structure) |
| Make a change | [Workflow](#workflow), [Rules](#rules), [Code conventions](#code-conventions) |
| Submit it | [Commits](#commits), [Changelog](#changelog), [Documentation](#documentation), [Pull requests](#pull-requests) |
| Publish a version | [Releases](#releases) (maintainers) |

This file is the source of truth for how work is done in Langmeld, by humans and AI agents
alike.

## Setup

You need [rustup](https://rustup.rs/), which installs the toolchain pinned in
`rust-toolchain.toml`, and a C linker (`cc`). With [Nix](https://nixos.org/) flakes enabled,
`nix develop` provides both, plus the other development tools; [direnv](https://direnv.net/)
users can run `direnv allow` instead.

```sh
git clone https://github.com/Slyverr/langmeld.git
cd langmeld
cargo fetch --locked
```

Verify it works by running the tests (see Commands).

## Commands

| Task | Command |
|------|---------|
| Setup | `cargo fetch --locked` |
| Build | `cargo build --workspace --locked` |
| Run | `cargo run -p langmeld-cli --locked` |
| Test | `cargo nextest run --workspace --locked` (doc tests: `cargo test --workspace --doc --locked`) |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Format | `cargo fmt --all` (check only: add `--check`) |
| WASM check | `cargo build -p langmeld-syntax --target wasm32-unknown-unknown --locked` |
| **Check (local)** | `cargo xtask check`: format, lint, tests and WASM build in under two minutes |
| Pull-request set | `cargo xtask ci`: the same with more test cases, plus doc tests and WASM tests in Node |

## Project structure

| Path | Contains |
|------|----------|
| `crates/langmeld/` | Facade: the only crate users depend on |
| `crates/langmeld-syntax/` | Core types, lexer, grammar, parser engine, syntax tree |
| `crates/langmeld-meta/` | `.langmeld` format, `langmeld.toml`, code generation |
| `crates/langmeld-ide/` | Binding, projects, language server, editor packaging |
| `crates/langmeld-cli/` | The `langmeld` binary and the CLI kit |
| `tests/` | Corpus files and the runner that checks them |
| `fuzz/` | Fuzz targets (nightly Rust; its own workspace) |
| `xtask/` | The `cargo xtask` commands (its own workspace) |
| `docs/` | In-depth documentation |
| `.github/` | Issue and pull request templates |
| `.github/workflows/` | CI on every pull request, and the longer nightly run |
| `nix/` | The Nix development shell, loaded by `flake.nix` |

## Workflow

1. Open or pick an issue for anything non-trivial, so the approach can be agreed first.
2. Branch from `main`: `<type>/<short-description>` (e.g. `feat/export-csv`,
   `fix/login-crash`). Fork the repository first if you can't push to it.
3. Keep changes focused: one concern per branch, no unrelated refactors.
4. Before pushing, run `cargo xtask check` (see Commands).
5. Update [CHANGELOG.md](CHANGELOG.md) in the same commit as a user-facing change (see
   Changelog).
6. Update docs affected by the change (see Documentation).
7. Open a pull request (see Pull requests).

## Rules

- **Always:** follow existing patterns in the code you touch; add tests for new behavior.
- **Discuss first:** new dependencies, changes to public APIs or file formats, CI/build
  configuration, deleting files, anything security-related.
- **Never:** commit secrets or credentials; edit generated files by hand; rewrite `main`
  or a published tag.
- **Done:** a change is done when it has been shown working, not only when its tests pass.
- **Maintainer-owned:** the grammar that describes `.langmeld` files, the public API of the
  `langmeld` crate, and the learning exercise in `spike/`. Others propose changes to them;
  the maintainer writes or approves them.

## Code conventions

- Follow the existing style; the formatter and linter are the final word.
- Never panic on user input. Use `expect` only for real invariants, and state the reason.
- `unsafe` code is forbidden by the workspace lints.
- `langmeld-syntax` must keep building for `wasm32-unknown-unknown`: no threads,
  filesystem, time or stdio.
- For the lexer, parser, validation and binding, write the test before the implementation.
- Don't add speculative options, crates or public items. Keep items `pub(crate)` unless
  they are meant to be public API.
- Define each version number in one place.
- Examples, tests and docs use invented names and languages of their own, never those of
  other projects.
- Public APIs need documentation, and code blocks in docs must compile.
- `examples/` holds languages built with Langmeld only, each showing what the framework can
  do. Prefer a few that show a lot over many small ones.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/). The title says where the
change took effect:

| Change | Title | Example |
|--------|-------|---------|
| Inside one area | `<type>(<area>/<scope>): <summary>` | `feat(langmeld-syntax/lexer): add nested comments` |
| Repository | `<type>(<scope>): <summary>` | `chore(release): prepare v0.2.0` |
| Repository, no clear scope | `<type>: <summary>` | `docs: fix broken links` |

- `<area>` is a crate's full name or a top-level folder, such as `examples` or `docs`, and
  `<scope>` is the part of it that the work targets.
- Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`.
- The body is optional. Add one when the reason isn't obvious: say what was wrong or
  missing and why it matters, then what changed. Leave a blank line after the title, wrap
  at 72 characters, and use plain sentences or short bullets. Don't list the files touched
  or repeat the title.
- Commit messages carry no co-author or tool credit lines.
- Breaking changes: add `!` before the colon (`feat(langmeld-syntax/parser)!: ...`) and
  explain in the body.
- Every commit builds. Give each crate its own commit when that stays true.
- Before a pull request is merged, fold each fix for its own work into the commit it fixes.

## Changelog

[CHANGELOG.md](CHANGELOG.md) follows [Keep a Changelog](https://keepachangelog.com/).

- Add an entry under `## [Unreleased]` for every user-facing change, in the matching
  section: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`. Create a
  section only when it gets its first entry; never leave empty ones.
- Put the entry in the commit that makes the change, not in a batch at the end. If it was
  forgotten, fold it into that commit, or add a `docs(changelog): ...` commit if you would
  rather not rewrite the branch.
- Write for users, not developers: what changed and why it matters, one line each.
- Skip internal-only changes (refactors, CI, tests) unless they affect users.

## Documentation

- The [README](README.md) stays short: intro, install, quickstart.
- Anything deeper goes in [docs/](docs/README.md), which explains its structure and rules.
- Docs change with the code: update affected docs in the same branch, including this file
  when setup, commands or structure change.

## Pull requests

Fill in the [pull request template](.github/pull_request_template.md); GitHub adds it to a
new pull request. A pull request opened from the command line must use it too.

Every change reaches `main` through a pull request, however small. Once every check passes,
it is merged with **Rebase and merge**, which keeps each commit and a linear history.

## Releases

This section is for maintainers. Versions follow
[Semantic Versioning](https://semver.org/): MAJOR for breaking changes, MINOR for features,
PATCH for fixes.

A version is released from the pull request that holds its last changes. There is no
separate pull request for the release.

1. As the last commit of that pull request, rename `## [Unreleased]` in
   [CHANGELOG.md](CHANGELOG.md) to `## [X.Y.Z] - YYYY-MM-DD`, add a fresh empty
   `## [Unreleased]` above it, and set `version` under `[workspace.package]` in
   `Cargo.toml`.
2. Commit as `chore(release): prepare vX.Y.Z`.
3. Merge the pull request once every check passes.
4. Tag the `chore(release)` commit on `main` `vX.Y.Z` with a signed tag and push the tag.
   Merging gives commits new hashes, so tag the one on `main`, not the one on the branch.

## Reporting issues

Open an issue with the bug report or feature request form. For a bug, include what you
expected, what happened, steps to reproduce, and your version and environment. Report
security issues
[privately](https://github.com/Slyverr/langmeld/security/advisories/new), not in public
issues.
