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
`configure`, `check`, `format`, `lock`, `extension`, `host`, and
`generate`, `inspect`, `constructs`, `describe`, and `language` commands
for Rust hosts with modular `.lm` definitions. Generation rebuilds language
tooling and emits Rust parser, lexer, and analysis callback scaffolds under
`.langmeld/generated/host/`; callback bodies are declaration-driven and
compile in a generated Rust project. `langmeld init` seeds the same host
modules in `src/host/`, and `langmeld generate --install-host` safely refreshes
them while preserving marked callback implementations. `langmeld init` also
writes a strict `langmeld.json` project manifest,
and the language API can load explicit or discovered program sources from it.
Interactive setup can include a Clap-based language CLI with `check`, `build`,
and `run` commands, or omit it with `--library-only`. The generated `build`
command dispatches to host-registered single-file or workspace backends, uses
project defaults from `langmeld.json`, and writes validated artifacts and
optional source-map sidecars under `target/`. Authors implement and register
their own backends in `src/host/backend.rs`; custom IR still needs a
host-authored lowering pipeline. The generated `run` command calls the host's
`run_sources` hook; language authors implement its runtime behavior.
Interactive project and backend configuration is available.
The standalone `langmeld-lsp` crate provides initialize/shutdown and
open-document diagnostics over LSP. Start it with
`cargo run -p langmeld-lsp -- --language language/language.lm` (or
`--project langmeld.json`). It supports versioned full/ranged document edits,
diagnostics, global literal-keyword completion, parsed-construct document
symbols, schema-based hover, and semantic tokens. Definition and references
work across currently open documents when the language declares symbol
metadata. It also suggests enum values and in-scope symbols in captured fields,
offers parser-context enum values, simple child-construct snippets, and
parser-derived continuations for incomplete patterns with unambiguous literal
and field sequences. Embedded Rust hosts can register bounded, typed completion
providers. Unopened project-file navigation, project watchers, and WASM adapters
remain pending.
Rust hosts can embed the server with `langmeld_lsp::serve_configured`, passing
a configured parser and registered analysis callbacks. The standalone LSP
binary remains for declarative-only languages.
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
set before atomically updating `langmeld.json`. The project can compose local,
data-only `.lm` or JSON extensions; versioned packages declare SemVer language
compatibility and package dependencies. Run `langmeld lock` to pin the resolved
definition files. A changed language definition then requires refreshing the
lock before project checks will proceed.
Manage declarations with `langmeld extension list`, `add`, `update`, and
`remove`. The list command shows manifest declarations; mutations validate the
effective language and refresh `langmeld.lock` as part of the update. For
example, add an unversioned path with `langmeld extension add --path-only
language/extensions/ui.lm`, or a local package with `--id`, `--version`,
`--path`, and `--language-version`. Package dependencies use repeated
`--depends ID@VERSION-REQUIREMENT` options. Nothing is downloaded or executed.
`langmeld format --language language/language.lm --source examples/demo.lang`
prints the formatted source. Add `--write` to update the file; invalid source
is reported without writing.
Use `langmeld inspect --language language/language.lm` to view language
metadata and imported definition files. `constructs` lists declared
constructs, and `describe --construct ID` prints one construct's details;
these commands also have `langmeld language inspect|constructs|describe`
aliases.
Use `langmeld check --project langmeld.json` (or its `validate` alias) to parse
the configured sources and run generic workspace analysis. This command cannot
register a language's Rust callbacks; callback-dependent languages must use a
host integration that binds those implementations.
For a project with a Rust binary target, `langmeld host -- --help` forwards
arguments to its `package.default-run` target (or its only binary target).
After registering Rust backends in the generated host, run
`langmeld host -- configure --interactive` to save backend defaults, or use
`--backend`, `--format`, `--target`, and `--set KEY=VALUE` for scripted
updates. These preferences are stored under `build` in `langmeld.json` and
are consumed by the host's `build` command.

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
