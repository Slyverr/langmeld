# Langmeld

Langmeld is an early-stage Rust language framework. The workspace currently
contains the `langmeld` facade and `langmeld-core` for source snapshots, stable
source identities, byte spans, structured diagnostics, and validated language
definitions, plus
`langmeld-expression` for configurable expression parsing and host-defined type
and runtime rules, and `langmeld-language` for source-spanned tokens, the
built-in lexer, custom lexer contracts, and a generic parser that builds typed
ASTs, lossless syntax trees, and editor classifications. The readable `.lm`
definition format uses the same generic parser, rebuilds its embedded grammar
from modular sources, and supports in-memory modules and safe relative file
imports. Prepared grammars compile declarative patterns once and provide
token-based block-child candidate hints. Generic analysis resolves symbols and
references across files and reports nested value-type facts. Checked host
behavior registries provide semantic callbacks, replacement symbol resolvers,
and workspace type-analysis passes. The Clap-based CLI provides `init`,
`configure`, `check`, `host`, and `generate` commands for Rust hosts with
modular `.lm` definitions. Generation rebuilds language tooling and includes
Rust parser, lexer, and
analysis callback scaffolds under `.langmeld/generated/host/`; callback bodies
are declaration-driven and compile in a generated Rust project. The
`langmeld generate --install-host` option safely wires these modules into
`src/host/` and preserves marked callback implementations while refreshing
them. `langmeld init` also writes a strict `langmeld.json` project manifest,
and the language API can load explicit or discovered program sources from it.
Interactive setup can include a Clap-based language CLI with `check` and `run`
commands, or omit it with `--library-only`. The generated `run` command calls
the host's `run_sources` hook; language authors implement its runtime behavior.
Interactive project configuration is available; backend build commands are
pending.
LSP and WASM adapters are also pending.
`langmeld-language` generates deterministic Rust source for typed AST views and
schema reconstruction, plus metadata,
reference docs, lexical-only TextMate rules, and conventional editor
configuration. A generic artifact pass pipeline plans lowering dependencies
and runs only the passes needed for requested outputs; the host owns its IR
values and pass context. Single-program backend contracts validate descriptors,
settings, required IR, artifacts, and source maps. Workspace backends apply the
same checks across multiple sources and require explicit source indexes for
ambiguous mappings. The optional Tree-sitter exporter emits a grammar for
compatible declarative syntax and reports when a definition cannot be
represented. Its grammar has not been checked with the Tree-sitter CLI in this
environment. The library also exposes a structural formatter that preserves
comments and strings, normalizes CRLF to LF, and supports space or tab
indentation for valid programs.

## Bootstrap a language project

Run `cargo run -p langmeld-cli -- init` in a terminal for interactive setup.
For automation, pass the project details and disable prompts:

```sh
cargo run -p langmeld-cli -- init --path ./my-language --name MyLanguage --no-interactive
cargo run -p langmeld-cli -- generate --project-root ./my-language --check
```

The generated project's `README.md` describes its structure and commands.
From a generated project, update source discovery interactively with
`langmeld configure`, or pass options such as `--source-root`, `--source`, and
`--source-extension`. Configuration resolves the proposed language and source
set before atomically updating `langmeld.json`.
Use `langmeld check --project langmeld.json` (or its `validate` alias) to parse
the configured sources and run generic workspace analysis. This command cannot
register a language's Rust callbacks; callback-dependent languages must use a
host integration that binds those implementations.
For a project with a Rust binary target, `langmeld host -- --help` forwards
arguments to its `package.default-run` target (or its only binary target).

The `langmeld` facade crate exposes the core schema, expression APIs, and
language tooling under `langmeld::{core, expression, language}`. Its default
feature set is empty; it does not advertise optional packages that are still
planned.

## Workspace checks

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Project guidance

- [CONTRIBUTING.md](CONTRIBUTING.md) describes the staged workflow, verification,
  and commit conventions.
- [AGENTS.md](AGENTS.md) gives coding agents the repository-specific rules and
  links to the project-local Rust skills under `.agents/skills/`.

See [REPORT.md](REPORT.md) for the current implementation snapshot and findings.

## License

Licensed under either of these terms, at your option:

- [MIT](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)
