# Langmeld Project Report

## Project overview

Langmeld is a Rust framework for building programming languages and domain
specific languages. It provides shared language models and reusable services;
a language project supplies its syntax, semantics, host behavior, and desired
outputs.

The framework is organized as a Cargo workspace with focused crates for core
models, expressions, language parsing and analysis, the public API, the command
line, and language-server support.

## Capabilities

- **Language definitions:** Author a language with Rust APIs, modular `.lm`
  definitions, or a combination. The inputs are validated into a canonical
  language model.
- **Lexing and parsing:** Use source-spanned tokens, declarative syntax patterns,
  typed captures, lossless syntax trees, error recovery, configurable limits,
  and host-provided parsers for syntax outside the declarative pattern model.
- **Language analysis:** Resolve symbols and references across source files,
  track lexical scopes, and add checked host callbacks for language-specific
  semantics and type information.
- **Language project tooling:** Initialize and configure projects, discover
  source files, validate definitions and sources, format supported syntax,
  inspect language models, manage local declarative extensions, and produce
  deterministic generated artifacts.
- **Editor support:** Run a language server with diagnostics, document symbols,
  definitions and references, hover, semantic tokens, completion, and safe
  rename for supported generic symbol analysis.
- **Outputs and backends:** Describe backend capabilities and settings, plan
  transformation passes, and produce workspace artifacts. Backend execution is
  registered by the language host so a project can target its own needs.
- **Project scaffolding:** Use the interactive or scripted `langmeld init`
  workflow to create Rust, declarative, or combined language-authoring projects.
  Generated files are kept under `.langmeld/`.
- **Consumer example:** The standalone declarative-interpreter example shows a
  modular `.lm` grammar, workspace reference resolution, and host-owned AST
  evaluation in a generated CLI project.

## Design boundaries

Langmeld is a framework rather than a single language definition or universal
compiler. The declarative syntax model covers common grammar patterns. A
language needing different syntax or behavior can supply Rust implementations
through the host APIs. The two routes meet at the canonical language model and
shared services where their contracts overlap.

Backends are independent extensions of a language host. The framework does not
require every language to lower into one universal intermediate representation
or target a fixed set of outputs. A backend must be implemented and registered
before a project can use it.

The current generated backend contracts and artifact pipeline are foundations
for backend integrations; they do not by themselves make arbitrary language
definitions executable or guarantee a particular deployment target. Browser
editors can consume editor artifacts and analysis results, while compiling a
language inside WebAssembly requires a compatible host and runtime build.

## Current limitations and improvement opportunities

- Declarative syntax has a defined supported subset. Host parser APIs cover
  cases outside it, but require Rust implementation.
- Some project commands operate on declarative definitions directly. Rust-only
  language models are available to generated hosts, while generic tooling that
  needs a Rust-authored model requires an explicit model-aware workflow.
- Backend contracts and pass planning are generic, but useful output requires a
  backend implementation with its own validation and integration tests.
- The language server's generic navigation and rename depend on generic symbol
  resolution. Host-specific resolvers need an explicit safe edit contract before
  rename can support them.
- Broader consumer examples, language-server features, continuous integration
  across supported platforms, and measured performance baselines would improve
  confidence and usability. The current example covers one declarative
  interpreted language; Rust API-only, combined authoring, and structurally
  different source-language consumers remain to be added.

## Verification

The workspace checks completed on 2026-11-03:

- `cargo fmt --all -- --check` passed.
- `cargo test --workspace --offline --locked` passed, including fresh generated
  project builds and the LSP protocol tests.
- `cargo clippy --workspace --all-targets --offline --locked -- -D warnings`
  passed.
- Both rename protocol cases passed after the final scope-path change.
- The independent example passed `cargo test --offline --locked` and strict
  Clippy. Its generated CLI `check --project` succeeded, and `run --project`
  printed `total = 42`.

The generated Tree-sitter grammar has not been validated with the Tree-sitter
CLI. Run that check before relying on generated grammars in an editor.
