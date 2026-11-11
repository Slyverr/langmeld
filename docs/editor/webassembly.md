# WebAssembly

The `langmeld-core`, `langmeld-expression`, `langmeld-language`,
`langmeld-project`, `langmeld-textmate`, and `langmeld-tree-sitter` crates
compile for `wasm32-unknown-unknown`. A browser host can use
`InMemoryDefinitionResolver` for embedded or virtual definition sources. The
manifest and directory loaders in `langmeld-project` still require native
filesystem access. Langmeld does not provide JavaScript bindings or a browser
editor adapter, and the LSP adapter currently serves protocol connections
rather than browser workers.
