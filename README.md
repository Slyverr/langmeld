# Langmeld

Langmeld requires Rust 1.88 or newer (Edition 2024). The CI workflow runs on
Linux, macOS, and Windows.

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
host-authored lowering pipeline. The generated `run` command passes a
`RuntimeContext` with the validated language, source snapshots, parsed
programs, and workspace analysis to the host's `run_sources` hook; language
authors implement runtime behavior.
Interactive project and backend configuration is available.
The standalone `langmeld-lsp` crate provides initialize/shutdown and
open-document diagnostics over LSP. Start it with
`cargo run -p langmeld-lsp -- --language language/language.lm` (or
`--project langmeld.json`). It supports versioned full/ranged document edits,
diagnostics, global literal-keyword completion, parsed-construct document
symbols, schema-based hover, and semantic tokens. Definition, references, and
safe rename work across currently open documents when the language declares
symbol metadata. Rename rejects unresolved references and same-scope name
collisions, returns no partial edits when analysis is limited, and caps names
and edits; callback-provided symbol resolution is not renameable. It also
suggests enum values and in-scope symbols in captured fields,
offers parser-context enum values, simple child-construct snippets, and
parser-derived continuations for incomplete patterns with unambiguous literal
and field sequences. For declarative languages using Langmeld's built-in
lexer, it also returns source-formatting edits and a versioned format code
action. Embedded Rust hosts can provide a formatter for custom syntax; Langmeld
validates its result by reparsing and analyzing the edited open workspace before
returning an edit. Embedded Rust hosts can also register bounded, typed
completion providers. In project mode, clients that support dynamic watched-file
registration receive watches for language definitions, the manifest, and the
definition lock; valid changes reload the language and reparse open documents.
Navigation does not yet discover unopened project files. The browser language
adapter is described below; the LSP server itself does not run in WebAssembly.
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
represented. CI is configured to compile the generated Ledger grammar and a
representative grammar with enums, lists, maps, and nested properties using the
Tree-sitter CLI; it runs highlight queries for both and parses the complex
fixture. These checks validate grammar and query syntax, not semantic
equivalence with Langmeld. The library also exposes a structural formatter
that preserves comments and strings, normalizes CRLF to LF, and supports space
or tab indentation for valid programs.

## Compatibility

Langmeld is pre-1.0. Public Rust APIs and generated host code may change in a
minor release, so rebuild generated hosts when upgrading; there is no stable
binary plugin ABI. The Rust crate version does not version language or project
data formats.

Project manifests and definition lockfiles have their own schema versions.
The loader supports project manifest versions 1 and 2 and lockfile version 1,
and rejects unsupported versions. It preserves existing manifest versions;
there is no automatic schema migration. Generated files under
`.langmeld/generated/` are derived and can be rebuilt from project sources.

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

## Browser and WebAssembly adapter

`langmeld init --wasm` adds an optional browser adapter to a generated Rust
language project; the interactive initializer also offers it. This runs the
Langmeld parser and generic editor analysis inside WebAssembly. It is a runtime
adapter, not a compiler backend that translates a user's language into
WebAssembly.

The generated `LanguageHost` can load an in-memory `.lm` root and module bundle
or canonical `LanguageSpec` JSON for generic parsing and analysis. Its
`fromProject()` constructor instead uses the generated project's compiled
language model and Rust callbacks, including custom parser behavior. The
adapter returns parse diagnostics, generic syntax data, editor classifications,
completion context, and cross-file generic references from explicitly
provided source text. It does not read files or resolve imports; the caller
supplies all modules and workspace files. Source ranges are UTF-8 byte offsets,
and opaque host values are not serialized. Generated projects include
`README-wasm.md` with build and JavaScript binding instructions.

The `langmeld` facade crate exposes the core schema, expression APIs, and
language tooling under `langmeld::{core, expression, language}`. Its default
feature set is empty; it does not advertise optional packages that are still
planned.

## Examples

[`examples/declarative-interpreter`](examples/declarative-interpreter) is an
independent consumer project. Its modular `.lm` definition declares a small
arithmetic language with a separate subtraction extension, Langmeld resolves
its variable references, and a Rust host interprets the parsed programs. Its
generated editor and typed-AST artifacts track the extension. Run its tests and
CLI from the example directory; the example README has the commands and output.

[`examples/rust-functions`](examples/rust-functions) authors a language model
with the Rust API, parses zero-argument and parameterized function signatures,
and lowers them to compilable Rust from a library-only project.

[`examples/combined-brief`](examples/combined-brief) combines a Rust-authored
base model, a modular `.lm` extension, and a Rust host that renders Markdown.

[`examples/c-signatures`](examples/c-signatures) declares a focused C prototype
model in `.lm`, parses source through a host-registered Rust parser, and emits
Rust FFI declarations.

## Workspace checks

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Project guidance

- [CONTRIBUTING.md](CONTRIBUTING.md) describes the development workflow,
  verification, and commit conventions.
- [AGENTS.md](AGENTS.md) gives coding agents the repository-specific rules and
  links to the project-local Rust skills under `.agents/skills/`.

## License

Licensed under either of these terms, at your option:

- [MIT](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)
