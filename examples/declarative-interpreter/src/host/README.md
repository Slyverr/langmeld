# Rust host modules

This generated module tree follows the language definition and Langmeld's extension APIs:

- `parser.rs` contains one stub per declared custom parser. Return source-spanned generic fields for shared Langmeld services, and attach a native Rust AST with `CustomConstructMatch::with_host_data(...)` when your language needs its own representation.
- `lexer.rs` is the host's optional custom-lexer implementation. Start from `langmeld::language::lex_source(source)` when only some lexical rules need adapting.
- `analysis.rs` contains semantic, symbol-resolution, and workspace behavior stubs plus checked registries.
- `backend.rs` registers the project's Rust-defined single-file and workspace backends. This host-owned module is not generated from `.langmeld` declarations.
- `runtime.rs` binds parser callbacks and an optional custom lexer to a validated language.
- `mod.rs` connects the Rust modules.

At a custom parser's byte offset, return `Ok(None)` only when the input does not
start that construct; this lets another construct parser or the declarative
pattern try. Once the input has the construct's recognizable prefix, return a
diagnostic for malformed syntax instead of falling through. Successful matches
must include the construct's declared required fields and UTF-8 byte spans.

`langmeld init` seeds these modules into `src/host/`. `langmeld generate`
writes deterministic refresh references under `.langmeld/generated/host/`;
`langmeld generate --install-host` safely refreshes the project modules while
preserving marked callback implementations.

The starter exposes `pub mod host;`, binds parser and lexer callbacks through
`host::runtime::configured_parser`, and calls the checked analysis registries
from its project-aware CLI. Add backend implementations to the user-owned
`src/host/backend.rs`; `.langmeld` declarations do not select executable backends.

The generated host tree contains callbacks derived from the language definition. Keep implementation code in the project host until Langmeld provides its safe install and refresh workflow.
