# Rust host modules

This generated module tree follows the language definition and Langmeld's extension APIs:

- `parser.rs` contains stubs for declared custom construct parsers and expression atom parsers. Return source-spanned generic fields or `CustomAtomToken` values that identify the host-owned syntax.
- `lexer.rs` is the host's optional custom-lexer implementation. Use `lex_source(source)` for the legacy `#` policy or `lex_source_with_lexical_spec(source, &policy)` when adapting declared comment rules.
- `analysis.rs` contains semantic, symbol-resolution, and workspace behavior stubs plus checked registries.
- `backend.rs` registers the project's Rust-defined single-file and workspace backends. This host-owned module is not generated from `.langmeld` declarations.
- `runtime.rs` binds parser and expression callbacks plus an optional custom lexer to a validated language.
- `mod.rs` connects the Rust modules.

At a custom parser's byte offset, return `Ok(None)` only when the input does not
start that construct; this lets another construct parser or the declarative
pattern try. Once the input has the construct's recognizable prefix, return a
diagnostic for malformed syntax instead of falling through. Successful matches
must include the construct's declared required fields and UTF-8 byte spans.

`langmeld init` seeds these modules into `src/host/`. For declarative
projects, `langmeld generate --install-host` refreshes these modules while
preserving marked callback implementations. Generated Rust host CLIs use
`cargo run -- generate` to refresh references under
`.langmeld/generated/host/`. Library-only projects can call
`generate_host_scaffold` and the safe artifact writer from Rust. The generic
CLI does not load Rust project code.

The starter exposes `pub mod host;`, binds parser and lexer callbacks through
`host::runtime::configured_parser`, and calls the checked analysis registries
from its project-aware CLI. Add backend implementations to the user-owned
`src/host/backend.rs`; `.langmeld` declarations do not select executable backends.

The generated host tree contains callback references derived from the language
definition. Keep implementations in `src/host/`; files under
`.langmeld/generated/host/` are refreshable references.
