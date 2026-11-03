# Langmeld

Langmeld is an early-stage Rust language framework. The workspace currently
contains `langmeld-core` for source snapshots, stable source identities, byte
spans, structured diagnostics, and validated language definitions, plus
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
and workspace type-analysis passes. The project CLI, language facade, LSP, and
WASM adapter remain to be ported. `langmeld-language` generates deterministic
Rust source for typed AST views and schema reconstruction, plus metadata,
reference docs, lexical-only TextMate rules, and conventional editor
configuration. A generic artifact pass pipeline plans lowering dependencies
and runs only the passes needed for requested outputs; the host owns its IR
values and pass context. Single-program backend contracts validate descriptors,
settings, required IR, artifacts, and source maps. Workspace backends apply the
same checks across multiple sources and require explicit source indexes for
ambiguous mappings. The optional Tree-sitter exporter emits a grammar for
compatible declarative syntax and reports when a definition cannot be
represented. Its grammar has not been checked with the Tree-sitter CLI in this
environment. Source formatting remains in progress.

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
