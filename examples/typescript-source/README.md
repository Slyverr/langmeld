# TypeScript Source Index

This adapter parses actual TypeScript with the TypeScript 5.9 compiler API and
maps top-level function declarations, parameters, type annotations, async
status, and direct calls to same-file or imported functions into Langmeld's
generic AST. It then uses Langmeld's generated Rust views, generic reference
analysis, diagnostics, and LSP definition navigation. TypeScript compiler
UTF-16 offsets are converted to Langmeld's UTF-8 byte spans.

The adapter parses source but never executes it. One long-lived Bun worker runs
the pinned TypeScript compiler dependency for each parser instance; set
`LANGMELD_BUN` to select a different Bun executable. Requests and responses use
newline-delimited JSON with source and response size limits.

```sh
bun install --cwd examples/typescript-source --frozen-lockfile
cargo run --manifest-path examples/typescript-source/Cargo.toml --bin typescript-source -- examples/typescript-source/main.ts
cargo run --manifest-path examples/typescript-source/Cargo.toml --bin typescript-source-lsp
cargo run --manifest-path examples/typescript-source/Cargo.toml --bin typescript-source-lsp -- \
  --project-root examples/typescript-source/project
cargo test --manifest-path examples/typescript-source/Cargo.toml --offline --locked
cargo run -p langmeld-cli -- generate --project-root examples/typescript-source
cargo run -p langmeld-cli -- generate --project-root examples/typescript-source --check
```

This is a focused source index, not a TypeScript type checker or full project
service. Project mode loads the `.ts` files selected by `langmeld.json` and resolves
named and default imports of exported functions across that set, including import
aliases. Default imports currently support named `export default function`
declarations. The LSP example loads its source set from the project's
`langmeld.json` before serving requests.
It checks relative `.ts`/`index.ts` paths and assigns each source file a module
namespace in the generic syntax tree, so two modules can export the same function
name and imported calls still navigate to the selected module.
Namespace imports, other default-export forms, re-exports, classes, methods,
overload resolution, TypeScript project references, and general-purpose error
recovery are not modeled. During
incomplete edits, the compiler AST still contributes valid functions alongside
syntax diagnostics so LSP navigation can continue.

The adapter uses an opaque source token because the TypeScript compiler owns
lexing and parsing. Generic LSP semantic tokens and the generated lexical editor
grammar therefore do not provide TypeScript syntax highlighting; a host lexer
must return TypeScript token spans for that editor feature.
