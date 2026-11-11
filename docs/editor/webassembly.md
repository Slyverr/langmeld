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

For a browser or another host without project filesystem access, embed the
definition files in the Rust application and insert each source into an
`InMemoryDefinitionResolver`. Imports are resolved from the inserted
project-relative keys, so imported files must also be included:

```rust,no_run
use langmeld_core::SourceFile;
use langmeld_language::parse_language_definition_project;
use langmeld_project::InMemoryDefinitionResolver;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut resolver = InMemoryDefinitionResolver::new();
    resolver.insert(
        "language/main.langmeld",
        SourceFile::new(
            "language/main.langmeld",
            r#"language "Mini" { import "constructs/statements.langmeld"; }"#,
        ),
    )?;
    resolver.insert(
        "language/constructs/statements.langmeld",
        SourceFile::new(
            "language/constructs/statements.langmeld",
            r#"construct "say" { syntax { literal "say"; string "message"; literal ";"; } }"#,
        ),
    )?;

    let root = resolver.load("language/main.langmeld")?;
    let definition = parse_language_definition_project(root, &resolver);
    let _language = definition.language.ok_or("invalid language definition")?;

    Ok(())
}
```

The resolver does not read files or parse the manifest. A browser integration
must provide its own JavaScript/WASM bindings and decide how source edits,
diagnostics, and editor requests cross that boundary. The downstream consumer
fixture exercises this embedded loading path in the native test run and
compiles it for `wasm32-unknown-unknown` in CI.
