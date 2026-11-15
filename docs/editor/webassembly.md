# WebAssembly

The `langmeld-core`, `langmeld-expression`, `langmeld-language`,
`langmeld-project`, `langmeld-textmate`, and `langmeld-tree-sitter` crates
compile for `wasm32-unknown-unknown`. A browser host can use
`InMemoryDefinitionResolver` for embedded or virtual definition sources. The
manifest and directory loaders in `langmeld-project` still require native
filesystem access. Langmeld does not provide JavaScript bindings or a browser
editor adapter, and the LSP adapter currently serves protocol connections
rather than browser workers.

## Embedded language definitions

For a manifest-backed project, `langmeld-build` resolves imports at build time
and generates a root key and source table. A browser host can validate that
table without filesystem access at runtime. Add it as a build dependency; this
works without enabling typed Rust bindings:

```toml
[build-dependencies]
langmeld-build = "0.16"
```

Then use this `build.rs`:

```rust,ignore
fn main() {
    if let Err(error) = langmeld_build::generate_embedded_sources() {
        panic!("{error}");
    }
}
```

Include the generated list in the host and load it into the resolver:

```rust,ignore
include!(concat!(env!("OUT_DIR"), "/langmeld_sources.rs"));

use langmeld_language::LanguageSpec;
use langmeld_project::{LanguageSpecLoadError, load_language_spec_from_sources};

fn embedded_language() -> Result<LanguageSpec, LanguageSpecLoadError> {
    load_language_spec_from_sources(LANGMELD_ROOT_KEY, LANGMELD_SOURCES)
}
```

The build script validates the project and tracks the manifest and imported
sources. Use `InMemoryDefinitionResolver` directly when sources are supplied or
edited at runtime, or when they need custom source names.

The resolver does not read files or parse the manifest. A browser integration
must provide its own JavaScript/WASM bindings and decide how source edits,
diagnostics, and editor requests cross that boundary. The downstream consumer
fixture exercises this embedded loading path in the native test run and
compiles it for `wasm32-unknown-unknown` in CI.
